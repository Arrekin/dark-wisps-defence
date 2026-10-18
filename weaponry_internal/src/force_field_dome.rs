//! # Force Field Dome
//!
//! Each `ForceField` carries a `ForceFieldDome` visual. Before the air band draws, the dome
//! refracts the ground and surface bands, then adds its light as an overlay.
//! Where domes overlap, the shader assigns each pixel to the nearest dome by distance divided
//! by radius, and draws seams where neighbours meet.
//!
//! A wisp entering a dome starts an impact ripple across that dome's shell. Each dome keeps its
//! most recent impacts in [`ForceFieldImpacts`].

use bevy::{prelude::*, reflect::TypePath, render::render_resource::{BlendState, ShaderType}};

use game_core::prelude::ZDepth;
use visuals::{
    lane::{LaneParts, LanePlugin, LaneVisual},
    shader::ADDITIVE_BLENDING,
};
use weaponry::{force_field::ForceFieldEntered, prelude::ForceField};

pub(crate) struct ForceFieldDomePlugin;
impl Plugin for ForceFieldDomePlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins(LanePlugin::<ForceFieldDome>::default())
            .add_systems(Update, update_force_field_domes)
            .add_observer(on_field_entered_record_impact);
    }
}

// Impact ripples
/// Maximum simultaneous impact ripples per dome. Mirrors `MAX_FIELD_IMPACTS` in
/// `assets/shaders/weaponry/force_field_dome.wesl`.
const MAX_FIELD_IMPACTS: usize = 8;
/// Seconds an impact ripple stays visible.
const IMPACT_LIFETIME: f32 = 3.0;

/// Dome visual stored on the force field entity. Field order and types mirror `Instance` in
/// `assets/shaders/weaponry/force_field_dome.wesl`.
#[derive(Component, ShaderType, TypePath, Clone, Default)]
#[require(ZDepth::AirEffect)]
pub(crate) struct ForceFieldDome {
    /// World-space center used to compare overlapping domes.
    center: Vec2,
    radius: f32,
    progress: f32,
    visual_noise_offset: f32,
    impact_count: u32,
    /// `xy` is the hit position, `z` the age in seconds.
    impacts: [Vec4; MAX_FIELD_IMPACTS],
}
impl LaneVisual for ForceFieldDome {
    const LABEL: &'static str = "force_field_dome";
    const LOOK: &'static str = "shaders::weaponry::force_field_dome";
    const PARTS: LaneParts = LaneParts::DISPLACE.and(LaneParts::OVERLAY);
    const READS_ALL_INSTANCES: bool = true;

    fn cut_quad_size(&self) -> Vec2 {
        Vec2::splat(2. * self.radius)
    }

    fn overlay_blend() -> BlendState {
        ADDITIVE_BLENDING
    }
}

/// Recent wisp entries into a dome, oldest first: hit position and the time it happened.
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
}

fn on_field_entered_record_impact(
    trigger: On<ForceFieldEntered>,
    time: Res<Time>,
    transforms: Query<&Transform>,
    mut impacts: Query<&mut ForceFieldImpacts>,
) {
    let event = trigger.event();
    if event.exited_field.is_some() { return; }
    let Ok(wisp_transform) = transforms.get(event.target) else { return; };
    let Ok(mut field_impacts) = impacts.get_mut(event.field) else { return; };
    field_impacts.record(wisp_transform.translation.xy(), time.elapsed_secs());
}

fn update_force_field_domes(
    time: Res<Time>,
    mut fields: Query<(&ForceField, &Transform, &ForceFieldImpacts, &mut ForceFieldDome)>,
) {
    let now = time.elapsed_secs();
    for (field, transform, field_impacts, mut dome) in fields.iter_mut() {
        let mut impacts = [Vec4::ZERO; MAX_FIELD_IMPACTS];
        let mut impact_count = 0;
        for (hit_position, impact_time) in field_impacts.0.iter() {
            let age = now - impact_time;
            if age > IMPACT_LIFETIME { continue; }
            impacts[impact_count] = hit_position.extend(age).extend(0.);
            impact_count += 1;
        }
        *dome = ForceFieldDome {
            center: transform.translation.xy(),
            radius: field.radius,
            progress: field.progress,
            visual_noise_offset: field.visual_noise_offset,
            impact_count: impact_count as u32,
            impacts,
        };
    }
}
