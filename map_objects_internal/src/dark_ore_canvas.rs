//! Dark ore is drawn by a single quad covering the grid, with a storage buffer holding one f32
//! per cell — the normalised fill level.

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

use almanach::Almanach;
use game_core::prelude::{Bounds, GridCoords, MapInfo, ZDepth};
use map_objects::prelude::DarkOre;
use states::prelude::MapLoadingStage;
use visuals::prelude::{MapCanvasBundle, ShaderLibraryAppExt};

pub(crate) struct DarkOreCanvasPlugin;
impl Plugin for DarkOreCanvasPlugin {
    fn build(&self, app: &mut App) {
        app
            .register_shader_library("shaders/dark_ore_crystals.wgsl")
            .add_plugins(Material2dPlugin::<DarkOreCanvasMaterial>::default())
            .init_resource::<DarkOreCanvasRebuildRequested>()
            .add_systems(OnEnter(MapLoadingStage::LoadResources), DarkOreCanvas::create)
            .add_systems(PostUpdate, (
                    request_rebuild_on_dark_ore_changed,
                    rebuild_dark_ore_canvas.run_if(DarkOreCanvasRebuildRequested::is_requested),
                ).chain(),
            )
            .add_observer(on_dark_ore_remove_request_rebuild)
            ;
    }
}

/// Field order and types mirror `DarkOreCanvasSettings` in `dark_ore_canvas.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, ShaderType, Default)]
struct DarkOreCanvasSettings {
    grid_width: u32,
    grid_height: u32,
}
impl DarkOreCanvasSettings {
    fn new(bounds: Bounds) -> Self {
        let (grid_width, grid_height) = bounds.as_u32();
        Self { grid_width, grid_height }
    }
}

#[derive(Asset, AsBindGroup, TypePath, Debug, Clone, Default)]
struct DarkOreCanvasMaterial {
    #[storage(0, read_only)]
    cells: Handle<ShaderBuffer>,
    #[uniform(1)]
    settings: DarkOreCanvasSettings,
}

impl Material2d for DarkOreCanvasMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/dark_ore_canvas.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

#[derive(Component)]
#[require(ZDepth::DARK_ORE)]
pub(crate) struct DarkOreCanvas;
impl DarkOreCanvas {
    fn create(
        mut commands: Commands,
        map_info: Res<MapInfo>,
        mut meshes: ResMut<Assets<Mesh>>,
        mut materials: ResMut<Assets<DarkOreCanvasMaterial>>,
        mut buffers: ResMut<Assets<ShaderBuffer>>,
        dark_ore_canvases: Query<Entity, With<DarkOreCanvas>>,
    ) {
        dark_ore_canvases.iter().for_each(|entity| commands.entity(entity).despawn());

        let map_bounds = map_info.grid_bounds;
        let cell_count = map_bounds.area();
        let cells = buffers.add(ShaderBuffer::from(vec![0f32; cell_count].as_slice()));

        let material = materials.add(DarkOreCanvasMaterial {
            cells,
            settings: DarkOreCanvasSettings::new(map_bounds),
        });
        commands.spawn((
            MapCanvasBundle::new(&mut meshes, material, &map_info),
            DarkOreCanvas,
        ));
    }
}

/// Coalesces changes: any number of requests in one frame produce a single rebuild. The flag
/// stays raised until a rebuild runs, so a request that arrives while the rebuild is skipped
/// (e.g. the canvas entity is not yet spawned) is not lost.
#[derive(Resource, Default)]
pub(crate) struct DarkOreCanvasRebuildRequested(bool);
impl DarkOreCanvasRebuildRequested {
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

fn request_rebuild_on_dark_ore_changed(
    mut rebuild_requested: ResMut<DarkOreCanvasRebuildRequested>,
    dark_ores: Query<&DarkOre, Changed<DarkOre>>,
) {
    if !dark_ores.is_empty() {
        rebuild_requested.request();
    }
}

fn on_dark_ore_remove_request_rebuild(
    _trigger: On<Remove, DarkOre>,
    mut rebuild_requested: ResMut<DarkOreCanvasRebuildRequested>,
) {
    rebuild_requested.request();
}

fn rebuild_dark_ore_canvas(
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    materials: Res<Assets<DarkOreCanvasMaterial>>,
    mut rebuild_requested: ResMut<DarkOreCanvasRebuildRequested>,
    map_info: Res<MapInfo>,
    almanach: Res<Almanach>,
    dark_ores: Query<(&GridCoords, &DarkOre)>,
    dark_ore_canvas: Single<&MeshMaterial2d<DarkOreCanvasMaterial>, With<DarkOreCanvas>>,
    // Reused so the fill does not reallocate; `set_data` still copies it into a fresh byte buffer.
    mut cell_data: Local<Vec<f32>>,
) -> Result<()> {
    rebuild_requested.clear();

    let material = materials.get(dark_ore_canvas.into_inner())
        .ok_or("DarkOreCanvas material asset missing")?;

    let max_field_saturation = almanach.dark_ore.max_field_saturation as f32;

    let map_bounds = map_info.grid_bounds;
    cell_data.clear();
    cell_data.resize(map_bounds.area(), 0.0);
    for (coords, dark_ore) in dark_ores.iter() {
        let Some(index) = map_bounds.index_checked(*coords) else { continue; };
        cell_data[index] = (dark_ore.amount as f32 / max_field_saturation).clamp(0.0, 1.0);
    }

    let mut buffer = buffers.get_mut(&material.cells)
        .ok_or("DarkOreCanvas cells buffer asset missing")?;
    buffer.set_data(&*cell_data);
    Ok(())
}
