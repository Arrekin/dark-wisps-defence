use bevy::prelude::*;

use alteration::modifiers::prelude::AttackDamage;
use game_core::prelude::{DamageMessage, GridCoords, Property};
use grids::wisps::WispsGrid;
use logging::prelude::*;
use persistence::{
    prelude::{AppGameLoadSaveExtension, CollectSave, GameDbHelpers, LoadContext, SaveWriter},
    rusqlite,
};
use states::prelude::{GameState, MapLoadingStage};
use weaponry::prelude::*;
use wisps::prelude::Wisp;

pub(crate) struct LaserDartPlugin;
impl Plugin for LaserDartPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Update, (
                laser_dart_move_system,
                laser_dart_hit_system,
            ).run_if(in_state(GameState::Running)))
            .add_observer(on_builder_add_spawn_laser_dart)
            .add_systems(CollectSave, collect_laser_darts)
            .register_loader(MapLoadingStage::SpawnMapElements, "laser_darts", load_laser_darts);
    }
}

// Flight tuning
const LASER_DART_SPEED: f32 = 600.0;
const LASER_DART_HIT_DISTANCE: f32 = 8.0;

#[log_tags(Tag::GameSave)]
fn collect_laser_darts(
    laser_darts: Query<(Entity, &Transform, &LaserDartTarget, &AttackDamage), With<LaserDart>>,
    mut save: SaveWriter,
) {
    if laser_darts.is_empty() { return; }

    #[debug_dev("Saving {} laser darts", rows.len())]
    let rows: Vec<(i64, Vec2, Option<i64>, Vec2, f32)> = laser_darts
        .iter()
        .map(|(entity, transform, target, damage)| {
            (
                entity.index_u32() as i64,
                transform.translation.xy(),
                target.target_wisp.map(|wisp| wisp.index_u32() as i64),
                target.target_vector,
                damage.get(),
            )
        })
        .collect();
    save.submit(move |tx| {
        for (id, position, target_wisp_id, target_vector, damage) in rows {
            tx.register_entity(id)?;
            tx.save_world_position(id, position)?;
            tx.execute(
                "INSERT OR REPLACE INTO laser_darts (id, target_wisp_id, vector_x, vector_y, damage) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![id, target_wisp_id, target_vector.x, target_vector.y, damage],
            )?;
        }
        Ok(())
    });
}

#[log_tags(Tag::GameLoad)]
fn load_laser_darts(ctx: &mut LoadContext) -> rusqlite::Result<()> {
    let mut stmt = ctx.conn.prepare(
        "SELECT id, target_wisp_id, vector_x, vector_y, damage FROM laser_darts",
    )?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let old_id: i64 = row.get(0)?;
        let target_wisp_old_id: Option<i64> = row.get(1)?;
        let vector_x: f32 = row.get(2)?;
        let vector_y: f32 = row.get(3)?;
        let damage: f32 = row.get(4)?;
        let world_position = ctx.conn.get_world_position(old_id)?;

        #[warn_dev("LaserDart with old ID {old_id} has no corresponding new entity")]
        let Some(entity) = ctx.entity(old_id) else { continue };
        let new_target_wisp = target_wisp_old_id.and_then(|id| ctx.entity(id));

        let builder = BuilderLaserDart::new(
            world_position,
            // new() requires an Entity; use PLACEHOLDER, then override via
            // with_target_wisp which accepts Option (including None).
            Entity::PLACEHOLDER,
            Vec2::new(vector_x, vector_y),
            AttackDamage::new(damage),
        )
        .with_target_wisp(new_target_wisp);
        ctx.insert(entity, builder);
    }
    Ok(())
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
                ..default()
            },
            Transform {
                translation: builder.world_position.extend(0.),
                rotation: Quat::from_rotation_z(builder.target_vector.y.atan2(builder.target_vector.x)),
                ..default()
            },
            LaserDart,
            LaserDartTarget { target_wisp: builder.target_wisp, target_vector: builder.target_vector },
            builder.damage,
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
        transform.translation += target.target_vector.extend(0.) * time.delta_secs() * LASER_DART_SPEED;
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
        let coords = GridCoords::from_transform(laser_dart_transform);
        if !coords.are_in_bounds(wisps_grid.bounds) {
            commands.entity(entity).despawn();
            continue;
        }
        let wisps_in_coords = &wisps_grid[coords];
        for wisp in wisps_in_coords {
            let Ok(wisp_transform) = wisps.get(*wisp) else { continue; }; // May not find wisp if the wisp spawned at the same frame.
            if laser_dart_transform.translation.xy().distance(wisp_transform.translation.xy()) < LASER_DART_HIT_DISTANCE {
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
