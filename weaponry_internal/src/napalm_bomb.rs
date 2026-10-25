//! # Napalm bomb
//!
//! The Napalm Battery's lobbed projectile. On landing, a bomb damages wisps on its cell.
//! It also ignites napalm unless a wall or building occupies the cell. Dark ore does not
//! prevent ignition.
//!
//! `Lob` stores the flight and scheduled launch time for saving and loading.
//! `NAPALM_BOMB_IMPACT_DAMAGE` sets impact damage independently of tower upgrades.

use bevy::prelude::*;

use game_core::prelude::{CELL_SIZE, DamageMessage, GridCoords, OptionalCommands};
use grids::{prelude::ObstacleGrid, wisps::WispsGrid};
use logging::prelude::*;
use persistence::prelude::*;
use session::{GameClock, TimeBound};
use states::prelude::{GameState, MapLoadingStage};
use weaponry::{lob::{Lob, LobInFlight}, prelude::*};
use wisps::prelude::Wisp;

use crate::{
    lob::{LoadLobExtension, LobBody, LobDiscImage, SaveLobExtension, advance_lobs},
    napalm::{BuilderNapalm, NAPALM_BURN_DURATION},
};

pub(crate) struct NapalmBombPlugin;
impl Plugin for NapalmBombPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Update, napalm_bomb_hit_system.after(advance_lobs).run_if(in_state(GameState::Running)))
            .add_observer(on_builder_add_spawn_napalm_bomb)
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
    napalm_bombs: Query<(Entity, &Lob), With<NapalmBomb>>,
    mut save: SaveWriter,
) {
    if napalm_bombs.is_empty() { return; }

    #[debug_dev("Saving {} napalm bombs", rows.len())]
    let rows: Vec<(u32, Lob)> = napalm_bombs
        .iter()
        .map(|(entity, lob)| (entity.index_u32(), *lob))
        .collect();
    save.submit(move |ctx| {
        for (id, lob) in rows {
            ctx.save_marker("napalm_bombs", id)?;
            ctx.save_lob(id, &lob)?;
        }
        Ok(())
    });
}

fn load_napalm_bombs(ctx: &mut LoadContext) -> LoadResult {
    ctx.for_each_entity("SELECT id FROM napalm_bombs", |ctx, old_id, entity, _| {
        ctx.insert(entity, BuilderNapalmBomb::new(ctx.lob(old_id)?));
        Ok(())
    })
}

fn on_builder_add_spawn_napalm_bomb(
    trigger: On<Add<BuilderNapalmBomb>>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    disc_image: Res<LobDiscImage>,
    builders: Query<&BuilderNapalmBomb>,
) {
    let entity = trigger.entity;
    let Ok(builder) = builders.get(entity) else { return; };

    commands.entity(entity)
        .remove::<BuilderNapalmBomb>()
        .insert_some(builder.shooter.map(ProjectileOf))
        .insert((
            NapalmBomb,
            builder.lob,
            children![
                (
                    LobBody,
                    Sprite {
                        image: asset_server.load(NAPALM_BOMB_IMAGE),
                        custom_size: Some(Vec2::splat(NAPALM_BOMB_SIZE)),
                        ..default()
                    },
                ),
                disc_image.make_shadow(NAPALM_BOMB_SIZE),
            ],
        ));
}

fn napalm_bomb_hit_system(
    mut commands: Commands,
    mut damage_messages: MessageWriter<DamageMessage>,
    clock: Res<GameClock>,
    obstacle_grid: Res<ObstacleGrid>,
    wisps_grid: Res<WispsGrid>,
    napalm_bombs: Query<(Entity, &Lob), (With<NapalmBomb>, With<LobInFlight>)>,
    wisps: Query<(), With<Wisp>>,
) {
    for (entity, lob) in napalm_bombs.iter() {
        if !lob.is_landed(clock.now()) { continue; }

        let landing_coords = GridCoords::from_world_position(lob.target());
        // TODO: Add a napalm impact visual.
        if wisps_grid.bounds.contains(landing_coords) {
            for wisp in &wisps_grid[landing_coords] {
                if !wisps.contains(*wisp) { continue; } // Grid entries may not yet be visible to the query.
                damage_messages.write(DamageMessage { target: *wisp, amount: NAPALM_BOMB_IMPACT_DAMAGE });
            }
            if !obstacle_grid[landing_coords].has_structure() {
                commands.spawn(BuilderNapalm::new(landing_coords, TimeBound::lasting(&clock, NAPALM_BURN_DURATION)));
            }
        }
        commands.entity(entity).despawn();
    }
}
