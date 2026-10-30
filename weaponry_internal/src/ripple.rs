//! Ripple wave effect spawned by the emitter tower.
//!
//! Each ripple carries its propagation state and a [`RippleDisplacement`] lane visual.
//! Before the air band draws, the visual distorts and brightens the scene from lower bands.
//!
//! Game logic: radial propagation, wisp hit detection, save/load.

use bevy::{prelude::*, reflect::TypePath, render::render_resource::ShaderType};

use alteration::{
    effects::{
        brittle::BuilderBrittleEffect,
        prelude::{EffectSourceOf, EffectTarget},
    },
    modifiers::prelude::MovementSpeed,
};
use game_core::prelude::{GridArea, Property};
use grids::wisps::WispsGrid;
use logging::prelude::*;
use persistence::{prelude::*, rusqlite};
use session::{GameClock, TimeBound};
use states::prelude::{GameState, MapLoadingStage};
use visuals::lane::{LaneParts, LanePlugin, LaneVisual};
use weaponry::prelude::*;
use wisps::prelude::Wisp;

pub(crate) struct RipplePlugin;
impl Plugin for RipplePlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins(LanePlugin::<RippleDisplacement>::default())
            // Chained as otherwise ripples may try to apply effects as they are removed, causing relations issues.
            .add_systems(Update, (
                ripple_propagate_system,
                ripple_hit_system,
            ).chain().run_if(in_state(GameState::Running)))
            .add_systems(Update, update_ripple_displacements)
            .add_observer(on_builder_add_spawn_ripple)
            .add_systems(CollectSave, collect_ripples)
            .register_loader(MapLoadingStage::SpawnMapElements, "ripples", load_ripples);
    }
}

// Brittle debuff parameters applied by this ripple source
const BRITTLE_DURATION_SECONDS: f64 = 10.0;
const BRITTLE_DAMAGE_MULTIPLIER: f32 = 1.5;

const RIPPLE_SPEED: f32 = 50.0;

#[log_tags(Tag::GameSave)]
fn collect_ripples(
    ripples: Query<(Entity, &Transform, &Ripple)>,
    mut save: SaveWriter,
) {
    if ripples.is_empty() { return; }

    #[debug_dev("Saving {} ripples", rows.len())]
    let rows: Vec<(u32, Vec2, f32, f32)> = ripples
        .iter()
        .map(|(entity, transform, ripple)| {
            (
                entity.index_u32(),
                transform.translation.xy(),
                ripple.max_radius,
                ripple.current_radius,
            )
        })
        .collect();
    save.submit(move |ctx| {
        for (id, position, max_radius, current_radius) in rows {
            ctx.register_entity(id)?;
            ctx.save_world_position(id, position)?;
            ctx.tx.execute(
                "INSERT OR REPLACE INTO ripples (id, max_radius, current_radius) VALUES (?1, ?2, ?3)",
                rusqlite::params![id, max_radius, current_radius],
            )?;
        }
        Ok(())
    });
}

fn load_ripples(ctx: &mut LoadContext) -> LoadResult {
    ctx.for_each_entity("SELECT id, max_radius, current_radius FROM ripples", |ctx, old_id, entity, row| {
        let max_radius: f32 = row.get(1)?;
        let current_radius: f32 = row.get(2)?;
        let world_position = ctx.world_position(old_id)?;
        let builder = BuilderRipple::new(world_position, max_radius)
            .with_current_radius(current_radius);
        ctx.insert(entity, builder);
        Ok(())
    })
}

fn on_builder_add_spawn_ripple(
    trigger: On<Add<BuilderRipple>>,
    mut commands: Commands,
    builders: Query<&BuilderRipple>,
) {
    let entity = trigger.entity;
    let Ok(builder) = builders.get(entity) else { return; };

    commands.entity(entity)
        .remove::<BuilderRipple>()
        .insert((
            Transform::from_translation(builder.world_position.extend(0.)),
            Ripple { max_radius: builder.max_radius, current_radius: builder.current_radius },
            MovementSpeed::new(RIPPLE_SPEED),
            RippleDisplacement::default(),
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
            continue;
        }
        ripple.current_radius += speed.get() * time.delta_secs();
    }
}

fn ripple_hit_system(
    mut commands: Commands,
    clock: Res<GameClock>,
    time: Res<Time>,
    wisps_grid: Res<WispsGrid>,
    ripples: Query<(Entity, &Ripple, &MovementSpeed, &Transform, Option<&EffectSourceOf>)>,
    wisps: Query<(&Transform, &MovementSpeed), With<Wisp>>,
    effect_targets: Query<&EffectTarget>,
) {
    for (ripple_entity, ripple, ripple_speed, ripple_transform, sourced_effects) in ripples.iter() {
        let covered_area = GridArea::covering_circle(ripple_transform.translation.xy(), ripple.current_radius);
        for coords in covered_area.iter_in_bounds(wisps_grid.bounds) {
            for wisp in &wisps_grid[coords] {
                if already_hit_this_target(sourced_effects, *wisp, &effect_targets) { continue; }
                let Ok((wisp_transform, wisp_speed)) = wisps.get(*wisp) else { continue; };
                let distance = wisp_transform.translation.xy().distance(ripple_transform.translation.xy());
                // Include a frame of ripple expansion and wisp movement so approaching wisps cannot skip the hit band.
                let closing_distance = (ripple_speed.get() + wisp_speed.get()) * time.delta_secs();
                if distance > ripple.current_radius || distance <= ripple.current_radius - closing_distance { continue; }
                commands.spawn(
                    BuilderBrittleEffect::new(*wisp, BRITTLE_DAMAGE_MULTIPLIER)
                        .with_source(ripple_entity)
                        .with_time_bound(TimeBound::lasting(&clock, BRITTLE_DURATION_SECONDS))
                );
            }
        }
    }
}

fn already_hit_this_target(
    sourced_effects: Option<&EffectSourceOf>,
    target_entity: Entity,
    effect_targets: &Query<&EffectTarget>,
) -> bool {
    sourced_effects.is_some_and(|effects| {
        effects.iter().any(|effect_entity| {
            effect_targets
                .get(effect_entity)
                .is_ok_and(|effect_target| effect_target.0 == target_entity)
        })
    })
}

/// Distortion data stored on the ripple entity. Field order and types mirror `Instance` in
/// `assets/shaders/weaponry/ripple_displacement.wesl`.
#[derive(Component, ShaderType, TypePath, Clone, Default)]
pub(crate) struct RippleDisplacement {
    /// Current radius as a fraction of the full diameter.
    normalized_radius: f32,
    max_radius: f32,
}
impl LaneVisual for RippleDisplacement {
    const LABEL: &'static str = "ripple_displacement";
    const LOOK: &'static str = "shaders::weaponry::ripple_displacement";
    const PARTS: LaneParts = LaneParts::DISPLACE;

    fn cut_quad_size(&self) -> Vec2 {
        Vec2::splat(2. * self.max_radius)
    }
}

fn update_ripple_displacements(mut ripples: Query<(&Ripple, &mut RippleDisplacement)>) {
    for (ripple, mut displacement) in ripples.iter_mut() {
        *displacement = RippleDisplacement { normalized_radius: ripple.normalized_radius(), max_radius: ripple.max_radius };
    }
}
