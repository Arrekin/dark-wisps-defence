use std::f32::consts::PI;

use bevy::prelude::*;

use alteration::modifiers::prelude::AttackDamage;
use game_core::prelude::{CELL_SIZE, DamageMessage, GridCoords, Property, Z_PROJECTILE};
use grids::{
    prelude::WispsGrid,
    search::common::ALL_DIRECTIONS,
};
use persistence::prelude::{AppGameLoadSaveExtension, SaveableBatchCommand};
use states::prelude::{GameState, MapLoadingStage};
use visuals::prelude::BuilderExplosion;
use weaponry::{
    cannonball::CannonballSaveData,
    prelude::*,
};
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
            .register_db_loader::<BuilderCannonball>(MapLoadingStage::SpawnMapElements)
            .register_db_saver(on_game_save_collect_cannonballs);
    }
}

pub(crate) const CANNONBALL_BASE_IMAGE: &str = "projectiles/cannonball.png";

fn on_game_save_collect_cannonballs(
    mut commands: Commands,
    cannonballs: Query<(Entity, &Transform, &CannonballTarget, &AttackDamage), With<Cannonball>>,
) {
    if cannonballs.is_empty() { return; }
    let batch = cannonballs.iter().map(|(entity, transform, target, damage)| {
         let save_data = CannonballSaveData {
             entity,
             initial_distance: target.initial_distance,
         };
         BuilderCannonball::new_for_saving(
             transform.translation.xy(),
             target.target_position,
             damage.clone(),
             save_data
         )
    }).collect::<SaveableBatchCommand<_>>();
    commands.queue(batch);
}

fn on_builder_add_spawn_cannonball(
    trigger: On<Add, BuilderCannonball>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    builders: Query<&BuilderCannonball>,
) {
    let entity = trigger.entity;
    let Ok(builder) = builders.get(entity) else { return; };

    let initial_distance = if let Some(save_data) = &builder.save_data {
        save_data.initial_distance
    } else {
        builder.world_position.distance(builder.target_position)
    };

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
                initial_distance,
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
