//! Renders walls and ramps in two passes, each using a quad that covers the map. Wisps render
//! between the passes, allowing wall tops to hide wisps inside tunnels.
//!
//! Shared GPU buffers supply the wall shaders with each cell's style and shape and the map's
//! wall style table.
//!
//! The tunnel reveal makes the tops of all hollow walls semi-transparent, showing the tunnel
//! interiors beneath.

use bevy::{
    prelude::*,
    reflect::TypePath,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
    sprite_render::{AlphaMode2d, Material2d, Material2dPlugin, MeshMaterial2d},
};
use strum::{EnumIter, IntoEnumIterator};

use game_core::{prelude::{DrawLevel, GridArea, MapInfo, ZDepth}, request_flag::RequestFlag};
use map_objects::{prelude::{Wall, VolumeShape}, wall_style::{WallCanvasDebug, WallStyleKey, WallStyles}};
use states::prelude::MapLoadingStage;
use visuals::{canvas::MapCanvasBundle, shared_gpu::{SharedGpuBinding, SharedGpuWriter}, specialize_with_shared_gpu};

use crate::tunnel_reveal::TunnelReveal;

pub(crate) struct WallCanvasPlugin;
impl Plugin for WallCanvasPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins(Material2dPlugin::<WallCanvasMaterial>::default())
            .init_resource::<RequestFlag<WallCellsGpuUpload>>()
            .init_resource::<WallCanvasDebug>()
            .add_systems(OnEnter(MapLoadingStage::Init), |mut commands: Commands| { commands.insert_resource(WallStyles::presets()); })
            .add_systems(OnEnter(MapLoadingStage::LoadResources), WallCanvas::create)
            .add_systems(Update, apply_tunnel_reveal.run_if(resource_changed::<TunnelReveal>))
            .add_systems(PostUpdate, (
                upload_gpu_wall_cells.run_if(RequestFlag::<WallCellsGpuUpload>::is_requested),
                upload_gpu_wall_styles.run_if(resource_changed::<WallStyles>),
                apply_wall_canvas_debug.run_if(resource_changed::<WallCanvasDebug>),
            ))
            .add_observer(on_insert_wall_request_wall_cells_gpu_upload)
            .add_observer(on_remove_wall_request_wall_cells_gpu_upload);
    }
}

/// Shift applied to [`VolumeShape::gpu_code`] in a packed cell; bits 0–7 store the style index.
/// Mirrors `SHAPE_SHIFT` in `assets/shaders/walls/styles.wesl`.
const SHAPE_SHIFT: u32 = 8;

/// Field order and types mirror `WallCanvasSettings` in `assets/shaders/walls/canvas.wesl`.
#[derive(Clone, Copy, Debug, PartialEq, ShaderType, Default)]
struct WallCanvasSettings {
    debug_mode: u32,
    /// Pass selected by [`WallCanvasPass::gpu_index`].
    canvas_pass: u32,
    /// From 0 (hollow wall tops opaque) to 1 (fully revealed).
    tunnel_reveal: f32,
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

/// Separates wall surfaces into layers below and above wisps.
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter)]
enum WallCanvasPass {
    /// Contact shadows, entrance recesses, tunnel interiors, and lower ramp halves.
    Bottom,
    /// Flat wall tops and upper ramp halves, starting at half the wall height.
    Top,
}
impl WallCanvasPass {
    /// Mirrors `PASS_*` in `assets/shaders/walls/canvas.wesl`.
    const fn gpu_index(self) -> u32 {
        match self {
            Self::Bottom => 0,
            Self::Top => 1,
        }
    }

    const fn z_depth(self) -> ZDepth {
        match self {
            Self::Bottom => ZDepth::WallBottom,
            Self::Top => ZDepth::WallTop,
        }
    }
}

/// One wall canvas quad.
#[derive(Component)]
#[require(DrawLevel::Elevation)]
struct WallCanvas;
impl WallCanvas {
    fn create(
        mut commands: Commands,
        map_info: Res<MapInfo>,
        debug: Res<WallCanvasDebug>,
        reveal: Res<TunnelReveal>,
        mut materials: ResMut<Assets<WallCanvasMaterial>>,
        mut upload_requested: ResMut<RequestFlag<WallCellsGpuUpload>>,
    ) {
        commands.despawn_all::<With<WallCanvas>>();
        upload_requested.request();

        for pass in WallCanvasPass::iter() {
            let material = materials.add(WallCanvasMaterial {
                settings: WallCanvasSettings { debug_mode: debug.gpu_index(), canvas_pass: pass.gpu_index(), tunnel_reveal: reveal.amount },
            });
            commands.spawn((
                MapCanvasBundle::new(material, &map_info),
                WallCanvas,
                pass.z_depth(),
            ));
        }
    }
}

/// Request marker for uploading packed wall cell data.
struct WallCellsGpuUpload;

fn on_insert_wall_request_wall_cells_gpu_upload(
    _trigger: On<Insert<Wall>>,
    mut upload_requested: ResMut<RequestFlag<WallCellsGpuUpload>>,
) {
    upload_requested.request();
}

fn on_remove_wall_request_wall_cells_gpu_upload(
    _trigger: On<Remove<Wall>>,
    mut upload_requested: ResMut<RequestFlag<WallCellsGpuUpload>>,
) {
    upload_requested.request();
}

fn upload_gpu_wall_cells(
    mut shared_gpu: SharedGpuWriter,
    mut upload_requested: ResMut<RequestFlag<WallCellsGpuUpload>>,
    map_info: Res<MapInfo>,
    walls: Query<(&VolumeShape, &GridArea, &WallStyleKey), With<Wall>>,
) {
    upload_requested.clear();

    let map_bounds = map_info.grid_bounds;
    let mut cells = vec![0u32; map_bounds.area()];
    for (shape, area, key) in walls.iter() {
        let Some(index) = map_bounds.index_checked(area.origin.coords) else { continue; };
        cells[index] = key.gpu_index() | shape.gpu_code() << SHAPE_SHIFT;
    }
    shared_gpu.write(SharedGpuBinding::WallCells, cells);
}

fn upload_gpu_wall_styles(
    mut shared_gpu: SharedGpuWriter,
    styles: Res<WallStyles>,
) {
    shared_gpu.write(SharedGpuBinding::WallStyles, styles.entries.iter().map(|entry| entry.style));
}

fn apply_tunnel_reveal(
    mut materials: ResMut<Assets<WallCanvasMaterial>>,
    reveal: Res<TunnelReveal>,
    wall_canvases: Query<&MeshMaterial2d<WallCanvasMaterial>, With<WallCanvas>>,
) {
    for material in wall_canvases.iter() {
        let Some(mut material) = materials.get_mut(material) else { continue; };
        material.settings.tunnel_reveal = reveal.amount;
    }
}

fn apply_wall_canvas_debug(
    mut materials: ResMut<Assets<WallCanvasMaterial>>,
    debug: Res<WallCanvasDebug>,
    wall_canvases: Query<&MeshMaterial2d<WallCanvasMaterial>, With<WallCanvas>>,
) {
    for material in wall_canvases.iter() {
        let Some(mut material) = materials.get_mut(material) else { continue; };
        material.settings.debug_mode = debug.gpu_index();
    }
}
