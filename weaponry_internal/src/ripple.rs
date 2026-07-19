//! Ripple wave effect spawned by the emitter tower.
//!
//! Each active ripple is a pure-data entity (`Ripple` + `Transform` + `MovementSpeed`)
//! with no mesh or material. Rendering is handled by [`super::ripple_post_process`],
//! which uploads all ripples into a shared GPU storage buffer and runs a fullscreen
//! displacement pass after tonemapping.
//!
//! Game logic: radial propagation, wisp hit detection, save/load.

use bevy::prelude::*;

use alteration::{
    effects::{
        brittle::BuilderBrittleEffect,
        prelude::{EffectSourceOf, EffectTarget, ExpiresAt},
    },
    modifiers::prelude::MovementSpeed,
};
use game_core::prelude::{CELL_SIZE, GridCoords, Property, Z_ABOVE_ALL};
use grids::wisps::WispsGrid;
use logging::prelude::*;
use persistence::{
    prelude::{AppGameLoadSaveExtension, CollectSave, GameDbHelpers, LoadContext, SaveWriter},
    rusqlite,
};
use session::GameClock;
use states::prelude::{GameState, MapLoadingStage};
use weaponry::prelude::*;
use wisps::prelude::Wisp;

// Brittle debuff parameters applied by this ripple source
const BRITTLE_DURATION_SECS: f64 = 10.0;
const BRITTLE_DAMAGE_MULTIPLIER: f32 = 1.5;

pub struct RipplePlugin;
impl Plugin for RipplePlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Update, (
                (
                    ripple_propagate_system,
                    ripple_hit_system,
                ).chain().run_if(in_state(GameState::Running)), // Chained as otherwise ripples may try to apply effects as they are removed, causing relations issues.
            ))
            .add_observer(on_builder_add_spawn_ripple)
            .add_systems(CollectSave, collect_ripples)
            .register_loader(MapLoadingStage::SpawnMapElements, "ripples", load_ripples)
            ;
    }
}

fn collect_ripples(
    ripples: Query<(Entity, &Transform, &Ripple)>,
    mut save: SaveWriter,
) {
    if ripples.is_empty() { return; }
    let rows: Vec<(i64, f32, f32, f32, f32)> = ripples
        .iter()
        .map(|(entity, transform, ripple)| {
            (
                entity.index_u32() as i64,
                transform.translation.x,
                transform.translation.y,
                ripple.max_radius,
                ripple.current_radius,
            )
        })
        .collect();
    save.submit(move |tx| {
        for (id, pos_x, pos_y, max_radius, current_radius) in rows {
            tx.register_entity(id)?;
            tx.save_world_position(id, Vec2::new(pos_x, pos_y))?;
            tx.execute(
                "INSERT OR REPLACE INTO ripples (id, max_radius, current_radius) VALUES (?1, ?2, ?3)",
                rusqlite::params![id, max_radius, current_radius],
            )?;
        }
        Ok(())
    });
}

fn load_ripples(ctx: &mut LoadContext) -> rusqlite::Result<()> {
    let mut stmt = ctx.conn.prepare("SELECT id, max_radius, current_radius FROM ripples")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let old_id: i64 = row.get(0)?;
        let max_radius: f32 = row.get(1)?;
        let current_radius: f32 = row.get(2)?;
        let world_position = ctx.conn.get_world_position(old_id)?;

        let Some(entity) = ctx.entity(old_id) else {
            Log::warn().dev().tag(Tag::GameLoad).message(format!(
                "ripples: unmapped id for row {old_id}"
            ));
            continue;
        };
        let builder = BuilderRipple::new(world_position, max_radius)
            .with_current_radius(current_radius);
        ctx.insert(entity, builder);
    }
    Ok(())
}

fn on_builder_add_spawn_ripple(
    trigger: On<Add, BuilderRipple>,
    mut commands: Commands,
    builders: Query<&BuilderRipple>,
) {
    let entity = trigger.entity;
    let Ok(builder) = builders.get(entity) else { return; };

    commands.entity(entity)
        .remove::<BuilderRipple>()
        .insert((
            Transform::from_translation(builder.world_position.extend(Z_ABOVE_ALL)),
            Ripple { max_radius: builder.radius, current_radius: builder.current_radius },
            MovementSpeed::new(50.0),
        ));
}

fn ripple_propagate_system(
    mut commands: Commands,
    time: Res<Time>,
    mut ripples: Query<(Entity, &mut Ripple, &MovementSpeed)>,
) {
    for (entity, mut ripple, speed) in ripples.iter_mut() {
        if ripple.current_radius > ripple.max_radius {
            commands.entity(entity).despawn();
        }
        ripple.current_radius += speed.get() * time.delta_secs();
    }
}

fn ripple_hit_system(
    mut commands: Commands,
    game_clock: Res<GameClock>,
    wisps_grid: Res<WispsGrid>,
    ripples: Query<(Entity, &Ripple, &Transform, Option<&EffectSourceOf>)>,
    wisps: Query<&Transform, With<Wisp>>,
    effect_targets: Query<&EffectTarget>,
) {
    for (ripple_entity, ripple, ripple_transform, sourced_effects) in ripples.iter() {
        // Check all fields covered by the ripple for wisp collisions
        let starting_grid_coords = GridCoords::from_transform(ripple_transform);
        let bounds_range = (ripple.current_radius / CELL_SIZE) as i32;
        // Make bounds +/-1 since the ripple starts from in-between the grid fields
        let lower_bound_x = std::cmp::max(0, starting_grid_coords.x - bounds_range - 1);
        let lower_bound_y = std::cmp::max(0, starting_grid_coords.y - bounds_range - 1);
        let upper_bound_x = std::cmp::min(wisps_grid.width - 1, starting_grid_coords.x + bounds_range + 1);
        let upper_bound_y = std::cmp::min(wisps_grid.height - 1, starting_grid_coords.y + bounds_range + 1);
        for x in lower_bound_x..=upper_bound_x {
            for y in lower_bound_y..=upper_bound_y {
                for wisp in &wisps_grid[GridCoords{ x, y }] {
                    if already_hit_this_target(sourced_effects, *wisp, &effect_targets) { continue; }
                    let Ok(wisp_transform) = wisps.get(*wisp) else { continue; };
                    let distance = wisp_transform.translation.distance(ripple_transform.translation);
                    // Hit only wisps within 1 world unit of the leading edge
                    if distance > ripple.current_radius || distance < ripple.current_radius - 1. { continue; }
                    commands.spawn(
                        BuilderBrittleEffect::new(*wisp, BRITTLE_DAMAGE_MULTIPLIER)
                            .with_source(ripple_entity)
                            .with_expiry(ExpiresAt(game_clock.elapsed + BRITTLE_DURATION_SECS))
                    );
                }
            }
        }
    }
}

fn already_hit_this_target(
    sourced_effects: Option<&EffectSourceOf>,
    target_entity: Entity,
    effect_targets: &Query<&EffectTarget>,
) -> bool {
    sourced_effects
        .map(|effects| {
            effects.iter().any(|effect_entity| {
                effect_targets
                    .get(effect_entity)
                    .is_ok_and(|effect_target| effect_target.0 == target_entity)
            })
        })
        .unwrap_or(false)
}
