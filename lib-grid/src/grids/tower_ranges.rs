use crate::lib_prelude::*;
use crate::grids::base::BaseGrid;
use crate::search::flooding::{flood_tower_range, FloodTowerRangeMode};

pub struct TowerRangesPlugin;
impl Plugin for TowerRangesPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(OnExit(MapLoadingStage::LoadMapInfo), |mut commands: Commands, map_info: Res<MapInfo>| { commands.insert_resource(TowerRangesGrid::new_with_size(map_info.grid_width, map_info.grid_height)); })
            .add_observer(TowerRangesGrid::on_tower_added)
            .add_observer(TowerRangesGrid::on_tower_removed)
            ;
    }
}


pub type TowerRangesGrid = BaseGrid<HashSet<Entity>, GridVersion>;
impl TowerRangesGrid {
    pub fn add_tower(&mut self, coords: GridCoords, tower: Entity) {
        self[coords].insert(tower);
        self.version = self.version.wrapping_add(1);
    }
    pub fn remove_tower(&mut self, coords: GridCoords, tower: Entity) {
        self[coords].remove(&tower);
        self.version = self.version.wrapping_add(1);
    }
    fn on_tower_added(
        trigger: On<Insert, AttackRange>,
        mut tower_ranges_grid: ResMut<TowerRangesGrid>,
        towers: Query<(&GridCoords, &GridImprint, &AttackRange), With<Tower>>,
    ) {
        let entity = trigger.entity;
        let Ok((grid_coords, grid_imprint, attack_range)) = towers.get(entity) else { return; };
        flood_tower_range(&mut tower_ranges_grid, grid_imprint.iter(*grid_coords), FloodTowerRangeMode::Add, attack_range.get() as usize, entity);
    }
    fn on_tower_removed(
        trigger: On<Discard, AttackRange>,
        mut tower_ranges_grid: ResMut<TowerRangesGrid>,
        towers: Query<(&GridCoords, &GridImprint, &AttackRange), With<Tower>>,
    ) {
        let entity = trigger.entity;
        let Ok((grid_coords, grid_imprint, attack_range)) = towers.get(entity) else { return; };
        flood_tower_range(&mut tower_ranges_grid, grid_imprint.iter(*grid_coords), FloodTowerRangeMode::Remove, attack_range.get() as usize, entity);
    }
}

