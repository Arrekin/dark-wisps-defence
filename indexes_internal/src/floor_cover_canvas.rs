//! Draws floor cover on one quad per floor, each covering the map, using the shared GPU buffers.
//! Each domain supplies shaders for its cover kinds. A canvas blends them in `FloorCoverKind`
//! order, bottom first. Draw order decides what stands over a cover.

use bevy::{
    mesh::MeshVertexBufferLayoutRef,
    prelude::*,
    reflect::TypePath,
    render::render_resource::{AsBindGroup, BlendState, RenderPipelineDescriptor, SpecializedMeshPipelineError},
    shader::ShaderRef,
    sprite_render::{AlphaMode2d, Material2d, Material2dKey, Material2dPipeline, Material2dPlugin},
};

use game_core::prelude::{AltitudeLevel, DrawLevel, MapInfo, ZDepth};
use grids::base::STACK_LEVELS;
use indexes::obstacles::level_gpu_index;
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

#[derive(Asset, AsBindGroup, TypePath, Debug, Clone)]
struct FloorCoverCanvasMaterial {
    /// Level code (`level_gpu_index`) of the covers this canvas draws.
    #[uniform(0)]
    level: u32,
}
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

/// The floor cover canvas of one floor. Hidden while that floor has no floor cover.
#[derive(Component)]
#[require(ZDepth::FloorCover)]
pub(crate) struct FloorCoverCanvas(pub AltitudeLevel);
impl FloorCoverCanvas {
    fn create(
        mut commands: Commands,
        map_info: Res<MapInfo>,
        mut materials: ResMut<Assets<FloorCoverCanvasMaterial>>,
    ) {
        commands.despawn_all::<With<FloorCoverCanvas>>();

        for level in STACK_LEVELS {
            let material = materials.add(FloorCoverCanvasMaterial { level: level_gpu_index(level) });
            commands.spawn((
                MapCanvasBundle::new(material, &map_info),
                FloorCoverCanvas(level),
                DrawLevel::from(level),
                Visibility::Hidden,
            ));
        }
    }
}
