use bevy::prelude::*;
use nanorand::Rng;

use game_core::prelude::*;
use grids::prelude::ObstacleGrid;
use logging::prelude::*;
use persistence::{
    prelude::{AppGameLoadSaveExtension, CollectSave, GameDbHelpers, LoadContext, SaveWriter},
    rusqlite,
};
use states::prelude::*;
use wisps::summoning::{BuilderSummoning, SpawnTempo, Summoning, SummoningRuntime, SummoningRuntimeState};

use super::spawning::BuilderWisp;

pub struct SummoningPlugin;
impl Plugin for SummoningPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(OnEnter(MapLoadingStage::LoadResources), |mut commands: Commands| { commands.insert_resource(SummoningClock::default());})
            .add_systems(Update, tick_active_summoning_system.run_if(in_state(GameState::Running)))
            .add_observer(on_summoning_activation_event_do_so)
            .add_observer(on_builder_add_spawn_summoning)
            .add_systems(CollectSave, collect_summonings)
            .add_systems(CollectSave, collect_summoning_clock)
            .register_loader(MapLoadingStage::LoadResources, "summonings", load_summonings)
            .register_loader(MapLoadingStage::LoadResources, "stats_summoning_clock", load_summoning_clock)
            ;
    }
}

// --------------- SUMMONING ENTITIES AND RUNTIME ---------------
#[derive(Component, Default)]
pub(crate) struct SummoningMarkerActive;

#[derive(Resource, Default, Clone)]
struct SummoningClock(f32);

fn collect_summoning_clock(clock: Res<SummoningClock>, mut save: SaveWriter) {
    let value = clock.0;
    save.submit(move |tx| {
        tx.save_stat("summoning_clock", value)?;
        Ok(())
    });
}

fn load_summoning_clock(ctx: &mut LoadContext) -> rusqlite::Result<()> {
    let clock_value = ctx.conn.get_stat("summoning_clock").unwrap_or(0.0);
    ctx.insert_resource(SummoningClock(clock_value));
    Ok(())
}

fn collect_summonings(
    summonings: Query<(Entity, &Summoning, &SummoningRuntime, Has<SummoningMarkerActive>)>,
    mut save: SaveWriter,
) {
    if summonings.is_empty() { return; }
    let rows: Vec<(i64, String, i32, f32, bool)> = summonings
        .iter()
        .map(|(entity, summoning, runtime, is_active)| {
            let summoning_json = serde_json::to_string(summoning)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
                .unwrap_or_default();
            (
                entity.index_u32() as i64,
                summoning_json,
                runtime.produced,
                runtime.next_spawn_time,
                is_active,
            )
        })
        .collect();
    save.submit(move |tx| {
        for (id, summoning_json, produced, next_spawn_time, is_active) in rows {
            tx.register_entity(id)?;
            tx.execute(
                "INSERT OR REPLACE INTO summonings (id, summoning_json, produced, next_spawn_time, is_active) VALUES (?1, ?2, ?3, ?4, ?5)",
                (id, summoning_json, produced, next_spawn_time, if is_active { 1 } else { 0 }),
            )?;
        }
        Ok(())
    });
}

fn load_summonings(ctx: &mut LoadContext) -> rusqlite::Result<()> {
    let mut stmt = ctx.conn.prepare(
        "SELECT id, summoning_json, produced, next_spawn_time, is_active FROM summonings",
    )?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let old_id: i64 = row.get(0)?;
        let summoning_json: String = row.get(1)?;
        let produced: i32 = row.get(2)?;
        let next_spawn_time: f32 = row.get(3)?;
        let is_active: i32 = row.get(4)?;

        let summoning: Summoning = serde_json::from_str(&summoning_json)
            .map_err(|e| rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e)))?;

        let Some(entity) = ctx.entity(old_id) else {
            Log::warn().dev().tag(Tag::GameLoad).message(format!("Summoning with old ID {old_id} has no corresponding new entity"));
            continue;
        };
        let builder = BuilderSummoning::new(summoning).with_runtime(SummoningRuntimeState {
            produced,
            next_spawn_time,
            is_active: is_active != 0,
        });
        ctx.insert(entity, builder);
    }
    Ok(())
}

fn on_builder_add_spawn_summoning(
    trigger: On<Add, BuilderSummoning>,
    mut commands: Commands,
    builders: Query<&BuilderSummoning>,
) {
    let entity = trigger.entity;
    let Ok(builder) = builders.get(entity) else { return; };

    let mut entity_commands = commands.entity(entity);

    // Restore runtime state if loading from save
    if let Some(runtime) = &builder.runtime {
        entity_commands.insert(SummoningRuntime {
            produced: runtime.produced,
            next_spawn_time: runtime.next_spawn_time,
        });

        if runtime.is_active {
            entity_commands.insert(SummoningMarkerActive);
        }
    }

    // Insert the actual Summoning component and remove builder
    entity_commands
        .remove::<BuilderSummoning>()
        .insert(builder.summoning.clone());
}

fn tick_active_summoning_system(
    mut commands: Commands,
    obstacle_grid: Res<ObstacleGrid>,
    mut clock: ResMut<SummoningClock>,
    time: Res<Time>,
    mut summoning: Query<(&Summoning, &mut SummoningRuntime), With<SummoningMarkerActive>>,
) {
    clock.0 += time.delta_secs();
    let now = clock.0;
    let mut rng = nanorand::tls_rng();

    for (summoning, mut runtime) in &mut summoning {
        // Check if limit is reached(if set)
        let remaining = summoning.limit_count.map(|m| m.saturating_sub(runtime.produced)).unwrap_or(i32::MAX);
        if remaining <= 0 { continue; }

        // Wait until due
        if now < runtime.next_spawn_time { continue; }

        match summoning.tempo {
            SpawnTempo::Continuous { seconds, jitter, bulk_count } => {
                let to_spawn: i32 = std::cmp::min(bulk_count, remaining);
                if to_spawn <= 0 { continue; }
                for _ in 0..(to_spawn as usize) {
                    let grid_coords = summoning.area.get_random_coord(&obstacle_grid, &mut rng);
                    let wisp_type = summoning.get_random_wisp_type(&mut rng);
                    commands.spawn(BuilderWisp::new(wisp_type, grid_coords));
                }
                runtime.produced = runtime.produced.saturating_add(to_spawn);
                let j = if jitter > 0.0 { (rng.generate::<f32>() * 2.0 - 1.0) * jitter } else { 0.0 };
                runtime.next_spawn_time = now + (seconds + j);
            }
        }
    }
}

fn on_summoning_activation_event_do_so(
    trigger: On<DynamicGameEvent>,
    mut commands: Commands,
    summonings: Query<(Entity, &Summoning), Without<SummoningMarkerActive>>,
) {
    let event = &trigger.event().0;
    for (entity, summoning) in summonings.iter() {
        if event != &summoning.activation_event { continue; }
        commands.entity(entity).insert(SummoningMarkerActive);
        Log::info().player().tag(Tag::Wave).message(format!("Summoning '{}' activated", summoning.id_name));
    }
}
