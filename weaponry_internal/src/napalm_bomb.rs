//! # Napalm Bomb
//!
//! Projectile lobbed by the Napalm Battery. Bombs spawned with a [`ProjectileLaunchCounter`] stay
//! hidden until [`ProjectileLaunched`] starts their [`Lob`] flight.
//!
//! On [`LobLanded`], the bomb deals [`NAPALM_BOMB_IMPACT_DAMAGE`] (fixed, independent of tower
//! modifiers) to wisps on the landing cell's top surface and spawns [`BuilderNapalm`] there unless
//! a building occupies the cell.

use bevy::prelude::*;

use game_core::prelude::{CELL_SIZE, DamageMessage, GridCoords, OptionalCommands};
use indexes::{prelude::ObstacleGrid, wisps::WispsGrid};
use logging::prelude::*;
use persistence::prelude::*;
use session::{GameClock, TimeBound};
use states::prelude::MapLoadingStage;
use visuals::drop_shadow::DropShadow;
use weaponry::{lob::{Lob, LobArc, LobArcProgress, LobLanded}, prelude::*};

use crate::{
    lob::{LoadLobExtension, LobBody, SaveLobExtension},
    napalm::{BuilderNapalm, NAPALM_BURN_DURATION},
    projectile_launch::{LoadProjectileLaunchCounterExtension, SaveProjectileLaunchCounterExtension},
};

pub(crate) struct NapalmBombPlugin;
impl Plugin for NapalmBombPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_observer(on_builder_add_spawn_napalm_bomb)
            .add_observer(on_projectile_launched_start_napalm_bomb_flight)
            .add_observer(on_lob_landed_ignite_napalm_bomb)
            .add_systems(CollectSave, collect_napalm_bombs)
            .register_loader(MapLoadingStage::SpawnMapElements, "napalm_bombs", load_napalm_bombs);
    }
}

const NAPALM_BOMB_IMAGE: &str = "projectiles/cannonball.png";
/// Body and ground-level shadow diameter, in world units.
const NAPALM_BOMB_SIZE: f32 = CELL_SIZE / 3.;
const NAPALM_BOMB_IMPACT_DAMAGE: f32 = 20.;

#[log_tags(Tag::GameSave)]
fn collect_napalm_bombs(
    napalm_bombs: Query<(Entity, &LobArc, &LobArcProgress, Option<&ProjectileLaunchCounter>), With<NapalmBomb>>,
    mut save: SaveWriter,
) {
    if napalm_bombs.is_empty() { return; }

    #[debug_dev("Saving {} napalm bombs", rows.len())]
    let rows: Vec<(u32, LobArc, LobArcProgress, Option<ProjectileLaunchCounter>)> = napalm_bombs
        .iter()
        .map(|(entity, arc, progress, launch_counter)| (entity.index_u32(), *arc, *progress, launch_counter.copied()))
        .collect();
    save.submit(move |ctx| {
        for (id, arc, progress, launch_counter) in rows {
            ctx.save_marker("napalm_bombs", id)?;
            ctx.save_lob(id, arc, progress)?;
            if let Some(launch_counter) = launch_counter {
                ctx.save_projectile_launch_counter(id, launch_counter)?;
            }
        }
        Ok(())
    });
}

fn load_napalm_bombs(ctx: &mut LoadContext) -> LoadResult {
    ctx.for_each_entity("SELECT id FROM napalm_bombs", |ctx, old_id, entity, _| {
        let (arc, progress) = ctx.lob(old_id)?;
        let builder = BuilderNapalmBomb::new(arc)
            .with_progress(progress)
            .with_launch_counter(ctx.projectile_launch_counter(old_id)?);
        ctx.insert(entity, builder);
        Ok(())
    })
}

fn on_builder_add_spawn_napalm_bomb(
    trigger: On<Add<BuilderNapalmBomb>>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    builders: Query<&BuilderNapalmBomb>,
) {
    let entity = trigger.entity;
    let Ok(builder) = builders.get(entity) else { return; };

    let mut bomb = commands.entity(entity);
    bomb
        .remove::<BuilderNapalmBomb>()
        .insert_some(builder.shooter.map(ProjectileOf))
        .insert((
            NapalmBomb,
            builder.arc,
            builder.progress,
            children![
                (
                    LobBody,
                    Sprite {
                        image: asset_server.load(NAPALM_BOMB_IMAGE),
                        custom_size: Some(Vec2::splat(NAPALM_BOMB_SIZE)),
                        ..default()
                    },
                ),
                DropShadow::new(NAPALM_BOMB_SIZE, builder.arc.target),
            ],
        ));
    match builder.launch_counter {
        Some(launch_counter) => { bomb.insert((launch_counter, Visibility::Hidden)); }
        None => { bomb.insert(Lob); }
    }
}

fn on_projectile_launched_start_napalm_bomb_flight(
    trigger: On<ProjectileLaunched>,
    mut commands: Commands,
    napalm_bombs: Query<(), With<NapalmBomb>>,
) {
    let entity = trigger.0;
    if !napalm_bombs.contains(entity) { return; }
    commands.entity(entity).insert((Lob, Visibility::Inherited));
}

fn on_lob_landed_ignite_napalm_bomb(
    trigger: On<LobLanded>,
    mut commands: Commands,
    mut damage_messages: MessageWriter<DamageMessage>,
    clock: Res<GameClock>,
    obstacle_grid: Res<ObstacleGrid>,
    wisps_grid: Res<WispsGrid>,
    napalm_bombs: Query<&LobArc, With<NapalmBomb>>,
) {
    let entity = trigger.0;
    let Ok(arc) = napalm_bombs.get(entity) else { return; };

    let landing_coords = GridCoords::from_world_position(arc.target);
    // TODO: Add a napalm impact visual.
    if wisps_grid.bounds.contains(landing_coords) {
        let top = obstacle_grid.top_position(landing_coords);
        for &wisp in wisps_grid.wisps_at(top) {
            damage_messages.write(DamageMessage { target: wisp, amount: NAPALM_BOMB_IMPACT_DAMAGE });
        }
        if !obstacle_grid[top].is_building() {
            commands.spawn(BuilderNapalm::new(top, TimeBound::lasting(&clock, NAPALM_BURN_DURATION)));
        }
    }
    commands.entity(entity).despawn();
}
