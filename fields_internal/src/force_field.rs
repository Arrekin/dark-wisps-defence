//! # Force Field
//!
//! Field towers spawn force fields through `BuilderForceField`.
//!
//! Each `ForceField` carries a `ForceFieldDome`: a transparent shell with a lit rim. Before the
//! air band draws, the dome distorts the ground and surface beneath it, then adds its light.
//! The shader uses the shared field index to draw within the field's territory, with a small
//! amount of rim light extending onto unclaimed ground.
//!
//! Wisps entering from outside all fields start impact ripples; crossing between fields does
//! not. [`ForceFieldImpacts`] stores the recent hits for each dome.

use bevy::{prelude::*, reflect::TypePath, render::render_resource::{BlendState, ShaderType}};

use fields::{
    field::{FieldEntered, FieldGeneratedBy},
    prelude::*,
};
use game_core::prelude::ZDepth;
use visuals::{
    lane::{LaneParts, LanePlugin, LaneVisual},
    shader::{ADDITIVE_BLENDING, seed_from_entity},
};

pub(crate) struct ForceFieldPlugin;
impl Plugin for ForceFieldPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins(LanePlugin::<ForceFieldDome>::default())
            .add_observer(on_builder_add_spawn_force_field)
            .add_observer(on_field_entered_record_impact)
            .add_systems(Update, update_force_field_domes.after(crate::field::resize_fields));
    }
}

// Impact ripples
/// Maximum simultaneous impact ripples per dome. Mirrors `MAX_FIELD_IMPACTS` in
/// `assets/shaders/fields/force_field_dome.wesl`.
const MAX_FIELD_IMPACTS: usize = 8;
/// Seconds an impact ripple stays visible.
const IMPACT_LIFETIME: f32 = 3.0;

// Dome
/// Maximum distance in world pixels that rim light extends onto unclaimed ground.
/// Mirrors `OUTER_REACH` in `assets/shaders/fields/force_field_dome.wesl`.
const OUTER_REACH: f32 = 10.0;

fn on_builder_add_spawn_force_field(
    trigger: On<Add<BuilderForceField>>,
    mut commands: Commands,
    builders: Query<&BuilderForceField>,
) {
    let entity = trigger.entity;
    let Ok(builder) = builders.get(entity) else { return; };

    commands.entity(entity)
        .remove::<BuilderForceField>()
        .insert((
            ForceField,
            FieldGeneratedBy(builder.tower_entity),
            Transform::from_translation(builder.world_position.extend(0.)),
            ForceFieldDome::default(),
            ForceFieldImpacts::default(),
        ));
}

// ── Dome ───────────────────────────────────────────────────────────────────────────────

/// Dome visual stored on the force field entity. Field order and types mirror `Instance` in
/// `assets/shaders/fields/force_field_dome.wesl`.
#[derive(Component, ShaderType, TypePath, Clone, Default)]
#[require(ZDepth::AirEffect)]
pub(crate) struct ForceFieldDome {
    /// Full dome radius, at least as large as the current field radius.
    /// Follows positive target radii and retains its value while the field folds to zero.
    radius: f32,
    /// Current field radius divided by the full dome radius; controls visual intensity.
    progress: f32,
    seed: f32,
    impact_count: u32,
    /// Index slot used to read the field's centre and current radius on the GPU.
    slot: u32,
    /// `xy` is the hit position, `z` the age in seconds.
    impacts: [Vec4; MAX_FIELD_IMPACTS],
}
impl LaneVisual for ForceFieldDome {
    const LABEL: &'static str = "force_field_dome";
    const LOOK: &'static str = "shaders::fields::force_field_dome";
    const PARTS: LaneParts = LaneParts::DISPLACE.and(LaneParts::OVERLAY);

    fn cut_quad_size(&self) -> Vec2 {
        Vec2::splat(2. * (self.radius + OUTER_REACH))
    }

    fn overlay_blend() -> BlendState {
        ADDITIVE_BLENDING
    }
}

/// Recent impact positions and timestamps in seconds, ordered oldest first.
#[derive(Component, Default)]
pub(crate) struct ForceFieldImpacts(Vec<(Vec2, f32)>);
impl ForceFieldImpacts {
    fn record(&mut self, hit_position: Vec2, now: f32) {
        self.0.retain(|(_, time)| now - time <= IMPACT_LIFETIME);
        if self.0.len() >= MAX_FIELD_IMPACTS {
            self.0.remove(0);
        }
        self.0.push((hit_position, now));
    }

    /// Packs unexpired impacts for the shader as `(x, y, age, 0)` and returns their count.
    fn to_instance(&self, now: f32) -> ([Vec4; MAX_FIELD_IMPACTS], u32) {
        let mut impacts = [Vec4::ZERO; MAX_FIELD_IMPACTS];
        let mut impact_count = 0;
        for (hit_position, impact_time) in self.0.iter() {
            let age = now - impact_time;
            if age > IMPACT_LIFETIME { continue; }
            impacts[impact_count] = hit_position.extend(age).extend(0.);
            impact_count += 1;
        }
        (impacts, impact_count as u32)
    }
}

fn on_field_entered_record_impact(
    trigger: On<FieldEntered>,
    time: Res<Time>,
    transforms: Query<&Transform>,
    mut impacts: Query<&mut ForceFieldImpacts>,
) {
    // Crossing a seam from a neighbouring field is not an impact.
    if trigger.exited_field.is_some() { return; }
    let Ok(wisp_transform) = transforms.get(trigger.target) else { return; };
    let Ok(mut field_impacts) = impacts.get_mut(trigger.field) else { return; };
    field_impacts.record(wisp_transform.translation.xy(), time.elapsed_secs());
}

fn update_force_field_domes(
    time: Res<Time>,
    mut fields: Query<(Entity, &Field, &FieldSlot, &ForceFieldImpacts, &mut ForceFieldDome)>,
) {
    let now = time.elapsed_secs();
    for (entity, field, slot, field_impacts, mut dome) in fields.iter_mut() {
        let (impacts, impact_count) = field_impacts.to_instance(now);
        // A folding field keeps its previous full size, so the dome fades as it shrinks.
        let full_radius = if field.target_radius > 0. { field.target_radius } else { dome.radius };
        let radius = full_radius.max(field.current_radius);
        let progress = if radius > 0. { field.current_radius / radius } else { 0. };
        *dome = ForceFieldDome {
            radius,
            progress,
            seed: seed_from_entity(entity),
            impact_count,
            slot: slot.index as u32,
            impacts,
        };
    }
}
