use bevy::prelude::*;

use alteration::modifiers::prelude::AttackDamage;
use game_core::prelude::{ALL_DIRECTIONS, CELL_SIZE, DamageMessage, GridCoords, OptionalCommands, Property, ZDepth};
use grids::wisps::WispsGrid;
use logging::prelude::*;
use persistence::{prelude::*, rusqlite};
use states::prelude::{GameState, MapLoadingStage};
use visuals::prelude::BuilderExplosion;
use weaponry::{lob::Lob, prelude::*};
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

    let lob = builder.lob;
    let size = CELL_SIZE / 2.;
    commands.entity(entity)
        .remove::<BuilderCannonball>()
        .insert_some(builder.shooter.map(ProjectileOf))
        .insert((
            Transform::from_translation(lob.current().ground_position.extend(0.)),
            Cannonball,
            lob,
            builder.damage,
            children![
                (
                    LobBody,
                    Sprite {
                        image: asset_server.load(CANNONBALL_BASE_IMAGE),
                        custom_size: Some(Vec2::splat(size)),
                        ..default()
                    },
                    ZDepth::PROJECTILE,
                ),
                disc_image.make_shadow(size),
            ],
        ));
}

fn cannonball_hit_system(
    mut commands: Commands,
    mut damage_messages: MessageWriter<DamageMessage>,
    wisps_grid: Res<WispsGrid>,
    cannonballs: Query<(Entity, &Lob, &AttackDamage), With<Cannonball>>,
    wisps: Query<(), With<Wisp>>,
) {
    for (entity, lob, attack_damage) in cannonballs.iter() {
        if !lob.is_landed() { continue; }

        let coords = GridCoords::from_world_vec2(lob.target());
        for direction in ALL_DIRECTIONS.iter().chain(&[(0, 0)]) {
            let blast_zone_coords = coords.shifted(*direction);
            if !blast_zone_coords.are_in_bounds(wisps_grid.bounds) { continue; }

            commands.spawn(BuilderExplosion(blast_zone_coords));

            let wisps_in_coords = &wisps_grid[blast_zone_coords];
            for wisp in wisps_in_coords {
                if !wisps.contains(*wisp) { continue; } // May not find wisp if the wisp spawned at the same frame.
                damage_messages.write(DamageMessage {
                    target: *wisp,
                    amount: attack_damage.get(),
                });
            }
        }
        commands.entity(entity).despawn();
    }
}
