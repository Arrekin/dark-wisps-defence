use bevy::prelude::*;

use alteration::modifiers::prelude::AttackRange;
use buildings::prelude::Tower;
use game_core::prelude::{GridCoords, GridImprint, MapInfo, Property};
use grids::{
    emissions::FloodTowerRangeMode,
    search::flooding::flood_tower_range,
    tower_ranges::TowerRangesGrid,
};
use states::prelude::MapLoadingStage;

fn on_tower_added_update_ranges(
    trigger: On<Insert, AttackRange>,
    mut tower_ranges_grid: ResMut<TowerRangesGrid>,
    towers: Query<(&GridCoords, &GridImprint, &AttackRange), With<Tower>>,
) {
    let entity = trigger.entity;
    let Ok((grid_coords, grid_imprint, attack_range)) = towers.get(entity) else { return; };
    flood_tower_range(&mut tower_ranges_grid, grid_imprint.iter(*grid_coords), FloodTowerRangeMode::Add, attack_range.get() as usize, entity);
}
fn on_tower_removed_update_ranges(
    trigger: On<Discard, AttackRange>,
    mut tower_ranges_grid: ResMut<TowerRangesGrid>,
    towers: Query<(&GridCoords, &GridImprint, &AttackRange), With<Tower>>,
) {
    let entity = trigger.entity;
    let Ok((grid_coords, grid_imprint, attack_range)) = towers.get(entity) else { return; };
    flood_tower_range(&mut tower_ranges_grid, grid_imprint.iter(*grid_coords), FloodTowerRangeMode::Remove, attack_range.get() as usize, entity);
}

pub struct TowerRangesPlugin;
impl Plugin for TowerRangesPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(OnExit(MapLoadingStage::LoadMapInfo), |mut commands: Commands, map_info: Res<MapInfo>| { commands.insert_resource(TowerRangesGrid::new_with_size(map_info.grid_width, map_info.grid_height)); })
            .add_observer(on_tower_added_update_ranges)
            .add_observer(on_tower_removed_update_ranges)
            ;
    }
}
