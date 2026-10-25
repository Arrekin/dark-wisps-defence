use bevy::prelude::*;

use alteration::modifiers::prelude::AttackDamage;
use game_core::prelude::{CELL_SIZE, DamageMessage, GridArea, GridCoords, GridImprint, OptionalCommands, Property};
use grids::wisps::WispsGrid;
use logging::prelude::*;
use persistence::{prelude::*, rusqlite};
use session::GameClock;
use states::prelude::{GameState, MapLoadingStage};
use visuals::prelude::BuilderExplosion;
use weaponry::{lob::{Lob, LobInFlight}, prelude::*};
use wisps::prelude::Wisp;

use crate::lob::{LoadLobExtension, LobBody, LobDiscImage, SaveLobExtension, advance_lobs};

pub(crate) struct CannonballPlugin;
impl Plugin for CannonballPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Update, cannonball_hit_system.after(advance_lobs).run_if(in_state(GameState::Running)))
            .add_observer(on_builder_add_spawn_cannonball)
            .add_systems(CollectSave, collect_cannonballs)
            .register_loader(MapLoadingStage::SpawnMapElements, "cannonballs", load_cannonballs);
    }
}

const CANNONBALL_BASE_IMAGE: &str = "projectiles/cannonball.png";

// Blast area, in cells from the landing cell
const BLAST_REACH: i32 = 1;

#[log_tags(Tag::GameSave)]
fn collect_cannonballs(
    cannonballs: Query<(Entity, &Lob, &AttackDamage), With<Cannonball>>,
    mut save: SaveWriter,
) {
    if cannonballs.is_empty() { return; }

    #[debug_dev("Saving {} cannonballs", rows.len())]
    let rows: Vec<(u32, f32, Lob)> = cannonballs
        .iter()
        .map(|(entity, lob, damage)| (entity.index_u32(), damage.get(), *lob))
        .collect();
    save.submit(move |ctx| {
        for (id, damage, lob) in rows {
            ctx.register_entity(id)?;
            ctx.tx.execute(
                "INSERT OR REPLACE INTO cannonballs (id, damage) VALUES (?1, ?2)",
                rusqlite::params![id, damage],
            )?;
            ctx.save_lob(id, &lob)?;
        }
        Ok(())
    });
}

fn load_cannonballs(ctx: &mut LoadContext) -> LoadResult {
    ctx.for_each_entity("SELECT id, damage FROM cannonballs", |ctx, old_id, entity, row| {
        let damage: f32 = row.get(1)?;
        let builder = BuilderCannonball::new(ctx.lob(old_id)?, AttackDamage::new(damage));
        ctx.insert(entity, builder);
        Ok(())
    })
}

fn on_builder_add_spawn_cannonball(
    trigger: On<Add<BuilderCannonball>>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    disc_image: Res<LobDiscImage>,
    builders: Query<&BuilderCannonball>,
) {
    let entity = trigger.entity;
    let Ok(builder) = builders.get(entity) else { return; };

    let size = CELL_SIZE / 2.;
    commands.entity(entity)
        .remove::<BuilderCannonball>()
        .insert_some(builder.shooter.map(ProjectileOf))
        .insert((
            Cannonball,
            builder.lob,
            builder.damage,
            children![
                (
                    LobBody,
                    Sprite {
                        image: asset_server.load(CANNONBALL_BASE_IMAGE),
                        custom_size: Some(Vec2::splat(size)),
                        ..default()
                    },
                ),
                disc_image.make_shadow(size),
            ],
        ));
}

fn cannonball_hit_system(
    mut commands: Commands,
    mut damage_messages: MessageWriter<DamageMessage>,
    clock: Res<GameClock>,
    wisps_grid: Res<WispsGrid>,
    cannonballs: Query<(Entity, &Lob, &AttackDamage), (With<Cannonball>, With<LobInFlight>)>,
    wisps: Query<(), With<Wisp>>,
) {
    for (entity, lob, attack_damage) in cannonballs.iter() {
        if !lob.is_landed(clock.now()) { continue; }

        let blast_zone = GridArea::centered_on(GridCoords::from_world_position(lob.target()), GridImprint::square_of_reach(BLAST_REACH));
        for blast_zone_coords in blast_zone.iter_in_bounds(wisps_grid.bounds) {
            commands.spawn(BuilderExplosion(blast_zone_coords));

            for wisp in &wisps_grid[blast_zone_coords] {
                if !wisps.contains(*wisp) { continue; } // Grid entries may not yet be visible to the query.
                damage_messages.write(DamageMessage { target: *wisp, amount: attack_damage.get() });
            }
        }
        commands.entity(entity).despawn();
    }
}
