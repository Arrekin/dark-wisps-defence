use bevy::prelude::*;

use game_core::prelude::MapInfo;
use grids::prelude::*;
use indexes::{obstacles::GridStructure, prelude::*};
use states::prelude::MapLoadingStage;
use visuals::shared_gpu::{SharedGpuBinding, SharedGpuWriter};

pub(crate) struct ObstaclesGridPlugin;
impl Plugin for ObstaclesGridPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<ObstacleGrid>()
            .init_resource::<ReservedCoords>()
            .add_systems(OnExit(MapLoadingStage::LoadMapInfo), |mut commands: Commands, map_info: Res<MapInfo>| { commands.insert_resource(ObstacleGrid::new_with_size(map_info.grid_bounds)); })
            .add_systems(First, clear_reserved_coords)
            .add_systems(PostUpdate, upload_gpu_obstacle_cells.run_if(resource_changed::<ObstacleGrid>));
    }
}

// Obstacle cell layout, mirrored in `assets/shaders/grids/obstacles.wesl`.
const UNDERGROUND_SHIFT: u32 = 0;
const ELEVATION_SHIFT: u32 = 4;
const ON_ELEVATION_SHIFT: u32 = 8;
const QUANTUM_FIELD_BIT: u32 = 1 << 12;
const BUILDING_TYPE_SHIFT: u32 = 16;
const UNDERGROUND_SHAPE_SHIFT: u32 = 24;

fn clear_reserved_coords(mut reserved_coords: ResMut<ReservedCoords>) {
    reserved_coords.clear();
}

fn upload_gpu_obstacle_cells(
    mut shared_gpu: SharedGpuWriter,
    obstacle_grid: Res<ObstacleGrid>,
) {
    shared_gpu.write(SharedGpuBinding::ObstacleCells, obstacle_grid.grid.iter().map(pack_obstacle_cell));
}

fn pack_obstacle_cell(cell: &ObstacleCell) -> u32 {
    let building_type = match cell.elevation {
        GridStructure::Building { building_type, .. } => building_type.gpu_index(),
        _ => 0,
    };
    cell.underground.gpu_index() << UNDERGROUND_SHIFT
        | cell.elevation.gpu_index() << ELEVATION_SHIFT
        | cell.on_elevation.gpu_index() << ON_ELEVATION_SHIFT
        | if cell.is_within_quantum_field() { QUANTUM_FIELD_BIT } else { 0 }
        | building_type << BUILDING_TYPE_SHIFT
        | cell.underground.shape().map_or(0, |shape| shape.gpu_code()) << UNDERGROUND_SHAPE_SHIFT
}
