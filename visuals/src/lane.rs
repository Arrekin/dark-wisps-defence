//! # Lanes
//!
//! A visual kind pairs a component implementing [`LaneVisual`] with a WESL look module.
//! Each entity carrying the component is an instance. [`LanePlugin`] renders the parts
//! selected by [`LaneParts`]:
//!
//! | Part | Look function | What it does | Drawn |
//! |---|---|---|---|
//! | `PAINT` | `look(in, instance) -> vec4` | colour, at the instance's own depth | in its band, by the paint lane |
//! | `DISPLACE` | `displace(in, instance) -> vec4` | distorts the scene from lower bands | before its band |
//! | `OVERLAY` | `overlay(in, instance) -> vec4` | draws over the distorted scene, with access to its snapshot | before its band |
//!
//! ## Shader data
//!
//! - **Instance:** the component's data, mirrored by the WESL `Instance` struct.
//! - **Kind:** [`LaneVisual::Material`], for `PAINT`: textures, uniforms and buffers shared by all instances.
//! - **World:** the shared GPU group ([`crate::shared_gpu`]), available to every part.
//! - **Beneath:** the band's snapshot, through `shaders::common::beneath`, for `OVERLAY` and for a
//!   `PAINT` kind that sets [`LaneVisual::READS_BENEATH`].
//!
//! Every function receives Bevy's `VertexOutput`: `uv` over the quad, `world_position`, and
//! `instance_index`. For cut parts, `instance_index` addresses the list gathered for the frame.
//! [`LaneVisual::READS_ALL_INSTANCES`] lets the shader compare those instances, for example to
//! assign each pixel to one overlapping instance.
//!
//! ## Layers
//!
//! An instance's `ZDepth` selects its band. Cut parts act on the scene drawn by lower bands.
//! Bevy batches adjacent `PAINT` instances of the same kind in the sorted phase. A dedicated
//! layer keeps them together; a shared layer can split them into several draws.

use core::{hash::{Hash, Hasher}, marker::PhantomData};
use std::hash::DefaultHasher;

use bevy::{
    asset::uuid::Uuid,
    ecs::component::Mutable,
    prelude::*,
    reflect::TypePath,
    render::{
        RenderApp,
        extract_resource::ExtractResource,
        render_resource::{AsBindGroup, BlendState, ShaderSize, ShaderType, encase::internal::WriteInto},
    },
    shader::Shader,
    sprite_render::{ExtendedMaterial2d, MaterialExtension2d},
};

use game_core::z_depth::{Band, PerBand};

use crate::{cut_lane, paint_lane::{self, PaintMaterial}};

/// Draws the parts of kind `K`.
pub struct LanePlugin<K: LaneVisual>(PhantomData<K>);
impl<K: LaneVisual> Default for LanePlugin<K> {
    fn default() -> Self {
        Self(PhantomData)
    }
}
impl<K: LaneVisual> Plugin for LanePlugin<K> {
    fn build(&self, app: &mut App) {
        assert!(
            !(K::READS_ALL_INSTANCES && K::PARTS.has(LaneParts::PAINT)),
            "lane kind {}: READS_ALL_INSTANCES is for cut parts only",
            K::LABEL,
        );
        if K::PARTS.has(LaneParts::PAINT) {
            paint_lane::build::<K>(app);
        }
        if K::PARTS.has(LaneParts::DISPLACE) || K::PARTS.has(LaneParts::OVERLAY) {
            cut_lane::build::<K>(app);
        }
    }
}

/// Rendering operations used by a visual kind. Combine with [`LaneParts::and`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LaneParts(u8);
impl LaneParts {
    pub const PAINT: Self = Self(1);
    pub const DISPLACE: Self = Self(1 << 1);
    pub const OVERLAY: Self = Self(1 << 2);

    pub const fn and(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn has(self, part: Self) -> bool {
        self.0 & part.0 == part.0
    }
}

/// A visual kind drawn by a lane. The component is one instance's data.
pub trait LaneVisual: Component<Mutability = Mutable> + ShaderType + ShaderSize + WriteInto + TypePath + Clone {
    /// Unique name of the kind, used for labels and the generated entry shaders.
    const LABEL: &'static str;
    /// WESL module path of the look, for example `shaders::weaponry::fireball_glow`. It provides
    /// `struct Instance` and one function per part.
    const LOOK: &'static str;
    const PARTS: LaneParts = LaneParts::PAINT;
    /// Requests a snapshot of lower bands for `PAINT` to sample through `shaders::common::beneath`.
    /// A snapshot is taken in each band containing visible instances of this kind.
    const READS_BENEATH: bool = false;
    /// Whether the look module declares the instance array itself, as
    /// `@group(2) @binding(0) var<storage, read> instances: array<Instance>`.
    /// The array contains the cut instances gathered for the frame. This flag cannot be used
    /// with `PAINT`, whose buffer retains stale slots.
    const READS_ALL_INSTANCES: bool = false;

    /// Textures, uniforms and buffers shared by this kind's `PAINT` instances.
    /// Declare bindings in the look module, starting at group 2, binding 1; binding 0 holds instances.
    /// Update the material's `extension` through [`LaneMaterialHandle`].
    type Material: MaterialExtension2d<Data: Copy + Eq + Hash> + Default = NoLaneMaterial;

    /// World size of the quad every `PAINT` instance draws at scale 1. Each instance's transform
    /// scales, rotates and places it.
    fn quad_size() -> Vec2 {
        Vec2::ONE
    }

    /// The mesh every `PAINT` instance draws: by default a rectangle of [`Self::quad_size`].
    fn mesh() -> Mesh {
        Rectangle::from_size(Self::quad_size()).into()
    }

    /// World size of this instance's quad for its cut parts, before its transform; by default
    /// [`Self::quad_size`].
    fn cut_quad_size(&self) -> Vec2 {
        Self::quad_size()
    }

    /// How `PAINT` blends onto the scene.
    fn blend() -> BlendState {
        BlendState::ALPHA_BLENDING
    }

    /// How `OVERLAY` blends onto the scene.
    fn overlay_blend() -> BlendState {
        BlendState::ALPHA_BLENDING
    }
}

/// Stable shader handle derived from the lane name and kind label, without world access.
pub(crate) fn entry_shader_id<K: LaneVisual>(lane: &str) -> Handle<Shader> {
    let mut hasher = DefaultHasher::new();
    (lane, K::LABEL).hash(&mut hasher);
    Handle::Uuid(Uuid::from_u128(hasher.finish().into()), PhantomData)
}

/// Asset path of kind `K`'s look module.
pub(crate) fn look_asset_path<K: LaneVisual>() -> String {
    format!("{}.wesl", K::LOOK.replace("::", "/"))
}

/// Shared paint material combining the lane's instance buffer with the kind's material data.
pub type LaneMaterial<K> = ExtendedMaterial2d<PaintMaterial<K>, <K as LaneVisual>::Material>;

/// Default material extension with no additional bindings.
#[derive(Asset, AsBindGroup, TypePath, Clone, Default)]
pub struct NoLaneMaterial {}
impl MaterialExtension2d for NoLaneMaterial {}

/// Handle to the kind's shared paint material. Use `Assets<LaneMaterial<K>>` to edit its `extension`.
#[derive(Resource)]
pub struct LaneMaterialHandle<K: LaneVisual>(pub Handle<LaneMaterial<K>>);

/// Bands that need a scene snapshot for visible `PAINT` instances this frame.
#[derive(Resource, ExtractResource, Clone, Default)]
#[extract_app(RenderApp)]
pub struct BandsReadingBeneath(PerBand<bool>);
impl BandsReadingBeneath {
    pub fn mark(&mut self, band: Band) {
        self.0[band] = true;
    }

    pub fn reads(&self, band: Band) -> bool {
        self.0[band]
    }

    pub fn clear(&mut self) {
        self.0 = PerBand::default();
    }
}
