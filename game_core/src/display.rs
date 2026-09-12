use bevy::prelude::*;

use crate::types::MapObject;

// ============================================================================
// Display vocabulary
//
// Shared components describing how something presents itself. Domains attach
// these to their own entities; consumers (side menu, tooltips, panels) read
// them without knowing which domain produced them.
// ============================================================================

#[derive(Component, Clone, Debug, Default)]
pub struct DisplayName(pub String);

#[derive(Component, Clone, Debug, Default)]
pub struct DisplayDescription(pub String);

/// Path to the icon image, resolved into [`DisplayIcon`] when inserted.
#[derive(Component, Clone, Debug, Default)]
pub struct DisplayIconSwitcher(pub String);

#[derive(Component, Clone, Debug)]
pub struct DisplayIcon(pub Handle<Image>);

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct DisplayOrder(pub u32);

// ============================================================================
// Object faces
// ============================================================================

/// Render target for an object face.
///
/// UI faces use UI components such as `ImageNode` or `MaterialNode`. World faces use 2D world
/// components such as `Sprite` or `MeshMaterial2d`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FaceSurface {
    Ui,
    World,
}

/// Opacity a placement ghost is drawn at, so the ground under it stays readable.
pub const GHOST_ALPHA: f32 = 0.5;

/// Requests an object's domain renderer to attach a face to `entity`.
///
/// Renderers match `object` and `surface`. `alpha` applies to the entire face; placement ghosts use
/// [`GHOST_ALPHA`]. The target must already have any components required by its renderer, such as
/// `GridImprint` for world-space faces.
#[derive(EntityEvent, Clone, Copy, Debug)]
pub struct ObjectFaceRequest {
    pub entity: Entity,
    pub object: MapObject,
    pub surface: FaceSurface,
    pub alpha: f32,
}

impl ObjectFaceRequest {
    /// Requests a fully opaque UI face.
    pub fn ui(entity: Entity, object: MapObject) -> Self {
        Self { entity, object, surface: FaceSurface::Ui, alpha: 1.0 }
    }

    /// Requests a fully opaque world-space face.
    pub fn world(entity: Entity, object: MapObject) -> Self {
        Self { entity, object, surface: FaceSurface::World, alpha: 1.0 }
    }

    /// Requests a world-space face with placement-ghost opacity.
    pub fn ghost(entity: Entity, object: MapObject) -> Self {
        Self { entity, object, surface: FaceSurface::World, alpha: GHOST_ALPHA }
    }
}
