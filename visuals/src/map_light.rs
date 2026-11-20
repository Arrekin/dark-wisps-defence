//! Shared map sun. Mirrors `assets/shaders/common/map_light.wesl`.

use bevy::prelude::*;

/// Unit ground-plane direction toward the sun.
pub const MAP_SUN_GROUND_DIRECTION: Vec2 = Vec2::new(-0.4472, 0.8944);

/// Unit direction toward the sun, approximately 62 degrees above the ground plane.
pub const MAP_SUN_DIRECTION: Vec3 = Vec3::new(-0.2124, 0.4248, 0.8800);

/// Tangent of the sun's elevation above the ground plane, from `MAP_SUN_DIRECTION`.
pub const MAP_SUN_ELEVATION_TANGENT: f32 = 1.8529;
