use bevy::prelude::*;

use game_core::prelude::{AltitudeLevel, MapInfo};
use grids::{base::GridCell, prelude::*};
use indexes::{obstacles::GridStructure, prelude::*};
use states::prelude::MapLoadingStage;
use visuals::shared_gpu::{SharedGpuBinding, SharedGpuWriter};

pub(crate) struct ObstaclesGridPlugin;
impl Plugin for ObstaclesGridPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<ObstacleGrid>()
            .init_resource::<ReservedCoords>()
            .add_systems(OnExit(MapLoadingStage::LoadMapInfo), create_obstacle_grid)
            .add_systems(First, clear_reserved_coords)
            .add_systems(PostUpdate, upload_gpu_obstacle_cells.run_if(resource_changed::<ObstacleGrid>));
    }
}

// Obstacle cell layout, mirrored in `assets/shaders/grids/obstacles.wesl`.
const STRUCTURE_BITS: u32 = 4;
const QUANTUM_FIELD_BIT: u32 = 1 << 12;
const BUILDING_TYPE_SHIFT: u32 = 16;
const WALL_STYLE_SHIFT: u32 = 24;
const SHAPE_BITS: u32 = 8;

fn create_obstacle_grid(
    mut commands: Commands,
    map_info: Res<MapInfo>,
) {
    commands.insert_resource(ObstacleGrid::new_with_size(map_info.grid_bounds));
}

fn clear_reserved_coords(mut reserved_coords: ResMut<ReservedCoords>) {
    reserved_coords.clear();
}

fn upload_gpu_obstacle_cells(
    mut shared_gpu: SharedGpuWriter,
    obstacle_grid: Res<ObstacleGrid>,
) {
    shared_gpu.write(SharedGpuBinding::ObstacleCells, obstacle_grid.grid.iter().map(pack_obstacle_cell));
}

/// Packs `cell` into `[structures, shapes]` matching `ObstacleCell` in
/// `assets/shaders/grids/obstacles.wesl`.
fn pack_obstacle_cell(cell: &ObstacleCell) -> [u32; 2] {
    let (building_type, wall_style) = match cell.on_ground {
        GridStructure::Building { building_type, .. } => (building_type.gpu_index(), 0),
        GridStructure::Wall { style, .. } => (0, style.gpu_index()),
        _ => (0, 0),
    };
    let (structures, shapes) = AltitudeLevel::THREE_LAYERS.into_iter().fold((0, 0), |(structures, shapes), level| {
        let structure = cell.at(level);
        let index = level.three_layer_index_gpu();
        (
            structures | structure.gpu_index() << (index * STRUCTURE_BITS),
            shapes | structure.shape().map_or(0, |shape| shape.gpu_code()) << (index * SHAPE_BITS),
        )
    });
    let structures = structures
        | if cell.is_within_quantum_field() { QUANTUM_FIELD_BIT } else { 0 }
        | building_type << BUILDING_TYPE_SHIFT
        | wall_style << WALL_STYLE_SHIFT;
    [structures, shapes]
}
