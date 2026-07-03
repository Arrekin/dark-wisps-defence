use bevy::prelude::*;

#[derive(Component)]
#[component(immutable)]
#[require(Transform)]
pub struct ZDepth(pub f32);
impl From<f32> for ZDepth {
    fn from(value: f32) -> Self {
        Self(value)
    }
}

macro_rules! define_z_indexes {
    // Internal macro to handle incrementing the counter
    (@internal $counter:expr, $name:ident) => {
        pub const $name: f32 = $counter;
    };
    (@internal $counter:expr, $name:ident, $($rest:ident),+) => {
        pub const $name: f32 = $counter;
        define_z_indexes!(@internal $counter + 0.001, $($rest),+);
    };
    // Public-facing macro interface
    ($($name:ident),+) => {
        define_z_indexes!(@internal 0.001, $($name),+);
    };
}

define_z_indexes!(
    Z_OVERLAY_EMISSIONS,
    Z_OBSTACLE,
    Z_OVERLAY_ENERGY_SUPPLY,
    Z_OVERLAY_TOWER_RANGES,
    Z_BUILDING,
    Z_WISP,
    Z_GROUND_EFFECT,
    Z_TOWER_TOP,
    Z_MAP_UI,
    Z_AERIAL_UNIT,
    Z_PROJECTILE_UNDER,
    Z_PROJECTILE,
    Z_ABOVE_ALL
);
