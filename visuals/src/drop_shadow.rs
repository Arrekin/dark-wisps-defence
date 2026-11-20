//! # Drop shadow
//!
//! The round shadow an object above the map casts onto it. The shadow falls on the top of each cell
//! it covers, read from the shared obstacle cells by its look (`shaders::visuals::drop_shadow`). It
//! steps toward the object on tops higher than the base cell's and away on lower ones. Below an edge
//! where a top drops, it follows a slope as steep as the sun is high, so its parts on the two tops
//! join across the edge.
//!
//! The owner decides where the object is: it spawns a `DropShadow` as a child standing at the
//! object's ground position, reports the object's position each frame with [`DropShadow::cast`],
//! and fits the child's transform with [`DropShadow::place`].

use bevy::{prelude::*, reflect::TypePath, render::render_resource::ShaderType};

use game_core::prelude::{DrawLevel, ZDepth};

use crate::{lane::LaneVisual, map_light::{MAP_SUN_ELEVATION_TANGENT, MAP_SUN_GROUND_DIRECTION}};

// Mirrors of `assets/shaders/visuals/drop_shadow.wesl`, for fitting the quad
const SHADOW_SPREAD: f32 = 0.35;
const LEVEL_STEP: f32 = 24.;
/// Levels a cell's top can lie above or below the base cell's top.
const MAX_RISE: f32 = 2.;

/// Round shadow of an object above the map, drawn by a paint lane at `OnElevation` above wall tops
/// and the wisps standing on them. Field order and types mirror `Instance` in
/// `assets/shaders/visuals/drop_shadow.wesl`.
#[derive(Component, ShaderType, TypePath, Clone)]
#[require(ZDepth::GroundEffect, DrawLevel::OnElevation)]
pub struct DropShadow {
    /// World position below the object.
    ground: Vec2,
    /// World position whose cell top `height` is measured from.
    base: Vec2,
    /// Object height above the base cell's top, in world units.
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
    /// A shadow `diameter` world units across at zero height, with heights measured from the top of
    /// the cell at `base`.
    pub fn new(diameter: f32, base: Vec2) -> Self {
        Self { ground: base, base, height: 0., diameter, opacity: 1. }
    }

    /// Places the object above `ground` at `height` over the base cell's top.
    pub fn cast(&mut self, ground: Vec2, height: f32) {
        self.ground = ground;
        self.height = height;
    }

    /// Fits `transform` to cover the shadow on any top. The entity's parent stands at `ground`.
    pub fn place(&self, transform: &mut Transform) {
        let offset = -MAP_SUN_GROUND_DIRECTION * self.height / MAP_SUN_ELEVATION_TANGENT;
        let level_shift = MAX_RISE * LEVEL_STEP / MAP_SUN_ELEVATION_TANGENT;
        transform.translation = offset.extend(transform.translation.z);
        transform.scale = Vec2::splat(self.diameter * (1. + SHADOW_SPREAD) + 2. * level_shift).extend(1.);
    }
}
