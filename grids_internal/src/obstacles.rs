use bevy::prelude::*;

use game_core::prelude::{BuildingType, GridCoords, GridImprint, MapInfo};
use grids::{obstacles::GridStructureType, prelude::*};
use states::prelude::MapLoadingStage;

fn on_insert_obstacle_grid_object_imprint_on_grid(
    trigger: On<Insert, ObstacleGridObject>,
    mut obstacle_grid: ResMut<ObstacleGrid>,
    buildings: Query<&BuildingType>,
    objects: Query<(&GridImprint, &GridCoords, &ObstacleGridObject)>,
) {
    let entity = trigger.entity;
    let Ok((grid_imprint, grid_coords, obstacle_grid_object)) = objects.get(entity) else { return; };
    match obstacle_grid_object {
        ObstacleGridObject::Building => {
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
    trigger: On<Remove, ObstacleGridObject>,
    mut obstacle_grid: ResMut<ObstacleGrid>,
    objects: Query<(&GridImprint, &GridCoords, &ObstacleGridObject)>,
) {
    let entity = trigger.entity;
    let Ok((grid_imprint, grid_coords, obstacle_grid_object)) = objects.get(entity) else { return; };
    match obstacle_grid_object {
        ObstacleGridObject::Building => {
            obstacle_grid.deprint_structure(*grid_coords, *grid_imprint);
        }
        ObstacleGridObject::Wall => {
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

pub struct ObstaclesGridPlugin;
impl Plugin for ObstaclesGridPlugin {
    fn build(&self, app: &mut App) {
        app
            .insert_resource(ObstacleGrid::new_empty())
            .init_resource::<ReservedCoords>()
            .add_systems(OnExit(MapLoadingStage::LoadMapInfo), |mut commands: Commands, map_info: Res<MapInfo>| { commands.insert_resource(ObstacleGrid::new_with_size(map_info.grid_width, map_info.grid_height)); })
            .add_systems(First, clear_reserved_coords)
            .add_observer(on_insert_obstacle_grid_object_imprint_on_grid)
            .add_observer(on_remove_obstacle_grid_object_remove_from_grid)
            ;
    }
}
