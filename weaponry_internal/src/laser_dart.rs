use bevy::prelude::*;

use alteration::modifiers::prelude::AttackDamage;
use game_core::prelude::{AltitudeLevel, DamageMessage, GridArea, GridCoords, Property};
use indexes::{prelude::ObstacleGrid, wisps::WispsGrid};
use logging::prelude::*;
use persistence::{prelude::*, rusqlite};
use states::prelude::{GameState, MapLoadingStage};
use weaponry::{laser_dart::LASER_DART_SPEED, prelude::*};
use wisps::prelude::Wisp;

pub(crate) struct LaserDartPlugin;
impl Plugin for LaserDartPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Update, (
                move_laser_darts,
                resolve_laser_dart_hits,
            ).chain().run_if(in_state(GameState::Running)))
            .add_observer(on_builder_add_spawn_laser_dart)
            .add_systems(CollectSave, collect_laser_darts)
            .register_loader(MapLoadingStage::SpawnMapElements, "laser_darts", load_laser_darts);
    }
}

const LASER_DART_HIT_DISTANCE: f32 = 8.0;

#[log_tags(Tag::GameSave)]
fn collect_laser_darts(
    laser_darts: Query<(Entity, &Transform, &ProjectileDirection, &AttackDamage), With<LaserDart>>,
    mut save: SaveWriter,
) {
    if laser_darts.is_empty() { return; }

    #[debug_dev("Saving {} laser darts", rows.len())]
    let rows: Vec<(u32, Vec2, Vec2, f32)> = laser_darts
        .iter()
        .map(|(entity, transform, direction, damage)| {
            (
                entity.index_u32(),
                transform.translation.xy(),
                direction.0,
                damage.get(),
            )
        })
        .collect();
    save.submit(move |ctx| {
        for (id, position, direction, damage) in rows {
            ctx.register_entity(id)?;
            ctx.save_world_position(id, position)?;
            ctx.tx.execute(
                "INSERT OR REPLACE INTO laser_darts (id, vector_x, vector_y, damage) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![id, direction.x, direction.y, damage],
            )?;
        }
        Ok(())
    });
}

fn load_laser_darts(ctx: &mut LoadContext) -> LoadResult {
    ctx.for_each_entity("SELECT id, vector_x, vector_y, damage FROM laser_darts", |ctx, old_id, entity, row| {
        let direction = Vec2::new(row.get(1)?, row.get(2)?);
        let damage = AttackDamage::new(row.get(3)?);
        let world_position = ctx.world_position(old_id)?;
        ctx.insert(entity, BuilderLaserDart::new(world_position, direction, damage));
        Ok(())
    })
}

fn on_builder_add_spawn_laser_dart(
    trigger: On<Add<BuilderLaserDart>>,
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
                rotation: Quat::from_rotation_z(builder.direction.to_angle()),
                ..default()
            },
            LaserDart,
            ProjectileDirection(builder.direction),
            builder.damage,
        ));
}

fn move_laser_darts(
    time: Res<Time>,
    mut laser_darts: Query<(&mut Transform, &ProjectileDirection), With<LaserDart>>,
) {
    for (mut transform, direction) in laser_darts.iter_mut() {
        transform.translation += direction.0.extend(0.) * time.delta_secs() * LASER_DART_SPEED;
    }
}

/// Sweeps the line each dart travelled this frame. The first wisp along it at the top of a cell ends the dart.
fn resolve_laser_dart_hits(
    mut commands: Commands,
    mut damage_messages: MessageWriter<DamageMessage>,
    time: Res<Time>,
    obstacle_grid: Res<ObstacleGrid>,
    wisps_grid: Res<WispsGrid>,
    laser_darts: Query<(Entity, &Transform, &ProjectileDirection, &AttackDamage), With<LaserDart>>,
    wisps: Query<&Transform, With<Wisp>>,
) {
    let travel = LASER_DART_SPEED * time.delta_secs();
    for (entity, laser_dart_transform, direction, damage) in laser_darts.iter() {
        let end = laser_dart_transform.translation.xy();
        let start = end - direction.0 * travel;
        let swept = Rect::from_corners(start, end).inflate(LASER_DART_HIT_DISTANCE);
        let first_hit = obstacle_grid.tops_in(GridArea::covering(swept, AltitudeLevel::TopLevel))
            .flat_map(|top| wisps_grid.wisps_at(top))
            .filter_map(|&wisp| {
                // A deferred spawn can enter the grid before its components are available to queries.
                let wisp_transform = wisps.get(wisp).ok()?;
                let offset = wisp_transform.translation.xy() - start;
                let along = offset.dot(direction.0).clamp(0., travel);
                (offset.distance(direction.0 * along) < LASER_DART_HIT_DISTANCE).then_some((wisp, along))
            })
            .min_by(|(_, first), (_, second)| first.total_cmp(second));
        if let Some((wisp, _)) = first_hit {
            damage_messages.write(DamageMessage { target: wisp, amount: damage.get() });
            commands.entity(entity).despawn();
        } else if !wisps_grid.bounds.contains(GridCoords::from_world_position(end)) {
            commands.entity(entity).despawn();
        }
    }
}
