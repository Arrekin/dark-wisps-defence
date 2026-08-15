//! Every wall on the map is drawn by a single quad covering the grid, with a storage buffer
//! holding one style index per cell.

use bevy::{
    prelude::*,
    reflect::TypePath,
    render::{
        render_resource::{AsBindGroup, ShaderType},
        storage::ShaderBuffer,
    },
    shader::ShaderRef,
    sprite_render::{AlphaMode2d, Material2d, Material2dPlugin, MeshMaterial2d},
};

use game_core::prelude::{GridCoords, MapInfo, ZDepth};
use map_objects::{prelude::Wall, wall_style::{WallCanvasDebug, WallStyle, WallStyleKey, WallStyles}};
use states::prelude::MapLoadingStage;
use visuals::prelude::ShaderLibraryAppExt;

pub(crate) struct WallCanvasPlugin;
impl Plugin for WallCanvasPlugin {
    fn build(&self, app: &mut App) {
        app
            .register_shader_library("shaders/wall_style.wgsl")
            .add_plugins(Material2dPlugin::<WallCanvasMaterial>::default())
            .init_resource::<WallCanvasRebuildRequested>()
            .init_resource::<WallCanvasDebug>()
            .add_systems(OnEnter(MapLoadingStage::Init), |mut commands: Commands| { commands.insert_resource(WallStyles::presets()); })
            .add_systems(OnEnter(MapLoadingStage::LoadResources), WallCanvas::create)
            .add_systems(PostUpdate, (
                    rebuild_wall_canvas.run_if(WallCanvasRebuildRequested::is_requested),
                    apply_wall_canvas_styles.run_if(resource_changed::<WallStyles>),
                    apply_wall_canvas_debug.run_if(resource_changed::<WallCanvasDebug>),
                ),
            )
            .add_observer(on_style_insert_request_wall_canvas_rebuild)
            .add_observer(on_style_remove_request_wall_canvas_rebuild)
            ;
    }
}

/// Field order and types mirror `WallCanvasSettings` in `assets/shaders/wall_canvas.wgsl`
/// exactly. The third `u32` is free because the uniform block is padded to 16 bytes
/// regardless.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, ShaderType, Default)]
struct WallCanvasSettings {
    grid_width: u32,
    grid_height: u32,
    debug_mode: u32,
}

#[derive(Asset, AsBindGroup, TypePath, Debug, Clone, Default)]
struct WallCanvasMaterial {
    #[storage(0, read_only)]
    cells: Handle<ShaderBuffer>,
    #[storage(1, read_only)]
    styles: Handle<ShaderBuffer>,
    #[uniform(2)]
    settings: WallCanvasSettings,
}
impl Material2d for WallCanvasMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/wall_canvas.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

#[derive(Component)]
#[require(ZDepth::OBSTACLE)]
pub(crate) struct WallCanvas;
impl WallCanvas {
    fn create(
        mut commands: Commands,
        map_info: Res<MapInfo>,
        styles: Res<WallStyles>,
        debug: Res<WallCanvasDebug>,
        mut meshes: ResMut<Assets<Mesh>>,
        mut materials: ResMut<Assets<WallCanvasMaterial>>,
        mut buffers: ResMut<Assets<ShaderBuffer>>,
        wall_canvases: Query<Entity, With<WallCanvas>>,
    ) {
        wall_canvases.iter().for_each(|entity| commands.entity(entity).despawn());

        // Both buffers are filled here: the material has to have something to bind on the
        // frame it is spawned.
        let cell_count = (map_info.grid_width * map_info.grid_height) as usize;
        let cells = buffers.add(ShaderBuffer::from(vec![0u32; cell_count].as_slice()));
        let style_values: Vec<WallStyle> = styles.entries.iter().map(|e| e.style).collect();
        let style_buffer = buffers.add(ShaderBuffer::from(style_values.as_slice()));

        commands.spawn((
            Mesh2d(meshes.add(Rectangle::new(1.0, 1.0))),
            MeshMaterial2d(materials.add(WallCanvasMaterial {
                cells,
                styles: style_buffer,
                settings: WallCanvasSettings {
                    grid_width: map_info.grid_width as u32,
                    grid_height: map_info.grid_height as u32,
                    debug_mode: debug.shader_index(),
                },
            })),
            Transform::from_xyz(map_info.world_width / 2., map_info.world_height / 2., 0.)
                .with_scale(Vec3::new(map_info.world_width, -map_info.world_height, 1.)),  // Flip vertically due to coordinate system
            WallCanvas,
        ));
    }
}

/// Raised whenever something that changes the picture happens, and lowered by the rebuild that
/// answers it. Several changes in one frame cost one rebuild, and the signal keeps waiting on a
/// frame where the rebuild cannot run.
#[derive(Resource, Default)]
pub(crate) struct WallCanvasRebuildRequested(bool);
impl WallCanvasRebuildRequested {
    pub(crate) fn request(&mut self) {
        self.0 = true;
    }

    pub(crate) fn clear(&mut self) {
        self.0 = false;
    }

    pub(crate) fn is_requested(requested: Res<Self>) -> bool {
        requested.0
    }
}

fn rebuild_wall_canvas(
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    mut materials: ResMut<Assets<WallCanvasMaterial>>,
    mut rebuild_requested: ResMut<WallCanvasRebuildRequested>,
    map_info: Res<MapInfo>,
    walls: Query<(&GridCoords, &WallStyleKey), With<Wall>>,
    wall_canvas: Single<&MeshMaterial2d<WallCanvasMaterial>, With<WallCanvas>>,
    // Reused so the rebuild is allocation-free after the first.
    mut cell_data: Local<Vec<u32>>,
) {
    rebuild_requested.clear();

    let Some(mut material) = materials.get_mut(wall_canvas.into_inner()) else { return; };

    cell_data.clear();
    cell_data.resize((map_info.grid_width * map_info.grid_height) as usize, 0);
    for (coords, key) in walls.iter() {
        if !coords.is_in_bounds((map_info.grid_width, map_info.grid_height)) { continue; }
        let index = (coords.y * map_info.grid_width + coords.x) as usize;
        // +1 because the cell buffer reserves 0 for open ground; style indices are 0-based
        // in the table itself.
        cell_data[index] = key.0 + 1;
    }

    let Some(mut buffer) = buffers.get_mut(&material.cells) else { return; };
    buffer.set_data(&*cell_data);

    material.settings.grid_width = map_info.grid_width as u32;
    material.settings.grid_height = map_info.grid_height as u32;
}

fn apply_wall_canvas_styles(
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    mut materials: ResMut<Assets<WallCanvasMaterial>>,
    styles: Res<WallStyles>,
    wall_canvas: Single<&MeshMaterial2d<WallCanvasMaterial>, With<WallCanvas>>,
) {
    let Some(material) = materials.get_mut(wall_canvas.into_inner()) else { return; };
    let style_values: Vec<WallStyle> = styles.entries.iter().map(|e| e.style).collect();
    let Some(mut buffer) = buffers.get_mut(&material.styles) else { return; };
    buffer.set_data(&style_values);
}

fn apply_wall_canvas_debug(
    mut materials: ResMut<Assets<WallCanvasMaterial>>,
    debug: Res<WallCanvasDebug>,
    wall_canvas: Single<&MeshMaterial2d<WallCanvasMaterial>, With<WallCanvas>>,
) {
    let Some(mut material) = materials.get_mut(wall_canvas.into_inner()) else { return; };
    material.settings.debug_mode = debug.shader_index();
}

fn on_style_insert_request_wall_canvas_rebuild(
    _trigger: On<Insert, WallStyleKey>,
    mut rebuild_requested: ResMut<WallCanvasRebuildRequested>,
) {
    rebuild_requested.request();
}
fn on_style_remove_request_wall_canvas_rebuild(
    _trigger: On<Remove, WallStyleKey>,
    mut rebuild_requested: ResMut<WallCanvasRebuildRequested>,
) {
    rebuild_requested.request();
}
