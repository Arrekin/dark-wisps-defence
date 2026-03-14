use crate::lib_prelude::*;
use crate::grids::base::BaseGrid;

pub struct ObstaclesGridPlugin;
impl Plugin for ObstaclesGridPlugin {
    fn build(&self, app: &mut App) {
        app
            .insert_resource(ObstacleGrid::new_empty())
            .init_resource::<ReservedCoords>()
            .add_systems(OnExit(MapLoadingStage::LoadMapInfo), |mut commands: Commands, map_info: Res<MapInfo>| { commands.insert_resource(ObstacleGrid::new_with_size(map_info.grid_width, map_info.grid_height)); })
            .add_systems(First, ReservedCoords::clear_system)
            .add_observer(on_obstacle_grid_object_inserted)
            .add_observer(on_obstacle_grid_object_removed)
            ;
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Field {
    pub dark_ore: Option<Entity>,
    pub quantum_field: Option<Entity>,
    pub structure: GridStructureType,
}
impl Field {
    pub fn has_dark_ore(&self) -> bool {
        self.dark_ore.is_some()
    }
    pub fn is_within_quantum_field(&self) -> bool {
        self.quantum_field.is_some()
    }
    pub fn is_empty(&self) -> bool {
        matches!(self.structure, GridStructureType::Empty) && !self.is_within_quantum_field() && !self.has_dark_ore()
    }
    pub fn has_building(&self) -> bool {
        matches!(self.structure, GridStructureType::Building(..))
    }
    pub fn has_wall(&self) -> bool {
        matches!(self.structure, GridStructureType::Wall(_))
    }
    pub fn has_structure(&self) -> bool { 
        matches!(self.structure, GridStructureType::Wall(_) | GridStructureType::Building(..)) 
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub enum GridStructureType {
    #[default]
    Empty,
    Building(Entity, BuildingType),
    Wall(Entity),
}

pub type ObstacleGrid = BaseGrid<Field, GridVersion>;

impl ObstacleGrid {
    pub fn imprint_structure(&mut self, coords: GridCoords, imprint: GridImprint, structure: GridStructureType) {
        imprint.iter(coords).for_each(|cell| self[cell].structure = structure.clone());
        self.version = self.version.wrapping_add(1);
    }
    pub fn deprint_structure(&mut self, coords: GridCoords, imprint: GridImprint) {
        imprint.iter(coords).for_each(|cell| self[cell].structure = GridStructureType::Empty);
        self.version = self.version.wrapping_add(1);
    }
    // Naive reprint that deprints all in old coords and hard imprints in new coords
    pub fn reprint_structure(&mut self, old_coords: GridCoords, new_coords: GridCoords, imprint: GridImprint, new_structure: GridStructureType) {
        self.deprint_structure(old_coords, imprint);
        self.imprint_structure(new_coords, imprint, new_structure);
    }
    /// True if `query` returns true for every in-bounds cell of the imprint.
    pub fn query_imprint_all(&self, coords: GridCoords, imprint: GridImprint, query: fn(&Field) -> bool) -> bool {
        imprint.iter_in_bounds(coords, self.bounds()).all(|c| query(&self[c]))
    }
    /// True if `query` returns true for at least one in-bounds cell of the imprint.
    pub fn query_imprint_any(&self, coords: GridCoords, imprint: GridImprint, query: fn(&Field) -> bool) -> bool {
        imprint.iter_in_bounds(coords, self.bounds()).any(|c| query(&self[c]))
    }
    /// Number of in-bounds cells for which `query` returns true.
    pub fn query_imprint_count(&self, coords: GridCoords, imprint: GridImprint, query: fn(&Field) -> bool) -> usize {
        imprint.iter_in_bounds(coords, self.bounds()).filter(|c| query(&self[*c])).count()
    }
    /// Collects the non-None results of `query` over every in-bounds cell.
    pub fn query_imprint_element<T>(&self, coords: GridCoords, imprint: GridImprint, query: fn(&Field) -> Option<T>) -> Vec<T> {
        imprint.iter_in_bounds(coords, self.bounds()).filter_map(|c| query(&self[c])).collect()
    }

    pub fn imprint_custom(&mut self, coords: GridCoords, imprint: GridImprint, imprint_fn: impl Fn(&mut Field)) {
        imprint.iter(coords).for_each(|cell| imprint_fn(&mut self[cell]));
        self.version = self.version.wrapping_add(1);
    }
}

fn on_obstacle_grid_object_inserted(
    trigger: On<Insert, ObstacleGridObject>,
    mut obstacle_grid: ResMut<ObstacleGrid>,
    objects: Query<(&GridImprint, &GridCoords, &ObstacleGridObject)>,
    buildings: Query<&BuildingType>,
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
fn on_obstacle_grid_object_removed(
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

// When placing objects on map sometimes we want to reserve space so the state can be changed async later while ensuring that no other object can be placed there in parallel systems.
// These reservations are cleared in the First schedule of the following frame.
#[derive(Resource, Default)]
pub struct ReservedCoords {
    pub for_structures: HashSet<GridCoords>,
}
impl ReservedCoords {
    pub fn reserve(&mut self, coords: GridCoords, imprint: GridImprint) {
        self.for_structures.extend(imprint.iter(coords));
    }
    pub fn any_reserved(&self, coords: GridCoords, imprint: GridImprint) -> bool {
        imprint.iter(coords).any(|c| self.for_structures.contains(&c))
    }
    fn clear_system(mut reserved_coords: ResMut<ReservedCoords>) {
        reserved_coords.for_structures.clear();
    }
}