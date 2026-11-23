//! # Lob Preview Paths
//!
//! Renders each [`LobPreviewShown`] arc using two instanced paint-lane quads: [`LobPreviewBody`]
//! for the airborne dot trail and [`LobPreviewShadow`] for the shadow trail cast onto cell tops.
//!
//! Path entities link to their previewed owner via [`LobPreviewPathOf`] (`linked_spawn`) rather
//! than [`ChildOf`], because [`LobPreviewShown`] can be inserted before a builder's `children![..]`
//! bundle runs (which replaces [`Children`]).
//!
//! The shaders evaluate dot positions in world space; this module updates each path's [`LobArc`]
//! instance data and fits its oriented bounding quad:
//! - [`LobPreviewBody`] stays colinear with the ground chord, requiring only a narrow strip padded
//!   by the peak perspective-scaled dot radius.
//! - [`LobPreviewShadow`] shifts away from the sun in proportion to height above the surface, so
//!   its quad expands along the leeward sun vector to cover the peak apex and maximum elevation drop.

use bevy::{ecs::component::Mutable, prelude::*, reflect::TypePath, render::render_resource::ShaderType};

use game_core::prelude::{DrawLevel, ZDepth};
use visuals::{
    drop_shadow::{LEVEL_STEP, MAX_RISE, SHADOW_SPREAD},
    lane::{LanePlugin, LaneVisual},
    map_light::{MAP_SUN_ELEVATION_TANGENT, MAP_SUN_GROUND_DIRECTION},
};
use weaponry::lob::{LobArc, LobPreviewShown, LobPreviewSystems};

use crate::lob::LobBody;

pub(crate) struct LobPreviewPathsPlugin;
impl Plugin for LobPreviewPathsPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                LanePlugin::<LobPreviewBody>::default(),
                LanePlugin::<LobPreviewShadow>::default(),
            ))
            .add_systems(Update, (
                update_lob_preview_paths::<LobPreviewBody>,
                update_lob_preview_paths::<LobPreviewShadow>,
            ).after(LobPreviewSystems).run_if(any_with_component::<LobPreviewShown>))
            .add_observer(on_add_lob_preview_shown_spawn_paths)
            .add_observer(on_remove_lob_preview_shown_despawn_paths);
    }
}

// Mirrored from `lob_preview_body.wesl` and `lob_preview_shadow.wesl` for quad bounds calculation.
const BODY_DOT_DIAMETER: f32 = 4.;
const SHADOW_DOT_DIAMETER: f32 = 6.;

/// Links a preview path visual entity to its [`LobPreviewShown`] owner.
#[derive(Component)]
#[relationship(relationship_target = LobPreviewPaths)]
struct LobPreviewPathOf(Entity);

/// Preview path visual entities owned by this [`LobPreviewShown`] entity.
#[derive(Component)]
#[relationship_target(relationship = LobPreviewPathOf, linked_spawn)]
struct LobPreviewPaths(Vec<Entity>);

/// Airborne dot trail instance for a lob preview.
///
/// Field order and types mirror `Instance` in `assets/shaders/weaponry/lob_preview_body.wesl`.
#[derive(Component, ShaderType, TypePath, Clone, Default, PartialEq)]
#[require(ZDepth::ProjectileUnder, DrawLevel::Airborne, Visibility::Hidden)]
struct LobPreviewBody {
    origin: Vec2,
    landing: Vec2,
    /// Peak height above ground, in world units.
    apex: f32,
}
impl LaneVisual for LobPreviewBody {
    const LABEL: &'static str = "lob_preview_body";
    const LOOK: &'static str = "shaders::weaponry::lob_preview_body";
}

/// Ground-projected shadow dot trail instance for a lob preview.
///
/// Field order and types mirror `Instance` in `assets/shaders/weaponry/lob_preview_shadow.wesl`.
#[derive(Component, ShaderType, TypePath, Clone, Default, PartialEq)]
#[require(ZDepth::GroundEffect, DrawLevel::OnElevation, Visibility::Hidden)]
struct LobPreviewShadow {
    origin: Vec2,
    /// Landing position and reference cell for surface elevation.
    landing: Vec2,
    /// Peak height above ground, in world units.
    apex: f32,
}
impl LaneVisual for LobPreviewShadow {
    const LABEL: &'static str = "lob_preview_shadow";
    const LOOK: &'static str = "shaders::weaponry::lob_preview_shadow";
}

fn on_add_lob_preview_shown_spawn_paths(
    trigger: On<Add<LobPreviewShown>>,
    mut commands: Commands,
) {
    let owner = trigger.entity;
    commands.spawn((LobPreviewBody::default(), LobPreviewPathOf(owner)));
    commands.spawn((LobPreviewShadow::default(), LobPreviewPathOf(owner)));
}

fn on_remove_lob_preview_shown_despawn_paths(
    trigger: On<Remove<LobPreviewShown>>,
    mut commands: Commands,
    previews: Query<&LobPreviewPaths>,
) {
    let Ok(paths) = previews.get(trigger.entity) else { return; };
    for path in paths.iter() {
        commands.entity(path).try_despawn();
    }
}

/// Builds lane instance data and a bounding quad [`Transform`] for a preview [`LobArc`].
trait LobPreviewPath: Component<Mutability = Mutable> + PartialEq {
    fn from_arc(arc: &LobArc) -> Self;
    /// Returns the world [`Transform`] for a unit quad covering this path's dots for `arc`.
    fn fit(arc: &LobArc) -> Transform;
}

/// Syncs instance data, quad transform, and visibility of each `T` path from its owner's
/// [`LobPreviewShown`].
fn update_lob_preview_paths<T: LobPreviewPath>(
    previews: Query<(&LobPreviewShown, &InheritedVisibility, &LobPreviewPaths)>,
    mut paths: Query<(&mut T, &mut Transform, &mut Visibility)>,
) {
    for (shown, owner_visibility, owned_paths) in previews.iter() {
        let arc = shown.arc.filter(|_| owner_visibility.get());
        for path in owned_paths.iter() {
            let Ok((mut instance, mut transform, mut visibility)) = paths.get_mut(path) else { continue; };
            let Some(arc) = arc else { visibility.set_if_neq(Visibility::Hidden); continue; };
            instance.set_if_neq(T::from_arc(&arc));
            transform.set_if_neq(T::fit(&arc));
            visibility.set_if_neq(Visibility::Inherited);
        }
    }
}

/// Computes the world [`Transform`] for a unit quad covering `[along.0, along.1]` (along
/// `arc.heading()`) by `[across.0, across.1]` (to its left), measured from the chord midpoint.
fn strip(arc: &LobArc, along: (f32, f32), across: (f32, f32)) -> Transform {
    let heading = arc.heading();
    let left = heading.perp();
    let midpoint = arc.origin.midpoint(arc.target);
    let centre = midpoint + heading * (along.0 + along.1) / 2. + left * (across.0 + across.1) / 2.;
    Transform {
        translation: centre.extend(0.),
        rotation: Quat::from_rotation_z(heading.to_angle()),
        scale: Vec3::new(along.1 - along.0, across.1 - across.0, 1.),
    }
}

impl LobPreviewPath for LobPreviewBody {
    fn from_arc(arc: &LobArc) -> Self {
        Self { origin: arc.origin, landing: arc.target, apex: arc.apex }
    }

    /// Bounds the airborne dots along the origin-to-target chord, padded by the peak
    /// perspective-scaled dot radius.
    fn fit(arc: &LobArc) -> Transform {
        let peak_scale = LobBody::scale_at(arc.apex);
        let margin = 0.5 * BODY_DOT_DIAMETER * peak_scale;
        let half_length = 0.5 * arc.origin.distance(arc.target);
        strip(arc, (-half_length - margin, half_length + margin), (-margin, margin))
    }
}

impl LobPreviewPath for LobPreviewShadow {
    fn from_arc(arc: &LobArc) -> Self {
        Self { origin: arc.origin, landing: arc.target, apex: arc.apex }
    }

    /// Bounds the shadow trail across the chord and its maximum leeward sun offset (at `apex` over
    /// a `MAX_RISE` elevation drop).
    fn fit(arc: &LobArc) -> Transform {
        let heading = arc.heading();
        let reach = -MAP_SUN_GROUND_DIRECTION * (arc.apex + MAX_RISE * LEVEL_STEP) / MAP_SUN_ELEVATION_TANGENT;
        let (reach_along, reach_across) = (reach.dot(heading), reach.dot(heading.perp()));
        let margin = 0.5 * SHADOW_DOT_DIAMETER * (1. + SHADOW_SPREAD);
        let half_length = 0.5 * arc.origin.distance(arc.target);
        strip(
            arc,
            (-half_length + reach_along.min(0.) - margin, half_length + reach_along.max(0.) + margin),
            (reach_across.min(0.) - margin, reach_across.max(0.) + margin),
        )
    }
}
