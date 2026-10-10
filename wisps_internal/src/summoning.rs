use bevy::prelude::*;
use nanorand::Rng;

use game_core::{moments::moment_attach_self_trigger_to_parent, prelude::*};
use grids::prelude::ObstacleGrid;
use logging::prelude::*;
use persistence::{prelude::*, rusqlite};
use session::GameClock;
use states::prelude::*;
use wisps::summoning::*;

use super::spawning::BuilderWisp;

pub(crate) struct SummoningPlugin;
impl Plugin for SummoningPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Update, tick_active_summoning_system.run_if(in_state(GameState::Running)))
            .add_observer(on_insert_summoning_state_sync_markers)
            .add_observer(on_moment_happened_activate_summoning)
            .add_observer(on_builder_add_spawn_summoning)
            .add_observer(moment_attach_self_trigger_to_parent::<MomentSummoningStarted, SummoningActivatedEvent>)
            .add_observer(moment_attach_self_trigger_to_parent::<MomentSummoningExhausted, SummoningExhaustedEvent>)
            .add_systems(CollectSave, collect_summonings)
            .register_loader(MapLoadingStage::SpawnMapElements, "summonings", load_summonings)
            .register_moment_persistence::<MomentSummoningStarted>()
            .register_moment_persistence::<MomentSummoningExhausted>();
    }
}

// --------------- SUMMONING ENTITIES AND RUNTIME ---------------

/// On every `Insert<SummoningState>`, swap the marker components to match
/// the new state. Markers are never inserted directly — this is the single
/// entry point that derives them. Works identically on fresh spawn (the
/// builder inserts `SummoningState`) and on load (the builder inserts the
/// restored state).
fn on_insert_summoning_state_sync_markers(
    trigger: On<Insert<SummoningState>>,
    mut commands: Commands,
    states: Query<&SummoningState>,
) {
    let entity = trigger.entity;
    let Ok(new_state) = states.get(entity) else { return };
    let mut entity_commands = commands.entity(entity);
    entity_commands.remove::<(SummoningInactive, SummoningActive, SummoningExhausted)>();
    match new_state {
        SummoningState::Inactive => { entity_commands.insert(SummoningInactive); }
        SummoningState::Active => { entity_commands.insert(SummoningActive); }
        SummoningState::Exhausted => { entity_commands.insert(SummoningExhausted); }
    }
}

#[log_tags(Tag::GameSave)]
fn collect_summonings(
    save_runner: Res<SaveRunner>,
    summonings: Query<(Entity, &Summoning, &SummoningState, &SummoningRuntime, Option<&MomentOfInterest>)>,
    mut save: SaveWriter,
) {
    if summonings.is_empty() { return; }

    struct Snapshot {
        id: u32,
        summoning: Summoning,
        state: SummoningState,
        activated_by: Option<u32>,
        produced: i32,
        next_spawn_time: f32,
    }

    #[debug_dev("Saving {} summonings", snapshots.len())]
    let snapshots: Vec<Snapshot> = summonings
        .iter()
        .map(|(entity, summoning, state, runtime, activated_by)| {
            let (state, produced, next_spawn_time) = if save_runner.save_as_scenario {
                (SummoningState::Inactive, 0, 0.0)
            } else {
                (*state, runtime.produced, runtime.next_spawn_time)
            };
            Snapshot {
                id: entity.index_u32(),
                summoning: summoning.clone(),
                state,
                activated_by: activated_by.map(|moment| moment.0.index_u32()),
                produced,
                next_spawn_time,
            }
        })
        .collect();

    save.submit(move |ctx| {
        for snapshot in &snapshots {
            ctx.register_entity(snapshot.id)?;

            ctx.tx.execute(
                "INSERT OR REPLACE INTO summonings (id, id_name, state, activated_by, tempo_kind, limit_count, area_kind, produced, next_spawn_time) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                rusqlite::params![
                    snapshot.id,
                    snapshot.summoning.id_name,
                    snapshot.state.as_ref(),
                    snapshot.activated_by,
                    snapshot.summoning.tempo.as_ref(),
                    snapshot.summoning.limit_count,
                    snapshot.summoning.area.as_ref(),
                    snapshot.produced,
                    snapshot.next_spawn_time,
                ],
            )?;

            save_tempo(ctx, snapshot.id, &snapshot.summoning.tempo)?;
            save_area(ctx, snapshot.id, &snapshot.summoning.area)?;
            save_wisp_types(ctx, snapshot.id, &snapshot.summoning.wisp_types)?;
        }
        Ok(())
    });
}

fn save_tempo(ctx: &SaveContext, id: u32, tempo: &SpawnTempo) -> rusqlite::Result<()> {
    match tempo {
        SpawnTempo::Continuous { seconds, jitter, bulk_count } => {
            ctx.tx.execute(
                "INSERT OR REPLACE INTO summoning_tempo_continuous (summoning_id, seconds, jitter, bulk_count) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![id, seconds, jitter, bulk_count],
            )?;
        }
    }
    Ok(())
}

fn save_area(ctx: &SaveContext, id: u32, area: &SpawnArea) -> rusqlite::Result<()> {
    match area {
        SpawnArea::Coords { coords } => {
            for coord in coords {
                ctx.tx.execute(
                    "INSERT OR REPLACE INTO summoning_area_coords (summoning_id, x, y) VALUES (?1, ?2, ?3)",
                    rusqlite::params![id, coord.x, coord.y],
                )?;
            }
        }
        SpawnArea::Rect { origin, width, height } => {
            ctx.tx.execute(
                "INSERT OR REPLACE INTO summoning_area_rect (summoning_id, origin_x, origin_y, width, height) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![id, origin.x, origin.y, width, height],
            )?;
        }
        SpawnArea::Edge { side } => {
            ctx.tx.execute(
                "INSERT OR REPLACE INTO summoning_area_edge (summoning_id, side) VALUES (?1, ?2)",
                rusqlite::params![id, side.as_ref()],
            )?;
        }
        SpawnArea::EdgesAll => {}
    }
    Ok(())
}

fn save_wisp_types(ctx: &SaveContext, id: u32, wisp_types: &[WispType]) -> rusqlite::Result<()> {
    for wisp_type in wisp_types {
        ctx.tx.execute(
            "INSERT OR REPLACE INTO summoning_wisp_types (summoning_id, wisp_type) VALUES (?1, ?2)",
            rusqlite::params![id, wisp_type.as_ref()],
        )?;
    }
    Ok(())
}

#[log_tags(Tag::GameLoad)]
fn load_summonings(ctx: &mut LoadContext) -> LoadResult {
    ctx.for_each_entity(
        "SELECT id, id_name, state, activated_by, tempo_kind, limit_count, area_kind, produced, next_spawn_time FROM summonings",
        |ctx, old_id, entity, row| {
            let id_name: String = row.get(1)?;
            let state = row.get_parsed::<SummoningState>(2)?;
            let activated_by_old_id: Option<u32> = row.get(3)?;
            let tempo_kind: String = row.get(4)?;
            let limit_count: Option<i32> = row.get(5)?;
            let area_kind: String = row.get(6)?;
            let produced: i32 = row.get(7)?;
            let next_spawn_time: f32 = row.get(8)?;

            let tempo = match tempo_kind.as_str() {
                "Continuous" => ctx.conn.query_row(
                    "SELECT seconds, jitter, bulk_count FROM summoning_tempo_continuous WHERE summoning_id = ?1",
                    [old_id],
                    |row| Ok(SpawnTempo::Continuous {
                        seconds: row.get(0)?,
                        jitter: row.get(1)?,
                        bulk_count: row.get(2)?,
                    }),
                ).map_err(LoadError::table_read("summoning_tempo_continuous"))?,
                other => return Err(LoadError::unknown_value("tempo kind", other)),
            };

            let area = match area_kind.as_str() {
                "Coords" => {
                    let coords = ctx.conn.prepare("SELECT x, y FROM summoning_area_coords WHERE summoning_id = ?1")?
                        .query_map([old_id], |row| Ok(GridCoords { x: row.get(0)?, y: row.get(1)? }))?
                        .collect::<rusqlite::Result<Vec<_>>>()?;
                    SpawnArea::Coords { coords }
                }
                "Rect" => ctx.conn.query_row(
                    "SELECT origin_x, origin_y, width, height FROM summoning_area_rect WHERE summoning_id = ?1",
                    [old_id],
                    |row| Ok(SpawnArea::Rect {
                        origin: GridCoords { x: row.get(0)?, y: row.get(1)? },
                        width: row.get(2)?,
                        height: row.get(3)?,
                    }),
                ).map_err(LoadError::table_read("summoning_area_rect"))?,
                "Edge" => {
                    let side_name: String = ctx.conn.query_row(
                        "SELECT side FROM summoning_area_edge WHERE summoning_id = ?1",
                        [old_id],
                        |row| row.get(0),
                    ).map_err(LoadError::table_read("summoning_area_edge"))?;
                    let side = side_name.parse::<EdgeSide>()
                        .map_err(|_| LoadError::unknown_value("edge side", &side_name))?;
                    SpawnArea::Edge { side }
                }
                "EdgesAll" => SpawnArea::EdgesAll,
                other => return Err(LoadError::unknown_value("area kind", other)),
            };

            // An unknown wisp type drops only that type; the summoning still loads with the rest.
            let wisp_types: Vec<WispType> = ctx.conn.prepare("SELECT wisp_type FROM summoning_wisp_types WHERE summoning_id = ?1")?
                .query_and_then([old_id], |row| row.get_parsed::<WispType>(0))?
                .filter_map(|wisp_type| wisp_type
                    .inspect_err(|error| warn_dev!("Summoning with old ID {old_id} dropped a wisp type: {error}"))
                    .ok())
                .collect();
            if wisp_types.is_empty() {
                return Err(LoadError::MissingRow { table: "summoning_wisp_types" });
            }

            let activated_by = ctx.optional_entity(activated_by_old_id)
                .inspect_err(|error| warn_dev!("Summoning with old ID {old_id} loads without its activator and will never activate: {error}"))
                .unwrap_or_default();

            let summoning = Summoning {
                id_name,
                wisp_types,
                area,
                tempo,
                limit_count,
            };
            let builder = BuilderSummoning::new(summoning)
                .with_state(state)
                .with_runtime(SummoningRuntime { produced, next_spawn_time })
                .with_activated_by(activated_by);
            ctx.insert(entity, builder);
            Ok(())
        },
    )
}

fn on_builder_add_spawn_summoning(
    trigger: On<Add<BuilderSummoning>>,
    mut commands: Commands,
    builders: Query<&BuilderSummoning>,
) {
    let entity = trigger.entity;
    let Ok(builder) = builders.get(entity) else { return; };

    commands.entity(entity)
        .remove::<BuilderSummoning>()
        .insert((builder.summoning.clone(), builder.runtime, builder.state))
        .insert_some(builder.activated_by);
}

#[log_tags(Tag::Wave)]
fn tick_active_summoning_system(
    mut commands: Commands,
    obstacle_grid: Res<ObstacleGrid>,
    clock: Res<GameClock>,
    mut summonings: Query<(Entity, &Summoning, &mut SummoningRuntime), With<SummoningActive>>,
) {
    let now = clock.elapsed as f32;
    let mut rng = nanorand::tls_rng();

    for (entity, summoning, mut runtime) in summonings.iter_mut() {
        // Wait until due
        if now < runtime.next_spawn_time { continue; }

        match summoning.tempo {
            SpawnTempo::Continuous { seconds, jitter, bulk_count } => {
                let remaining = summoning.limit_count.map(|limit| limit.saturating_sub(runtime.produced)).unwrap_or(i32::MAX);
                let to_spawn: i32 = bulk_count.min(remaining).max(0);
                for _ in 0..(to_spawn as usize) {
                    let grid_coords = summoning.area.get_random_coord(&obstacle_grid, &mut rng);
                    let wisp_type = summoning.get_random_wisp_type(&mut rng);
                    commands.spawn(BuilderWisp::new(wisp_type, grid_coords));
                }
                runtime.produced = runtime.produced.saturating_add(to_spawn);
                let jitter_offset = if jitter > 0.0 { (rng.generate::<f32>() * 2.0 - 1.0) * jitter } else { 0.0 };
                runtime.next_spawn_time = now + (seconds + jitter_offset);
            }
        }

        // This tick's spawn may have reached the limit
        if let Some(limit) = summoning.limit_count && runtime.produced >= limit {
            #[info_player("Summoning '{}' exhausted", summoning.id_name)]
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
#[log_tags(Tag::Wave)]
fn on_moment_happened_activate_summoning(
    trigger: On<MomentHappened>,
    mut commands: Commands,
    summonings: Query<&Summoning, With<SummoningInactive>>,
) {
    let entity = trigger.entity;
    let Ok(summoning) = summonings.get(entity) else { return };
    #[info_player("Summoning '{}' activated", summoning.id_name)]
    commands.entity(entity)
        .insert(SummoningState::Active)
        .trigger(SummoningActivatedEvent::from);
}
