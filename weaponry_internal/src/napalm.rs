//! # Napalm
//!
//! Burning ground left by napalm bombs. Each patch damages wisps on one cell until its `TimeBound`
//! expires. Overlapping patches deal damage independently.
//!
//! Expired napalm leaves a visual char mark that fades over its own `TimeBound`.
//! Burning patches are saved; char marks are not.
//!
//! `NAPALM_DAMAGE_PER_SECOND` sets damage for all patches, independently of tower upgrades.

use bevy::prelude::*;

use game_core::prelude::{DamageMessage, GridArea, GridCoords, SSS};
use grids::{ground_cover::{GroundCover, GroundCoverKind}, wisps::WispsGrid};
use logging::prelude::*;
use persistence::prelude::*;
use session::{GameClock, TimeBound};
use states::prelude::{GameState, InGame, MapLoadingStage};
use wisps::prelude::Wisp;

pub(crate) struct NapalmPlugin;
impl Plugin for NapalmPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Update, napalm_damage_system.run_if(in_state(GameState::Running)))
            .add_observer(on_builder_add_spawn_napalm)
            .add_observer(on_remove_napalm_spawn_char.run_if(in_state(InGame)))
            .add_systems(CollectSave, collect_napalms)
            .register_loader(MapLoadingStage::SpawnMapElements, "napalms", load_napalms);
    }
}

// Napalm balance
pub(crate) const NAPALM_BURN_DURATION: f32 = 5.;
const NAPALM_DAMAGE_PER_SECOND: f32 = 12.;

// Char
const CHAR_DURATION: f32 = 3.;

#[derive(Component)]
pub(crate) struct Napalm;

#[derive(Component, SSS)]
pub(crate) struct BuilderNapalm {
    pub grid_coords: GridCoords,
    pub time_bound: TimeBound,
}
impl BuilderNapalm {
    pub fn new(grid_coords: GridCoords, time_bound: TimeBound) -> Self {
        Self { grid_coords, time_bound }
    }
}

#[log_tags(Tag::GameSave)]
fn collect_napalms(
    napalms: Query<(Entity, &GridArea, &TimeBound), With<Napalm>>,
    mut save: SaveWriter,
) {
    if napalms.is_empty() { return; }

    #[debug_dev("Saving {} napalms", rows.len())]
    let rows: Vec<(u32, GridCoords, TimeBound)> = napalms
        .iter()
        .map(|(entity, area, time_bound)| (entity.index_u32(), area.origin, *time_bound))
        .collect();
    save.submit(move |ctx| {
        for (id, grid_coords, time_bound) in rows {
            ctx.save_marker("napalms", id)?;
            ctx.save_grid_coords(id, grid_coords)?;
            ctx.save_time_bound(id, time_bound)?;
        }
        Ok(())
    });
}

fn load_napalms(ctx: &mut LoadContext) -> LoadResult {
    ctx.for_each_entity("SELECT id FROM napalms", |ctx, old_id, entity, _| {
        let grid_coords = ctx.grid_coords(old_id)?;
        let time_bound = ctx.time_bound(old_id)?.ok_or(LoadError::MissingRow { table: "time_bounds" })?;
        ctx.insert(entity, BuilderNapalm::new(grid_coords, time_bound));
        Ok(())
    })
}

fn on_builder_add_spawn_napalm(
    trigger: On<Add<BuilderNapalm>>,
    mut commands: Commands,
    builders: Query<&BuilderNapalm>,
) {
    let entity = trigger.entity;
    let Ok(builder) = builders.get(entity) else { return; };

    commands.entity(entity)
        .remove::<BuilderNapalm>()
        .insert((
            Napalm,
            GroundCover(GroundCoverKind::Napalm),
            GridArea::cell(builder.grid_coords),
            builder.time_bound,
        ));
}

fn napalm_damage_system(
    mut damage_messages: MessageWriter<DamageMessage>,
    time: Res<Time>,
    wisps_grid: Res<WispsGrid>,
    napalms: Query<&GridArea, With<Napalm>>,
    wisps: Query<(), With<Wisp>>,
) {
    let damage = NAPALM_DAMAGE_PER_SECOND * time.delta_secs();
    for area in napalms.iter() {
        let Some(wisps_here) = wisps_grid.get(area.origin) else { continue; };
        for wisp in wisps_here {
            if !wisps.contains(*wisp) { continue; } // Grid entries may not yet be visible to the query.
            damage_messages.write(DamageMessage { target: *wisp, amount: damage });
        }
    }
}

fn on_remove_napalm_spawn_char(
    trigger: On<Remove<Napalm>>,
    mut commands: Commands,
    clock: Res<GameClock>,
    napalms: Query<&GridArea, With<Napalm>>,
) {
    let Ok(area) = napalms.get(trigger.entity) else { return; };
    commands.spawn((
        GroundCover(GroundCoverKind::Char),
        *area,
        TimeBound::lasting(&clock, CHAR_DURATION),
    ));
}
