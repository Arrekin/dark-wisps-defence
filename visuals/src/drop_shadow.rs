//! # Drop Shadow
//!
//! Circular shadow cast by an airborne entity onto cell top surfaces via
//! `shaders::visuals::drop_shadow` and `shaders::visuals::drop_shadow_cast`.
//!
//! Heights are measured relative to a reference `base` cell's top surface so the shadow meets the
//! caster when `height` reaches zero on that surface. Across cells of different elevations, the
//! shader shifts the shadow offset per level and ramps across leeward elevation drops so the shadow
//! stays connected.
//!
//! Spawn [`DropShadow`] as a child of an entity positioned at the caster's ground projection. Each
//! frame, update the shadow with [`DropShadow::cast`] and size/offset its quad with
//! [`DropShadow::place`].

use bevy::{prelude::*, reflect::TypePath, render::render_resource::ShaderType};

use game_core::prelude::{DrawLevel, ZDepth};

use crate::{lane::LaneVisual, map_light::{MAP_SUN_ELEVATION_TANGENT, MAP_SUN_GROUND_DIRECTION}};

// Mirrored from `assets/shaders/visuals/drop_shadow_cast.wesl` to bound shadow quad transforms.
/// Fractional shadow diameter increase at maximum height spread.
pub const SHADOW_SPREAD: f32 = 0.35;
/// Vertical height per elevation level, in world units.
pub const LEVEL_STEP: f32 = 24.;
/// Maximum level difference between any cell top and the base cell (`top_level` spans `0..=2`).
pub const MAX_RISE: f32 = 2.;

/// Circular drop shadow rendered at [`DrawLevel::OnElevation`] (above walls and ground wisps).
///
/// Field order and types mirror `Instance` in `assets/shaders/visuals/drop_shadow.wesl`.
#[derive(Component, ShaderType, TypePath, Clone)]
#[require(ZDepth::GroundEffect, DrawLevel::OnElevation)]
pub struct DropShadow {
    /// World position directly below the caster.
    ground: Vec2,
    /// World position of the reference cell whose top `height` is measured from.
    base: Vec2,
    /// Caster height above the `base` cell's top surface, in world units.
    height: f32,
    /// Shadow diameter at zero height, in world units.
    diameter: f32,
    opacity: f32,
}
impl LaneVisual for DropShadow {
    const LABEL: &'static str = "drop_shadow";
    const LOOK: &'static str = "shaders::visuals::drop_shadow";
}
impl DropShadow {
    /// Creates a shadow with zero-height `diameter` (world units) and heights measured relative to
    /// the cell top at `base`.
    pub fn new(diameter: f32, base: Vec2) -> Self {
        Self { ground: base, base, height: 0., diameter, opacity: 1. }
    }

    /// Updates the caster's `ground` projection and `height` above the base cell's top.
    pub fn cast(&mut self, ground: Vec2, height: f32) {
        self.ground = ground;
        self.height = height;
    }

    /// Positions and scales `transform` (relative to a parent at `ground`) so the quad covers the
    /// shadow across all possible surface elevations.
    pub fn place(&self, transform: &mut Transform) {
        let offset = -MAP_SUN_GROUND_DIRECTION * self.height / MAP_SUN_ELEVATION_TANGENT;
        let level_shift = MAX_RISE * LEVEL_STEP / MAP_SUN_ELEVATION_TANGENT;
        transform.translation = offset.extend(transform.translation.z);
        transform.scale = Vec2::splat(self.diameter * (1. + SHADOW_SPREAD) + 2. * level_shift).extend(1.);
    }
}
