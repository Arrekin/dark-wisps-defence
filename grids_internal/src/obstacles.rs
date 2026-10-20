use bevy::prelude::*;

use game_core::prelude::{BuildingType, GridCoords, GridImprint, MapInfo};
use grids::{obstacles::GridStructureType, prelude::*};
use logging::prelude::*;
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
            .add_systems(PostUpdate, upload_gpu_obstacle_cells.run_if(resource_changed::<ObstacleGrid>))
            .add_observer(on_insert_obstacle_grid_object_imprint_on_grid)
            .add_observer(on_remove_obstacle_grid_object_remove_from_grid);
    }
}

// Obstacle cell layout, mirrored in `assets/shaders/grids/obstacles.wesl`.
const STRUCTURE_EMPTY: u32 = 0;
const STRUCTURE_WALL: u32 = 1;
const STRUCTURE_BUILDING: u32 = 2;
const DARK_ORE_BIT: u32 = 1 << 2;
const QUANTUM_FIELD_BIT: u32 = 1 << 3;
const BUILDING_TYPE_SHIFT: u32 = 8;

#[log_tags(Tag::Build)]
fn on_insert_obstacle_grid_object_imprint_on_grid(
    trigger: On<Insert<ObstacleGridObject>>,
    mut obstacle_grid: ResMut<ObstacleGrid>,
    buildings: Query<&BuildingType>,
    objects: Query<(&GridImprint, &GridCoords, &ObstacleGridObject)>,
) {
    let entity = trigger.entity;
    let Ok((grid_imprint, grid_coords, obstacle_grid_object)) = objects.get(entity) else { return; };
    match obstacle_grid_object {
        ObstacleGridObject::Building => {
            #[warn_dev("Building {entity} has no BuildingType; not imprinted on the obstacle grid")]
            let Ok(building_type) = buildings.get(entity) else { return; };
            obstacle_grid.imprint_structure(*grid_coords, *grid_imprint, GridStructureType::Building(entity, *building_type));
        }
        ObstacleGridObject::Wall => {
            obstacle_grid.imprint_structure(*grid_coords, *grid_imprint, GridStructureType::Wall(entity));
        }
        ObstacleGridObject::QuantumField => {
            obstacle_grid.imprint_custom(*grid_coords, *grid_imprint, |field| field.quantum_field = Some(entity));
        }
        ObstacleGridObject::DarkOre => {
            obstacle_grid.imprint_custom(*grid_coords, *grid_imprint, |field| field.dark_ore = Some(entity));
        }
    }
}

fn on_remove_obstacle_grid_object_remove_from_grid(
    trigger: On<Remove<ObstacleGridObject>>,
    mut obstacle_grid: ResMut<ObstacleGrid>,
    objects: Query<(&GridImprint, &GridCoords, &ObstacleGridObject)>,
) {
    let entity = trigger.entity;
    let Ok((grid_imprint, grid_coords, obstacle_grid_object)) = objects.get(entity) else { return; };
    match obstacle_grid_object {
        ObstacleGridObject::Building | ObstacleGridObject::Wall => {
            obstacle_grid.deprint_structure(*grid_coords, *grid_imprint);
        }
        ObstacleGridObject::QuantumField => {
            obstacle_grid.imprint_custom(*grid_coords, *grid_imprint, |field| field.quantum_field = None);
        }
        ObstacleGridObject::DarkOre => {
            obstacle_grid.imprint_custom(*grid_coords, *grid_imprint, |field| field.dark_ore = None);
        }
    }
}

fn clear_reserved_coords(mut reserved_coords: ResMut<ReservedCoords>) {
    reserved_coords.for_structures.clear();
}

fn upload_gpu_obstacle_cells(
    mut shared_gpu: SharedGpuWriter,
    obstacle_grid: Res<ObstacleGrid>,
) {
    shared_gpu.write(SharedGpuBinding::ObstacleCells, obstacle_grid.grid.iter().map(pack_obstacle_cell));
}

fn pack_obstacle_cell(field: &Field) -> u32 {
    let structure = match field.structure {
        GridStructureType::Empty => STRUCTURE_EMPTY,
        GridStructureType::Wall(_) => STRUCTURE_WALL,
        GridStructureType::Building(_, building_type) => STRUCTURE_BUILDING | (building_type.gpu_index() << BUILDING_TYPE_SHIFT),
    };
    structure
        | if field.has_dark_ore() { DARK_ORE_BIT } else { 0 }
        | if field.is_within_quantum_field() { QUANTUM_FIELD_BIT } else { 0 }
}
