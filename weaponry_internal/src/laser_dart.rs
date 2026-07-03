use bevy::prelude::*;

use alteration::modifiers::prelude::AttackDamage;
use game_core::prelude::{DamageMessage, GridCoords, Property, Z_PROJECTILE};
use grids::prelude::WispsGrid;
use persistence::prelude::{AppGameLoadSaveExtension, SaveableBatchCommand};
use states::prelude::{GameState, MapLoadingStage};
use weaponry::{
    laser_dart::LaserDartSaveData,
    prelude::*,
};
use wisps::prelude::Wisp;

pub struct LaserDartPlugin;
impl Plugin for LaserDartPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Update, (
                (
                    laser_dart_move_system,
                    laser_dart_hit_system,
                ).run_if(in_state(GameState::Running)),
            ))
            .add_observer(on_builder_add_spawn_laser_dart)
            .register_db_loader::<BuilderLaserDart>(MapLoadingStage::SpawnMapElements)
            .register_db_saver(on_game_save_collect_laser_darts);
    }
}

fn on_game_save_collect_laser_darts(
    mut commands: Commands,
    laser_darts: Query<(Entity, &Transform, &LaserDartTarget, &AttackDamage), With<LaserDart>>,
) {
    if laser_darts.is_empty() { return; }
    let batch = laser_darts.iter().map(|(entity, transform, target, damage)| {
         let save_data = LaserDartSaveData { entity };
         BuilderLaserDart::new_for_saving(
             transform.translation.xy(),
             target.target_wisp,
             target.target_vector,
             damage.clone(),
             save_data
         )
    }).collect::<SaveableBatchCommand<_>>();
    commands.queue(batch);
}

fn on_builder_add_spawn_laser_dart(
    trigger: On<Add, BuilderLaserDart>,
    mut commands: Commands,
    builders: Query<&BuilderLaserDart>,
) {
    let entity = trigger.entity;
    let Ok(builder) = builders.get(entity) else { return; };

    commands.entity(entity)
        .remove::<BuilderLaserDart>()
        .insert((
            Sprite {
                color: Color::srgb(1.0, 0.0, 0.0),
                custom_size: Some(Vec2::new(14.0, 2.0)),
                ..Default::default()
            },
            Transform {
                translation: builder.world_position.extend(Z_PROJECTILE),
                rotation: Quat::from_rotation_z(builder.target_vector.y.atan2(builder.target_vector.x)),
                ..Default::default()
            },
            LaserDart,
            LaserDartTarget{ target_wisp: builder.target_wisp, target_vector: builder.target_vector },
            builder.damage.clone(),
        ));
}

fn laser_dart_move_system(
    time: Res<Time>,
    mut laser_darts: Query<(&mut Transform, &mut LaserDartTarget), With<LaserDart>>,
    wisps: Query<&Transform, (With<Wisp>, Without<LaserDart>)>,
) {
    for (mut transform, mut target) in laser_darts.iter_mut() {
        // If the target wisp still exists - follow it by updating the target vector
        if let Some(target_wisp) = target.target_wisp {
            if let Ok(wisp_transform) = wisps.get(target_wisp) {
                target.target_vector = (wisp_transform.translation.xy() - transform.translation.xy()).normalize();
            } else {
                target.target_wisp = None;
            }
        }
        transform.translation += target.target_vector.extend(0.) * time.delta_secs() * 600.;
    }
}

fn laser_dart_hit_system(
    mut commands: Commands,
    mut damage_messages: MessageWriter<DamageMessage>,
    wisps_grid: Res<WispsGrid>,
    laser_darts: Query<(Entity, &Transform, &AttackDamage), With<LaserDart>>,
    wisps: Query<&Transform, With<Wisp>>,
) {
    for (entity, laser_dart_transform, damage) in laser_darts.iter() {
        let coords = GridCoords::from_transform(&laser_dart_transform);
        if !coords.is_in_bounds(wisps_grid.bounds()) {
            commands.entity(entity).despawn();
            continue;
        }
        let wisps_in_coords = &wisps_grid[coords];
        for wisp in wisps_in_coords {
            let Ok(wisp_transform) = wisps.get(*wisp) else { continue; }; // May not find wisp if the wisp spawned at the same frame.
            if laser_dart_transform.translation.xy().distance(wisp_transform.translation.xy()) < 8. {
                damage_messages.write(DamageMessage {
                    target: *wisp,
                    amount: damage.get(),
                });
                commands.entity(entity).despawn();
                break;
            }
        }
    }
}
