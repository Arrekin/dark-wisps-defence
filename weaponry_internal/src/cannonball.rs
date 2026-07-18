use std::f32::consts::PI;

use bevy::prelude::*;

use alteration::modifiers::prelude::AttackDamage;
use game_core::prelude::{CELL_SIZE, DamageMessage, GridCoords, Property, Z_PROJECTILE};
use grids::{
    search::common::ALL_DIRECTIONS,
    wisps::WispsGrid,
};
use logging::prelude::*;
use persistence::{
    prelude::{AppGameLoadSaveExtension, CollectSave, GameDbHelpers, LoadContext, SaveWriter},
    rusqlite,
};
use states::prelude::{GameState, MapLoadingStage};
use visuals::prelude::BuilderExplosion;
use weaponry::prelude::*;
use wisps::prelude::Wisp;

pub struct CannonballPlugin;
impl Plugin for CannonballPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Update, (
                (
                    cannonball_move_system,
                    cannonball_hit_system,
                ).run_if(in_state(GameState::Running)),
            ))
            .add_observer(on_builder_add_spawn_cannonball)
            .add_systems(CollectSave, collect_cannonballs)
            .register_loader(MapLoadingStage::SpawnMapElements, "cannonballs", load_cannonballs)
            ;
    }
}

pub(crate) const CANNONBALL_BASE_IMAGE: &str = "projectiles/cannonball.png";

fn collect_cannonballs(
    cannonballs: Query<(Entity, &Transform, &CannonballTarget, &AttackDamage), With<Cannonball>>,
    mut save: SaveWriter,
) {
    if cannonballs.is_empty() { return; }
    // Copy into owned row tuples — the closure must not borrow the World.
    let rows: Vec<(i64, f32, f32, f32, f32, f32, f32)> = cannonballs
        .iter()
        .map(|(entity, transform, target, damage)| {
            (
                entity.index_u32() as i64,
                transform.translation.x,
                transform.translation.y,
                target.target_position.x,
                target.target_position.y,
                damage.get(),
                target.initial_distance,
            )
        })
        .collect();
    save.submit(move |tx| {
        for (id, pos_x, pos_y, tgt_x, tgt_y, damage, initial_distance) in rows {
            tx.register_entity(id)?;
            tx.save_world_position(id, Vec2::new(pos_x, pos_y))?;
            tx.execute(
                "INSERT OR REPLACE INTO cannonballs (id, target_x, target_y, damage, initial_distance) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![id, tgt_x, tgt_y, damage, initial_distance],
            )?;
        }
        Ok(())
    });
}

fn load_cannonballs(ctx: &mut LoadContext) -> rusqlite::Result<()> {
    let mut stmt = ctx.conn.prepare(
        "SELECT id, target_x, target_y, damage, initial_distance FROM cannonballs",
    )?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let old_id: i64 = row.get(0)?;
        let target_x: f32 = row.get(1)?;
        let target_y: f32 = row.get(2)?;
        let damage_val: f32 = row.get(3)?;
        let initial_distance: f32 = row.get(4)?;
        let world_position = ctx.conn.get_world_position(old_id)?;

        let Some(entity) = ctx.entity(old_id) else {
            Log::warn().dev().tag(Tag::GameLoad).message(format!(
                "cannonballs: unmapped id for row {old_id}"
            ));
            continue;
        };
        let builder = BuilderCannonball::new(
            world_position,
            Vec2::new(target_x, target_y),
            AttackDamage::new(damage_val),
        )
        .with_initial_distance(initial_distance);
        ctx.insert(entity, builder);
    }
    Ok(())
}

fn on_builder_add_spawn_cannonball(
    trigger: On<Add, BuilderCannonball>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    builders: Query<&BuilderCannonball>,
) {
    let entity = trigger.entity;
    let Ok(builder) = builders.get(entity) else { return; };

    commands.entity(entity)
        .remove::<BuilderCannonball>()
        .insert((
            Sprite {
                image: asset_server.load(CANNONBALL_BASE_IMAGE),
                custom_size: Some(Vec2::new(CELL_SIZE / 2., CELL_SIZE / 2.)),
                ..default()
            },
            Transform::from_translation(builder.world_position.extend(Z_PROJECTILE)),
            Cannonball,
            CannonballTarget {
                initial_distance: builder.initial_distance,
                target_position: builder.target_position,
            },
            builder.damage.clone(),
        ));
}

fn cannonball_move_system(
    time: Res<Time>,
    mut cannonballs: Query<(&mut Transform, &CannonballTarget), With<Cannonball>>,
) {
    for (mut transform, target) in cannonballs.iter_mut() {
        let direction_vector = (target.target_position - transform.translation.xy()).normalize();
        let move_distance = direction_vector * time.delta_secs() * 400.;

        let remaining_distance = (transform.translation.xy() + move_distance).distance(target.target_position);

        // Calculate the progress as a value between 0 and 1
        let progress = 1. - remaining_distance / target.initial_distance;

        // Determine the scaling factor based on progress, applying a sine function for non-linearity
        let scale_factor = if progress <= 0.5 {
            1.0 + (PI * progress).sin()  // Non-linear scale up in the first half
        } else {
            (PI * (1.0 - progress)).sin() + 1.0  // Non-linear scale down in the second half
        };
        transform.scale = Vec3::splat(scale_factor);

        // Move the cannonball
        transform.translation += move_distance.extend(0.);
    }
}

fn cannonball_hit_system(
    mut commands: Commands,
    mut damage_messages: MessageWriter<DamageMessage>,
    wisps_grid: Res<WispsGrid>,
    cannonballs: Query<(Entity, &Transform, &CannonballTarget, &AttackDamage), With<Cannonball>>,
    wisps: Query<(), With<Wisp>>,
) {
    for (entity, cannonball_transform, target, attack_damage) in cannonballs.iter() {
        if cannonball_transform.translation.xy().distance(target.target_position) > 4. { continue; } // TODO: 1. and 2. are causing cannonballs jitters at landing. Investigate.

        let coords = GridCoords::from_transform(&cannonball_transform);
        for (dx, dy) in ALL_DIRECTIONS.iter().chain(&[(0, 0)]) {
            let blast_zone_coords = coords.shifted((*dx, *dy));
            if !blast_zone_coords.is_in_bounds(wisps_grid.bounds()) { continue; }

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
