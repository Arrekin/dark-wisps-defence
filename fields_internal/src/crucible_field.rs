//! # Crucible Field
//!
//! Crucible towers spawn crucible fields through `BuilderCrucibleField`.
//!
//! Each `CrucibleField` carries a `CrucibleFieldDome`: a dome of heat. Before the air band draws,
//! a haze bends the ground and surface beneath it, then fire strands, glow, tendrils and embers add
//! their light. The shader uses the shared field index to draw only within the field's territory.

use bevy::{prelude::*, reflect::TypePath, render::render_resource::{BlendState, ShaderType}};

use fields::{
    field::FieldGeneratedBy,
    prelude::*,
};
use game_core::prelude::ZDepth;
use visuals::{
    lane::{LaneParts, LanePlugin, LaneVisual},
    shader::{ADDITIVE_BLENDING, seed_from_entity},
};

pub(crate) struct CrucibleFieldPlugin;
impl Plugin for CrucibleFieldPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins(LanePlugin::<CrucibleFieldDome>::default())
            .add_observer(on_builder_add_spawn_crucible_field)
            .add_systems(Update, update_crucible_field_domes.after(crate::field::resize_fields));
    }
}

fn on_builder_add_spawn_crucible_field(
    trigger: On<Add<BuilderCrucibleField>>,
    mut commands: Commands,
    builders: Query<&BuilderCrucibleField>,
) {
    let entity = trigger.entity;
    let Ok(builder) = builders.get(entity) else { return; };

    commands.entity(entity)
        .remove::<BuilderCrucibleField>()
        .insert((
            CrucibleField,
            FieldGeneratedBy(builder.tower_entity),
            Transform::from_translation(builder.world_position.extend(0.)),
            CrucibleFieldDome::default(),
        ));
}

// ── Dome ───────────────────────────────────────────────────────────────────────────────

/// Dome visual stored on the crucible field entity. Field order and types mirror `Instance` in
/// `assets/shaders/fields/crucible_field_dome.wesl`.
#[derive(Component, ShaderType, TypePath, Clone, Default)]
#[require(ZDepth::AirEffect)]
pub(crate) struct CrucibleFieldDome {
    /// Full dome radius, at least as large as the current field radius.
    /// Follows positive target radii and retains its value while the field folds to zero.
    radius: f32,
    /// Current field radius divided by the full dome radius; controls visual intensity.
    progress: f32,
    seed: f32,
    /// Index slot used to read the field's centre and current radius on the GPU.
    slot: u32,
}
impl LaneVisual for CrucibleFieldDome {
    const LABEL: &'static str = "crucible_field_dome";
    const LOOK: &'static str = "shaders::fields::crucible_field_dome";
    const PARTS: LaneParts = LaneParts::DISPLACE.and(LaneParts::OVERLAY);

    fn cut_quad_size(&self) -> Vec2 {
        Vec2::splat(2. * self.radius)
    }

    fn overlay_blend() -> BlendState {
        ADDITIVE_BLENDING
    }
}

fn update_crucible_field_domes(
    mut fields: Query<(Entity, &Field, &FieldSlot, &mut CrucibleFieldDome)>,
) {
    for (entity, field, slot, mut dome) in fields.iter_mut() {
        // A folding field keeps its previous full size, so the dome fades as it shrinks.
        let full_radius = if field.target_radius > 0. { field.target_radius } else { dome.radius };
        let radius = full_radius.max(field.current_radius);
        let progress = if radius > 0. { field.current_radius / radius } else { 0. };
        *dome = CrucibleFieldDome {
            radius,
            progress,
            seed: seed_from_entity(entity),
            slot: slot.index as u32,
        };
    }
}
