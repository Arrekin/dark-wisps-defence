//! Draws dark ore on one quad covering the map.
//! Cell fill levels are uploaded as shared GPU data, so the canvas and other shaders use the same values.

use bevy::{
    prelude::*,
    reflect::TypePath,
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
    sprite_render::{AlphaMode2d, Material2d, Material2dPlugin},
};

use almanach::Almanach;
use game_core::{prelude::{GridCoords, MapInfo, ZDepth}, shared_gpu::{SharedGpuBinding, SharedGpuWriter}};
use map_objects::prelude::DarkOre;
use states::prelude::MapLoadingStage;
use visuals::{canvas::MapCanvasBundle, specialize_with_shared_gpu};

pub(crate) struct DarkOreCanvasPlugin;
impl Plugin for DarkOreCanvasPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins(Material2dPlugin::<DarkOreCanvasMaterial>::default())
            .init_resource::<DarkOreFillGpuUploadRequested>()
            .add_systems(OnEnter(MapLoadingStage::LoadResources), DarkOreCanvas::create)
            .add_systems(PostUpdate, (
                request_dark_ore_fill_gpu_upload_on_dark_ore_changed,
                upload_gpu_dark_ore_fill.run_if(resource_changed::<MapInfo>.or_else(DarkOreFillGpuUploadRequested::is_requested)),
            ).chain())
            .add_observer(on_remove_dark_ore_request_dark_ore_fill_gpu_upload);
    }
}

#[derive(Asset, AsBindGroup, TypePath, Debug, Clone, Default)]
struct DarkOreCanvasMaterial {}
impl Material2d for DarkOreCanvasMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/dark_ore/canvas.wesl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }

    specialize_with_shared_gpu!();
}

#[derive(Component)]
#[require(ZDepth::DarkOre)]
struct DarkOreCanvas;
impl DarkOreCanvas {
    fn create(
        mut commands: Commands,
        map_info: Res<MapInfo>,
        mut materials: ResMut<Assets<DarkOreCanvasMaterial>>,
    ) {
        commands.despawn_all::<With<DarkOreCanvas>>();

        let material = materials.add(DarkOreCanvasMaterial {});
        commands.spawn((
            MapCanvasBundle::new(material, &map_info),
            DarkOreCanvas,
        ));
    }
}

/// Marks dark ore fill data for upload, coalescing multiple requests within a frame.
#[derive(Resource, Default)]
struct DarkOreFillGpuUploadRequested(bool);
impl DarkOreFillGpuUploadRequested {
    fn request(&mut self) {
        self.0 = true;
    }

    fn clear(&mut self) {
        self.0 = false;
    }

    fn is_requested(requested: Res<Self>) -> bool {
        requested.0
    }
}

fn request_dark_ore_fill_gpu_upload_on_dark_ore_changed(
    mut upload_requested: ResMut<DarkOreFillGpuUploadRequested>,
    dark_ores: Query<&DarkOre, Changed<DarkOre>>,
) {
    if !dark_ores.is_empty() {
        upload_requested.request();
    }
}

fn on_remove_dark_ore_request_dark_ore_fill_gpu_upload(
    _trigger: On<Remove<DarkOre>>,
    mut upload_requested: ResMut<DarkOreFillGpuUploadRequested>,
) {
    upload_requested.request();
}

fn upload_gpu_dark_ore_fill(
    mut shared_gpu: SharedGpuWriter,
    mut upload_requested: ResMut<DarkOreFillGpuUploadRequested>,
    almanach: Res<Almanach>,
    map_info: Res<MapInfo>,
    dark_ores: Query<(&GridCoords, &DarkOre)>,
) {
    upload_requested.clear();

    let max_field_saturation = almanach.dark_ore.max_field_saturation as f32;
    let map_bounds = map_info.grid_bounds;
    let mut fill = vec![0f32; map_bounds.area()];
    for (coords, dark_ore) in dark_ores.iter() {
        let Some(index) = map_bounds.index_checked(*coords) else { continue; };
        fill[index] = (dark_ore.amount as f32 / max_field_saturation).clamp(0.0, 1.0);
    }
    shared_gpu.write(SharedGpuBinding::DarkOreFill, fill);
}
