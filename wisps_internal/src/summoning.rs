use bevy::prelude::*;
use nanorand::Rng;

use game_core::prelude::*;
use grids::prelude::ObstacleGrid;
use logging::prelude::*;
use persistence::{prelude::*, rusqlite};
use states::prelude::*;
use wisps::summoning::{BuilderSummoning, SpawnTempo, Summoning, SummoningRuntime, SummoningSaveData};

use super::spawning::BuilderWisp;

pub struct SummoningPlugin;
impl Plugin for SummoningPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(OnEnter(MapLoadingStage::LoadResources), |mut commands: Commands| { commands.insert_resource(SummoningClock::default());})
            .add_systems(Update, tick_active_summoning_system.run_if(in_state(GameState::Running)))
            .add_observer(on_summoning_activation_event_do_so)
            .add_observer(on_builder_add_spawn_summoning)
            .register_db_loader::<BuilderSummoning>(MapLoadingStage::LoadResources)
            .register_db_loader::<SummoningClock>(MapLoadingStage::LoadResources)
            .register_db_saver(on_game_save_collect_summonings)
            .register_db_saver(SummoningClock::on_game_save_collect_summoning_clock);
    }
}

// --------------- SUMMONING ENTITIES AND RUNTIME ---------------
#[derive(Component, Default)]
pub(crate) struct SummoningMarkerActive;

#[derive(Resource, Default, Clone, SSS)]
struct SummoningClock(f32);
impl Saveable for SummoningClock {
    fn save(self, tx: &rusqlite::Transaction) -> rusqlite::Result<()> {
        tx.save_stat("summoning_clock", self.0)?;
        Ok(())
    }
}

impl Loadable for SummoningClock {
    fn load(ctx: &mut LoadContext) -> rusqlite::Result<LoadResult> {
        let clock_value = ctx.conn.get_stat("summoning_clock").unwrap_or(0.0);
        ctx.commands.insert_resource(SummoningClock(clock_value));
        Ok(LoadResult::Finished)
    }
}
impl SummoningClock {
    fn on_game_save_collect_summoning_clock(
        mut commands: Commands,
        clock: Res<SummoningClock>,
    ) {
        commands.queue(SaveableBatchCommand::from_single(clock.clone()));
    }
}

fn on_game_save_collect_summonings(
    mut commands: Commands,
    summonings: Query<(Entity, &Summoning, &SummoningRuntime, Has<SummoningMarkerActive>)>,
) {
    if summonings.is_empty() { return; }
    let batch = summonings.iter().map(|(entity, summoning, runtime, is_active)| {
        let save_data = SummoningSaveData {
            entity,
            produced: runtime.produced,
            next_spawn_time: runtime.next_spawn_time,
            is_active,
        };
        BuilderSummoning::new_for_saving(summoning.clone(), save_data)
    }).collect::<SaveableBatchCommand<_>>();
    commands.queue(batch);
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
    if let Some(save_data) = &builder.save_data {
        entity_commands.insert(SummoningRuntime {
            produced: save_data.produced,
            next_spawn_time: save_data.next_spawn_time,
        });

        if save_data.is_active {
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
