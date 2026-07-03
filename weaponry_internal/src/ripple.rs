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
use grids::prelude::WispsGrid;
use persistence::prelude::{AppGameLoadSaveExtension, SaveableBatchCommand};
use session::GameClock;
use states::prelude::{GameState, MapLoadingStage};
use weaponry::{
    prelude::*,
    ripple::RippleSaveData,
};
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
            .register_db_loader::<BuilderRipple>(MapLoadingStage::SpawnMapElements)
            .register_db_saver(on_game_save_collect_ripples)
            ;
    }
}

fn on_game_save_collect_ripples(
    mut commands: Commands,
    ripples: Query<(Entity, &Transform, &Ripple)>,
) {
    if ripples.is_empty() { return; }
    let batch = ripples.iter().map(|(entity, transform, ripple)| {
         let save_data = RippleSaveData {
             entity,
             current_radius: ripple.current_radius,
         };
         BuilderRipple::new_for_saving(
             transform.translation.xy(),
             ripple.max_radius,
             save_data
         )
    }).collect::<SaveableBatchCommand<_>>();
    commands.queue(batch);
}

fn on_builder_add_spawn_ripple(
    trigger: On<Add, BuilderRipple>,
    mut commands: Commands,
    builders: Query<&BuilderRipple>,
) {
    let entity = trigger.entity;
    let Ok(builder) = builders.get(entity) else { return; };

    let current_radius = builder.save_data.as_ref().map_or(0., |d| d.current_radius);

    commands.entity(entity)
        .remove::<BuilderRipple>()
        .insert((
            Transform::from_translation(builder.world_position.extend(Z_ABOVE_ALL)),
            Ripple { max_radius: builder.radius, current_radius },
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
        let starting_grid_coords = GridCoords::from_transform(&ripple_transform);
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
