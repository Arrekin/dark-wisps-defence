use bevy::prelude::*;
use nanorand::Rng;

use game_core::prelude::*;
use grids::prelude::ObstacleGrid;
use logging::prelude::*;
use persistence::{
    prelude::{AppGameLoadSaveExtension, CollectSave, GameDbHelpers, LoadContext, SaveContext, SaveWriter},
    rusqlite,
};
use states::prelude::*;
use wisps::summoning::*;

use super::spawning::BuilderWisp;

pub struct SummoningPlugin;
impl Plugin for SummoningPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(OnEnter(MapLoadingStage::LoadResources), |mut commands: Commands| { commands.insert_resource(SummoningClock::default());})
            .add_systems(Update, tick_active_summoning_system.run_if(in_state(GameState::Running)))
            .add_observer(on_insert_summoning_state_sync_markers)
            .add_observer(on_moment_happened_activate_summoning)
            .add_observer(on_builder_add_spawn_summoning)
            .add_observer(moment_attach_self_trigger_to_parent::<MomentSummoningStarted, SummoningActivatedEvent>)
            .add_observer(moment_attach_self_trigger_to_parent::<MomentSummoningExhausted, SummoningExhaustedEvent>)
            .add_systems(CollectSave, collect_summonings)
            .add_systems(CollectSave, collect_summoning_clock)
            .register_loader(MapLoadingStage::SpawnMapElements, "summonings", load_summonings)
            .register_loader(MapLoadingStage::LoadResources, "stats_summoning_clock", load_summoning_clock)
            .register_moment_persistence::<MomentSummoningStarted>()
            .register_moment_persistence::<MomentSummoningExhausted>()
            ;
    }
}

// --------------- SUMMONING ENTITIES AND RUNTIME ---------------

/// On every `Insert<SummoningState>`, swap the marker components to match
/// the new state. Markers are never inserted directly — this is the single
/// entry point that derives them. Works identically on fresh spawn (the
/// builder inserts `SummoningState`) and on load (the builder inserts the
/// restored state).
fn on_insert_summoning_state_sync_markers(
    trigger: On<Insert, SummoningState>,
    states: Query<&SummoningState>,
    mut commands: Commands,
) {
    let entity = trigger.entity;
    let Ok(new_state) = states.get(entity) else { return };
    let mut ec = commands.entity(entity);
    ec.remove::<(SummoningInactive, SummoningActive, SummoningExhausted)>();
    match new_state {
        SummoningState::Inactive => { ec.insert(SummoningInactive); }
        SummoningState::Active => { ec.insert(SummoningActive); }
        SummoningState::Exhausted => { ec.insert(SummoningExhausted); }
    }
}

#[derive(Resource, Default, Clone)]
struct SummoningClock(f32);

fn collect_summoning_clock(clock: Res<SummoningClock>, save_ctx: Res<SaveContext>, mut save: SaveWriter) {
    let value = if save_ctx.save_as_scenario { 0.0 } else { clock.0 };
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
    save_ctx: Res<SaveContext>,
    summonings: Query<(Entity, &Summoning, &SummoningState, &SummoningRuntime, Option<&MomentOfInterest>)>,
    mut save: SaveWriter,
) {
    if summonings.is_empty() { return; }

    struct Snapshot {
        id: i64,
        summoning: Summoning,
        state: SummoningState,
        activated_by: Option<i64>,
        produced: i32,
        next_spawn_time: f32,
    }

    let snapshots: Vec<Snapshot> = summonings
        .iter()
        .map(|(entity, summoning, state, runtime, activated_by)| {
            let (state, produced, next_spawn_time) = if save_ctx.save_as_scenario {
                (SummoningState::Inactive, 0, 0.0)
            } else {
                (*state, runtime.produced, runtime.next_spawn_time)
            };
            Snapshot {
                id: entity.index_u32() as i64,
                summoning: summoning.clone(),
                state,
                activated_by: activated_by.map(|ab| ab.0.index_u32() as i64),
                produced,
                next_spawn_time,
            }
        })
        .collect();

    save.submit(move |tx| {
        for snap in &snapshots {
            tx.register_entity(snap.id)?;
            if let Some(ab_id) = snap.activated_by {
                tx.register_entity(ab_id)?;
            }

            tx.execute(
                "INSERT OR REPLACE INTO summonings (id, id_name, state, activated_by, tempo_kind, limit_count, area_kind, produced, next_spawn_time) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                rusqlite::params![
                    snap.id,
                    snap.summoning.id_name,
                    snap.state.as_ref(),
                    snap.activated_by,
                    snap.summoning.tempo.as_ref(),
                    snap.summoning.limit_count,
                    snap.summoning.area.as_ref(),
                    snap.produced,
                    snap.next_spawn_time,
                ],
            )?;

            save_tempo(tx, snap.id, &snap.summoning.tempo)?;
            save_area(tx, snap.id, &snap.summoning.area)?;
            save_wisp_types(tx, snap.id, &snap.summoning.wisp_types)?;
        }
        Ok(())
    });
}

// --------------- SpawnTempo persistence ---------------

fn save_tempo(tx: &rusqlite::Transaction, id: i64, tempo: &SpawnTempo) -> rusqlite::Result<()> {
    match tempo {
        SpawnTempo::Continuous { seconds, jitter, bulk_count } => {
            tx.execute(
                "INSERT OR REPLACE INTO summoning_tempo_continuous (summoning_id, seconds, jitter, bulk_count) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![id, seconds, jitter, bulk_count],
            )?;
        }
    }
    Ok(())
}

fn load_tempo(ctx: &mut LoadContext, old_id: i64, kind: &str) -> rusqlite::Result<Option<SpawnTempo>> {
    Ok(match kind {
        "Continuous" => {
            let mut stmt = ctx.conn.prepare("SELECT seconds, jitter, bulk_count FROM summoning_tempo_continuous WHERE summoning_id = ?1")?;
            let mut rows = stmt.query([old_id])?;
            if let Some(row) = rows.next()? {
                Some(SpawnTempo::Continuous {
                    seconds: row.get(0)?,
                    jitter: row.get(1)?,
                    bulk_count: row.get(2)?,
                })
            } else { None }
        }
        other => {
            Log::warn().dev().tag(Tag::GameLoad).message(format!("Unknown tempo kind in save: {other}"));
            None
        }
    })
}

// --------------- SpawnArea persistence ---------------

fn save_area(tx: &rusqlite::Transaction, id: i64, area: &SpawnArea) -> rusqlite::Result<()> {
    match area {
        SpawnArea::Coords { coords } => {
            for c in coords {
                tx.execute(
                    "INSERT OR REPLACE INTO summoning_area_coords (summoning_id, x, y) VALUES (?1, ?2, ?3)",
                    rusqlite::params![id, c.x, c.y],
                )?;
            }
        }
        SpawnArea::Rect { origin, width, height } => {
            tx.execute(
                "INSERT OR REPLACE INTO summoning_area_rect (summoning_id, origin_x, origin_y, width, height) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![id, origin.x, origin.y, width, height],
            )?;
        }
        SpawnArea::Edge { side } => {
            tx.execute(
                "INSERT OR REPLACE INTO summoning_area_edge (summoning_id, side) VALUES (?1, ?2)",
                rusqlite::params![id, side.as_ref()],
            )?;
        }
        SpawnArea::EdgesAll => {}
    }
    Ok(())
}

fn load_area(ctx: &mut LoadContext, old_id: i64, kind: &str) -> rusqlite::Result<Option<SpawnArea>> {
    Ok(match kind {
        "Coords" => {
            let mut stmt = ctx.conn.prepare("SELECT x, y FROM summoning_area_coords WHERE summoning_id = ?1")?;
            let mut rows = stmt.query([old_id])?;
            let mut coords = Vec::new();
            while let Some(row) = rows.next()? {
                coords.push(GridCoords { x: row.get(0)?, y: row.get(1)? });
            }
            Some(SpawnArea::Coords { coords })
        }
        "Rect" => {
            let mut stmt = ctx.conn.prepare("SELECT origin_x, origin_y, width, height FROM summoning_area_rect WHERE summoning_id = ?1")?;
            let mut rows = stmt.query([old_id])?;
            if let Some(row) = rows.next()? {
                Some(SpawnArea::Rect {
                    origin: GridCoords { x: row.get(0)?, y: row.get(1)? },
                    width: row.get(2)?,
                    height: row.get(3)?,
                })
            } else { None }
        }
        "Edge" => {
            let mut stmt = ctx.conn.prepare("SELECT side FROM summoning_area_edge WHERE summoning_id = ?1")?;
            let mut rows = stmt.query([old_id])?;
            if let Some(row) = rows.next()? {
                let side_str: String = row.get(0)?;
                match side_str.parse::<EdgeSide>() {
                    Ok(side) => Some(SpawnArea::Edge { side }),
                    Err(_) => {
                        Log::warn().dev().tag(Tag::GameLoad).message(format!("Unknown edge side in save: {side_str}"));
                        None
                    }
                }
            } else { None }
        }
        "EdgesAll" => Some(SpawnArea::EdgesAll),
        other => {
            Log::warn().dev().tag(Tag::GameLoad).message(format!("Unknown area kind in save: {other}"));
            None
        }
    })
}

// --------------- Wisp types persistence ---------------

fn save_wisp_types(tx: &rusqlite::Transaction, id: i64, wisp_types: &[WispType]) -> rusqlite::Result<()> {
    for wt in wisp_types {
        tx.execute(
            "INSERT OR REPLACE INTO summoning_wisp_types (summoning_id, wisp_type) VALUES (?1, ?2)",
            rusqlite::params![id, wt.as_ref()],
        )?;
    }
    Ok(())
}

fn load_wisp_types(ctx: &mut LoadContext, old_id: i64) -> rusqlite::Result<Vec<WispType>> {
    let mut stmt = ctx.conn.prepare("SELECT wisp_type FROM summoning_wisp_types WHERE summoning_id = ?1")?;
    let mut rows = stmt.query([old_id])?;
    let mut types = Vec::new();
    while let Some(row) = rows.next()? {
        let wt_str: String = row.get(0)?;
        match wt_str.parse::<WispType>() {
            Ok(wt) => types.push(wt),
            Err(_) => {
                Log::warn().dev().tag(Tag::GameLoad).message(format!("Unknown wisp type in save: {wt_str}"));
            }
        }
    }
    Ok(types)
}

fn load_summonings(ctx: &mut LoadContext) -> rusqlite::Result<()> {
    let mut stmt = ctx.conn.prepare(
        "SELECT id, id_name, state, activated_by, tempo_kind, limit_count, area_kind, produced, next_spawn_time FROM summonings",
    )?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let old_id: i64 = row.get(0)?;
        let id_name: String = row.get(1)?;
        let state_str: String = row.get(2)?;
        let activated_by_old: Option<i64> = row.get(3)?;
        let tempo_kind: String = row.get(4)?;
        let limit_count: Option<i32> = row.get(5)?;
        let area_kind: String = row.get(6)?;
        let produced: i32 = row.get(7)?;
        let next_spawn_time: f32 = row.get(8)?;

        let Some(entity) = ctx.entity(old_id) else {
            Log::warn().dev().tag(Tag::GameLoad).message(format!("Summoning with old ID {old_id} has no corresponding new entity"));
            continue;
        };

        let Ok(state) = state_str.parse::<SummoningState>() else {
            Log::warn().dev().tag(Tag::GameLoad).message(format!("Unknown summoning state in save: {state_str}"));
            continue;
        };

        let Some(tempo) = load_tempo(ctx, old_id, &tempo_kind)? else { continue };
        let Some(area) = load_area(ctx, old_id, &area_kind)? else { continue };

        let wisp_types = load_wisp_types(ctx, old_id)?;
        if wisp_types.is_empty() {
            Log::warn().dev().tag(Tag::GameLoad).message(format!(
                "Summoning with old ID {old_id} has no wisp types in save — skipping"
            ));
            continue;
        }

        let summoning = Summoning {
            id_name,
            wisp_types,
            area,
            tempo,
            limit_count,
        };

        let activated_by = activated_by_old.and_then(|old_ab| {
            ctx.entity(old_ab).or_else(|| {
                Log::warn().dev().tag(Tag::GameLoad).message(format!(
                    "Summoning with old ID {old_id} has activated_by={old_ab} that failed entity remap — summoning will not activate"
                ));
                None
            })
        });

        let builder = BuilderSummoning::new(summoning)
            .with_state(state)
            .with_runtime(SummoningRuntime { produced, next_spawn_time });
        let builder = if let Some(moment_entity) = activated_by {
            builder.with_activated_by(moment_entity)
        } else {
            builder
        };
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

    entity_commands
        .remove::<BuilderSummoning>()
        .insert(builder.summoning.clone())
        .insert(builder.runtime)
        .insert(builder.state);

    if let Some(moment_entity) = builder.activated_by {
        entity_commands.insert(MomentOfInterest(moment_entity));
    }
}

fn tick_active_summoning_system(
    mut commands: Commands,
    obstacle_grid: Res<ObstacleGrid>,
    mut clock: ResMut<SummoningClock>,
    time: Res<Time>,
    mut summoning: Query<(Entity, &Summoning, &mut SummoningRuntime), With<SummoningActive>>,
) {
    clock.0 += time.delta_secs();
    let now = clock.0;
    let mut rng = nanorand::tls_rng();

    for (entity, summoning, mut runtime) in &mut summoning {
        // Wait until due
        if now < runtime.next_spawn_time { continue; }

        match summoning.tempo {
            SpawnTempo::Continuous { seconds, jitter, bulk_count } => {
                let remaining = summoning.limit_count.map(|m| m.saturating_sub(runtime.produced)).unwrap_or(i32::MAX);
                let to_spawn: i32 = std::cmp::min(bulk_count, remaining).max(0);
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

        // This tick's spawn may have reached the limit
        if let Some(limit) = summoning.limit_count && runtime.produced >= limit {
            commands.entity(entity)
                .insert(SummoningState::Exhausted)
                .trigger(SummoningExhaustedEvent::from);
        }
    }
}

// ============================================================================
// ACTIVATION
// ============================================================================

/// On `MomentHappened` at a summoning root: if the summoning is `Inactive`,
/// transition to `Active` and fire `SummoningActivatedEvent`. Raw
/// `SummoningState` inserts (load path) only trigger marker sync — they do not
/// fire the terminal event.
fn on_moment_happened_activate_summoning(
    trigger: On<MomentHappened>,
    summonings: Query<&Summoning, With<SummoningInactive>>,
    mut commands: Commands,
) {
    let entity = trigger.entity;
    let Ok(summoning) = summonings.get(entity) else { return };
    commands.entity(entity)
        .insert(SummoningState::Active)
        .trigger(SummoningActivatedEvent::from);
    Log::info().player().tag(Tag::Wave).message(format!("Summoning '{}' activated", summoning.id_name));
}
