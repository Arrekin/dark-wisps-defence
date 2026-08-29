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

use game_core::prelude::{Bounds, GridCoords, MapInfo, ZDepth};
use map_objects::{prelude::Wall, wall_style::{WallCanvasDebug, WallStyle, WallStyleKey, WallStyles}};
use states::prelude::MapLoadingStage;
use visuals::prelude::{MapCanvasBundle, ShaderLibraryAppExt};

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

/// Field order and types mirror `WallCanvasSettings` in `assets/shaders/wall_canvas.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, ShaderType, Default)]
struct WallCanvasSettings {
    grid_width: u32,
    grid_height: u32,
    debug_mode: u32,
}
impl WallCanvasSettings {
    fn new(bounds: Bounds, debug_mode: u32) -> Self {
        let (grid_width, grid_height) = bounds.as_u32();
        Self { grid_width, grid_height, debug_mode }
    }
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

        // Bind initialized buffers on the material's spawn frame.
        let map_bounds = map_info.grid_bounds;
        let cell_count = map_bounds.area();
        let cells = buffers.add(ShaderBuffer::from(vec![0u32; cell_count].as_slice()));
        let style_values: Vec<WallStyle> = styles.entries.iter().map(|e| e.style).collect();
        let style_buffer = buffers.add(ShaderBuffer::from(style_values.as_slice()));

        let material = materials.add(WallCanvasMaterial {
            cells,
            styles: style_buffer,
            settings: WallCanvasSettings::new(map_bounds, debug.shader_index()),
        });
        commands.spawn((
            MapCanvasBundle::new(&mut meshes, material, &map_info),
            WallCanvas,
        ));
    }
}

/// Coalesces changes: any number of requests in one frame produce a single rebuild. The flag
/// stays raised until a rebuild runs, so a request that arrives while the rebuild is skipped
/// (e.g. the canvas entity is not yet spawned) is not lost.
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
    materials: Res<Assets<WallCanvasMaterial>>,
    mut rebuild_requested: ResMut<WallCanvasRebuildRequested>,
    map_info: Res<MapInfo>,
    walls: Query<(&GridCoords, &WallStyleKey), With<Wall>>,
    wall_canvas: Single<&MeshMaterial2d<WallCanvasMaterial>, With<WallCanvas>>,
    // Reused so the fill does not reallocate; `set_data` still copies it into a fresh byte buffer.
    mut cell_data: Local<Vec<u32>>,
) -> Result<()> {
    rebuild_requested.clear();

    let material = materials.get(wall_canvas.into_inner())
        .ok_or("WallCanvas material asset missing")?;

    let map_bounds = map_info.grid_bounds;
    cell_data.clear();
    cell_data.resize(map_bounds.area(), 0);
    for (coords, key) in walls.iter() {
        let Some(index) = map_bounds.index_checked(*coords) else { continue; };
        // +1 because the cell buffer reserves 0 for open ground; style indices are 0-based
        // in the table itself.
        cell_data[index] = key.0 + 1;
    }

    let mut buffer = buffers.get_mut(&material.cells)
        .ok_or("WallCanvas cells buffer asset missing")?;
    buffer.set_data(&*cell_data);
    Ok(())
}

fn apply_wall_canvas_styles(
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    materials: Res<Assets<WallCanvasMaterial>>,
    styles: Res<WallStyles>,
    wall_canvas: Single<&MeshMaterial2d<WallCanvasMaterial>, With<WallCanvas>>,
) -> Result<()> {
    let material = materials.get(wall_canvas.into_inner())
        .ok_or("WallCanvas material asset missing")?;
    let style_values: Vec<WallStyle> = styles.entries.iter().map(|e| e.style).collect();
    let mut buffer = buffers.get_mut(&material.styles)
        .ok_or("WallCanvas styles buffer asset missing")?;
    buffer.set_data(&style_values);
    Ok(())
}

fn apply_wall_canvas_debug(
    mut materials: ResMut<Assets<WallCanvasMaterial>>,
    debug: Res<WallCanvasDebug>,
    wall_canvas: Single<&MeshMaterial2d<WallCanvasMaterial>, With<WallCanvas>>,
) -> Result<()> {
    let mut material = materials.get_mut(wall_canvas.into_inner())
        .ok_or("WallCanvas material asset missing")?;
    material.settings.debug_mode = debug.shader_index();
    Ok(())
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
