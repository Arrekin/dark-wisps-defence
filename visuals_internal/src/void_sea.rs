//! # Void Sea
//!
//! The sea around the map. Its surface stands above the map, held back by invisible glass along the
//! map's edge, so the map reads as a hollow in it. Waves run in from the sea, crash into the glass and
//! die there.
//!
//! The sea draws on a frame mesh whose vertex shader fits it to each camera's view. The outer ring
//! follows the view's corners and the inner ring encloses the part of the map the sea cannot cover,
//! so the sea shades only pixels it can show and reaches any distance the camera travels. Each camera
//! gets its own perspective from its position and view height. The look lives in
//! `assets/shaders/void_sea/`.
//!
//! The raised sea also casts a shadow onto the map along the edges facing the map sun. It draws on
//! static strips along those edges, as wide as the longest shadow can reach.

use bevy::{
    asset::RenderAssetUsages,
    camera::visibility::NoFrustumCulling,
    mesh::{Indices, MeshVertexBufferLayoutRef, PrimitiveTopology},
    prelude::*,
    reflect::TypePath,
    render::render_resource::{AsBindGroup, RenderPipelineDescriptor, SpecializedMeshPipelineError},
    shader::ShaderRef,
    sprite_render::{AlphaMode2d, Material2d, Material2dKey, Material2dPipeline, Material2dPlugin, MeshMaterial2d},
};

use game_core::prelude::{MapInfo, ZDepth};
use states::prelude::MapLoadingStage;
use visuals::{
    canvas::UNIT_QUAD,
    map_light::MAP_SUN_GROUND_DIRECTION,
    shader::{SHADOW_BLENDING, set_blend},
    shared_gpu::add_shared_gpu_layout,
    specialize_with_shared_gpu,
};

// Frame triangles: one quad between the outer ring (vertices 0..3) and the inner ring (4..7) per side.
const FRAME_INDICES: [u16; 24] = [
    0, 1, 5, 0, 5, 4, // bottom
    1, 2, 6, 1, 6, 5, // right
    2, 3, 7, 2, 7, 6, // top
    3, 0, 4, 3, 4, 7, // left
];

// Farthest the sea's shadow reaches onto the map along the sun's ground direction, in world units.
// Mirrors the longest shadow in `assets/shaders/void_sea/shadow.wesl`:
// WATER_HEIGHT * (1 + WATER_SURGE) / MAP_SUN_ELEVATION_TANGENT, rounded up.
const SHADOW_REACH: f32 = 52.0;

pub(crate) struct VoidSeaPlugin;
impl Plugin for VoidSeaPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                Material2dPlugin::<VoidSeaMaterial>::default(),
                Material2dPlugin::<VoidSeaShadowMaterial>::default(),
            ))
            .add_systems(OnEnter(MapLoadingStage::LoadResources), (spawn_void_sea, spawn_void_sea_shadow));
    }
}

#[derive(Asset, AsBindGroup, TypePath, Debug, Clone, Default)]
struct VoidSeaMaterial {}
impl Material2d for VoidSeaMaterial {
    fn vertex_shader() -> ShaderRef {
        "shaders/void_sea/void_sea.wesl".into()
    }

    fn fragment_shader() -> ShaderRef {
        "shaders/void_sea/void_sea.wesl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }

    specialize_with_shared_gpu!();
}

#[derive(Asset, AsBindGroup, TypePath, Debug, Clone, Default)]
struct VoidSeaShadowMaterial {}
impl Material2d for VoidSeaShadowMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/void_sea/shadow.wesl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }

    // `AlphaMode2d` has no multiplying mode, so the blend is set on the pipeline.
    fn specialize(
        _pipeline: &Material2dPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: Material2dKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        add_shared_gpu_layout(descriptor);
        set_blend(descriptor, SHADOW_BLENDING);
        Ok(())
    }
}

#[derive(Component)]
#[require(ZDepth::VoidSea)]
struct VoidSea;

#[derive(Component)]
#[require(ZDepth::VoidSeaShadow)]
struct VoidSeaShadow;

fn spawn_void_sea(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<VoidSeaMaterial>>,
) {
    commands.despawn_all::<With<VoidSea>>();

    // The vertex shader places every vertex. A position's x holds the vertex's number in the frame,
    // since the draw offsets the vertex index by the mesh's place in Bevy's shared vertex buffer.
    let vertex_numbers: Vec<[f32; 3]> = (0..8).map(|number| [number as f32, 0., 0.]).collect();
    let frame = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vertex_numbers)
        .with_inserted_indices(Indices::U16(FRAME_INDICES.to_vec()));
    commands.spawn((
        Mesh2d(meshes.add(frame)),
        MeshMaterial2d(materials.add(VoidSeaMaterial {})),
        // Its stored positions say nothing about where it draws.
        NoFrustumCulling,
        VoidSea,
    ));
}

fn spawn_void_sea_shadow(
    mut commands: Commands,
    map_info: Res<MapInfo>,
    mut materials: ResMut<Assets<VoidSeaShadowMaterial>>,
) {
    commands.despawn_all::<With<VoidSeaShadow>>();

    // One strip along each edge the sun faces, as deep as the shadow reaches across it. The strips must
    // not overlap: the blend would darken the corner twice.
    let size = map_info.world_size();
    let depth = (SHADOW_REACH * MAP_SUN_GROUND_DIRECTION.abs()).min(size);
    let edge_strip = if MAP_SUN_GROUND_DIRECTION.y > 0. { Rect::new(0., size.y - depth.y, size.x, size.y) } else { Rect::new(0., 0., size.x, depth.y) };
    let (side_low, side_high) = if MAP_SUN_GROUND_DIRECTION.y > 0. { (0., edge_strip.min.y) } else { (edge_strip.max.y, size.y) };
    let side_strip = if MAP_SUN_GROUND_DIRECTION.x < 0. { Rect::new(0., side_low, depth.x, side_high) } else { Rect::new(size.x - depth.x, side_low, size.x, side_high) };
    let material = materials.add(VoidSeaShadowMaterial {});
    for strip in [edge_strip, side_strip] {
        commands.spawn((
            Mesh2d(UNIT_QUAD),
            MeshMaterial2d(material.clone()),
            Transform::from_translation(strip.center().extend(0.)).with_scale(strip.size().extend(1.)),
            VoidSeaShadow,
        ));
    }
}
