use bevy::prelude::*;

/// World-space render layer.
///
/// `ZDepth(z)` means: this entity renders at world Z = `z`, period — regardless of
/// where it sits in the transform hierarchy. Use the associated constants below
/// (e.g. `ZDepth::BUILDING`) as the component value.
///
/// Mechanics: local `Transform.z` inherits under parenting (world z = sum of ancestor
/// local z), so a child spawned with a layer constant in its local transform would
/// drift one parent-layer upward. Instead, `ZDepth` is enforced in world space:
/// - on insert, an observer writes local z as a cheap initial approximation
///   (exact for root entities);
/// - every frame, `enforce_zdepth_world_z` (PostUpdate, after transform propagation)
///   overwrites `GlobalTransform.z` with the `ZDepth` value.
///
/// Consequences:
/// - Nothing else should write `translation.z`. Movement systems write x/y only.
/// - Valid on children: a tower top parented to its base still lands exactly on
///   `ZDepth::TOWER_TOP` while inheriting x/y and rotation basis from the parent.
/// - Descendants of a `ZDepth` entity compute their `GlobalTransform` from the
///   *pre-enforcement* parent value, so a non-`ZDepth` child inherits the parent's
///   enforced z plus its own local offset. If you want a child on its own layer,
///   give it its own `ZDepth`.
#[derive(Component)]
#[component(immutable)]
#[require(Transform)]
pub struct ZDepth(pub f32);

macro_rules! define_z_indexes {
    // Internal macro to handle incrementing the counter
    (@internal $counter:expr, $name:ident) => {
        impl ZDepth { pub const $name: Self = Self($counter); }
    };
    (@internal $counter:expr, $name:ident, $($rest:ident),+) => {
        impl ZDepth { pub const $name: Self = Self($counter); }
        define_z_indexes!(@internal $counter + 0.001, $($rest),+);
    };
    // Public-facing macro interface
    ($($name:ident),+ $(,)?) => {
        define_z_indexes!(@internal 0.001, $($name),+);
    };
}

define_z_indexes!(
    OVERLAY_EMISSIONS,
    DARK_ORE,
    OBSTACLE,
    OVERLAY_ENERGY_SUPPLY,
    OVERLAY_TOWER_RANGES,
    BUILDING,
    WISP,
    GROUND_EFFECT,
    TOWER_TOP,
    MAP_UI,
    AERIAL_UNIT,
    PROJECTILE_UNDER,
    PROJECTILE,
    ABOVE_ALL,
);
