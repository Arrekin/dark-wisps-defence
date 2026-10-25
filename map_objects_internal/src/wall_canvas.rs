//! Draws all walls on one quad covering the map.
//! Cell style indices and the style table are shared GPU data, also available to preview shaders.

use bevy::{
    prelude::*,
    reflect::TypePath,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
    sprite_render::{AlphaMode2d, Material2d, Material2dPlugin, MeshMaterial2d},
};

use game_core::{prelude::{GridArea, MapInfo, ZDepth}, request_flag::RequestFlag};
use map_objects::{prelude::Wall, wall_style::{WallCanvasDebug, WallStyleKey, WallStyles}};
use states::prelude::MapLoadingStage;
use visuals::{canvas::MapCanvasBundle, shared_gpu::{SharedGpuBinding, SharedGpuWriter}, specialize_with_shared_gpu};

pub(crate) struct WallCanvasPlugin;
impl Plugin for WallCanvasPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins(Material2dPlugin::<WallCanvasMaterial>::default())
            .init_resource::<RequestFlag<WallStyleCellsGpuUpload>>()
            .init_resource::<WallCanvasDebug>()
            .add_systems(OnEnter(MapLoadingStage::Init), |mut commands: Commands| { commands.insert_resource(WallStyles::presets()); })
            .add_systems(OnEnter(MapLoadingStage::LoadResources), WallCanvas::create)
            .add_systems(PostUpdate, (
                upload_gpu_wall_style_cells.run_if(RequestFlag::<WallStyleCellsGpuUpload>::is_requested),
                upload_gpu_wall_styles.run_if(resource_changed::<WallStyles>),
                apply_wall_canvas_debug.run_if(resource_changed::<WallCanvasDebug>),
            ))
            .add_observer(on_insert_wall_style_key_request_wall_style_cells_gpu_upload)
            .add_observer(on_remove_wall_style_key_request_wall_style_cells_gpu_upload);
    }
}

/// Field order and types mirror `WallCanvasSettings` in `assets/shaders/walls/canvas.wesl`.
#[derive(Clone, Copy, Debug, PartialEq, ShaderType, Default)]
struct WallCanvasSettings {
    debug_mode: u32,
}

#[derive(Asset, AsBindGroup, TypePath, Debug, Clone, Default)]
struct WallCanvasMaterial {
    #[uniform(0)]
    settings: WallCanvasSettings,
}
impl Material2d for WallCanvasMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/walls/canvas.wesl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }

    specialize_with_shared_gpu!();
}

#[derive(Component)]
#[require(ZDepth::Obstacle)]
struct WallCanvas;
impl WallCanvas {
    fn create(
        mut commands: Commands,
        map_info: Res<MapInfo>,
        debug: Res<WallCanvasDebug>,
        mut materials: ResMut<Assets<WallCanvasMaterial>>,
        mut upload_requested: ResMut<RequestFlag<WallStyleCellsGpuUpload>>,
    ) {
        commands.despawn_all::<With<WallCanvas>>();
        upload_requested.request();

        let material = materials.add(WallCanvasMaterial {
            settings: WallCanvasSettings { debug_mode: debug.shader_index() },
        });
        commands.spawn((
            MapCanvasBundle::new(material, &map_info),
            WallCanvas,
        ));
    }
}

/// Upload of wall style indices.
struct WallStyleCellsGpuUpload;

fn on_insert_wall_style_key_request_wall_style_cells_gpu_upload(
    _trigger: On<Insert<WallStyleKey>>,
    mut upload_requested: ResMut<RequestFlag<WallStyleCellsGpuUpload>>,
) {
    upload_requested.request();
}

fn on_remove_wall_style_key_request_wall_style_cells_gpu_upload(
    _trigger: On<Remove<WallStyleKey>>,
    mut upload_requested: ResMut<RequestFlag<WallStyleCellsGpuUpload>>,
) {
    upload_requested.request();
}

fn upload_gpu_wall_style_cells(
    mut shared_gpu: SharedGpuWriter,
    mut upload_requested: ResMut<RequestFlag<WallStyleCellsGpuUpload>>,
    map_info: Res<MapInfo>,
    walls: Query<(&GridArea, &WallStyleKey), With<Wall>>,
) {
    upload_requested.clear();

    let map_bounds = map_info.grid_bounds;
    let mut cells = vec![0u32; map_bounds.area()];
    for (area, key) in walls.iter() {
        let Some(index) = map_bounds.index_checked(area.origin) else { continue; };
        cells[index] = key.shader_index();
    }
    shared_gpu.write(SharedGpuBinding::WallStyleCells, cells);
}

fn upload_gpu_wall_styles(
    mut shared_gpu: SharedGpuWriter,
    styles: Res<WallStyles>,
) {
    shared_gpu.write(SharedGpuBinding::WallStyles, styles.entries.iter().map(|entry| entry.style));
}

fn apply_wall_canvas_debug(
    mut materials: ResMut<Assets<WallCanvasMaterial>>,
    debug: Res<WallCanvasDebug>,
    wall_canvas: Single<&MeshMaterial2d<WallCanvasMaterial>, With<WallCanvas>>,
) {
    materials.get_mut(wall_canvas.into_inner()).unwrap().settings.debug_mode = debug.shader_index();
}
