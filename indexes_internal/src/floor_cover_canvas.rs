//! Draws floor cover on a single quad covering the map, using the shared GPU buffers.
//! Each domain supplies shaders for its cover kinds. The canvas blends them in `FloorCoverKind`
//! order, bottom first, and hides floor cover beneath walls and buildings.

use bevy::{
    mesh::MeshVertexBufferLayoutRef,
    prelude::*,
    reflect::TypePath,
    render::render_resource::{AsBindGroup, BlendState, RenderPipelineDescriptor, SpecializedMeshPipelineError},
    shader::ShaderRef,
    sprite_render::{AlphaMode2d, Material2d, Material2dKey, Material2dPipeline, Material2dPlugin},
};

use game_core::prelude::{DrawLevel, MapInfo, ZDepth};
use states::prelude::MapLoadingStage;
use visuals::{canvas::MapCanvasBundle, shader::set_blend, shared_gpu::add_shared_gpu_layout};

pub(crate) struct FloorCoverCanvasPlugin;
impl Plugin for FloorCoverCanvasPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins(Material2dPlugin::<FloorCoverCanvasMaterial>::default())
            .add_systems(OnEnter(MapLoadingStage::LoadResources), FloorCoverCanvas::create);
    }
}

#[derive(Asset, AsBindGroup, TypePath, Debug, Clone, Default)]
struct FloorCoverCanvasMaterial {}
impl Material2d for FloorCoverCanvasMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/grids/floor_cover_canvas.wesl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }

    // `AlphaMode2d` has no premultiplied mode, so the blend is set on the pipeline.
    fn specialize(
        _pipeline: &Material2dPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: Material2dKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        add_shared_gpu_layout(descriptor);
        set_blend(descriptor, BlendState::PREMULTIPLIED_ALPHA_BLENDING);
        Ok(())
    }
}

/// The map's floor cover canvas. Hidden while the map has no floor cover.
#[derive(Component)]
#[require(ZDepth::FloorCover, DrawLevel::GroundFloor)]
pub(crate) struct FloorCoverCanvas;
impl FloorCoverCanvas {
    fn create(
        mut commands: Commands,
        map_info: Res<MapInfo>,
        mut materials: ResMut<Assets<FloorCoverCanvasMaterial>>,
    ) {
        commands.despawn_all::<With<FloorCoverCanvas>>();

        let material = materials.add(FloorCoverCanvasMaterial {});
        commands.spawn((
            MapCanvasBundle::new(material, &map_info),
            FloorCoverCanvas,
            Visibility::Hidden,
        ));
    }
}
