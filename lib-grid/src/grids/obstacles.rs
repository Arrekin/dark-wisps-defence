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
        match imprint {
            GridImprint::Rectangle { width, height } => {
                for y in 0..height {
                    for x in 0..width {
                        let index = self.index(coords.shifted((x, y)));
                        self.grid[index].structure = structure.clone();
                    }
                }
            }
        }
        self.version = self.version.wrapping_add(1);
    }
    pub fn deprint_structure(&mut self, coords: GridCoords, imprint: GridImprint) {
        match imprint {
            GridImprint::Rectangle { width, height } => {
                for y in 0..height {
                    for x in 0..width {
                        let index = self.index(coords.shifted((x, y)));
                        self.grid[index].structure = GridStructureType::Empty;
                    }
                }
            }
        }
        self.version = self.version.wrapping_add(1);
    }
    // Naive reprint that deprints all in old coords and hard imprints in new coords
    pub fn reprint_structure(&mut self, old_coords: GridCoords, new_coords: GridCoords, imprint: GridImprint, new_structure: GridStructureType) {
        self.deprint_structure(old_coords, imprint);
        self.imprint_structure(new_coords, imprint, new_structure);
    }
    pub fn query_imprint_all(&self, coords: GridCoords, imprint: GridImprint, query: fn(&Field) -> bool) -> bool {
        match imprint {
            GridImprint::Rectangle { width, height } => {
                for y in 0..height {
                    for x in 0..width {
                        let inner_coords = coords.shifted((x, y));
                        if inner_coords.is_in_bounds(self.bounds()) && !query(&self[inner_coords]) {
                            return false;
                        }
                    }
                }
            }
        }
        true
    }
    pub fn query_imprint_any(&self, coords: GridCoords, imprint: GridImprint, query: fn(&Field) -> bool) -> bool {
        match imprint {
            GridImprint::Rectangle { width, height } => {
                for y in 0..height {
                    for x in 0..width {
                        let inner_coords = coords.shifted((x, y));
                        if inner_coords.is_in_bounds(self.bounds()) && query(&self[inner_coords]) {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }
    pub fn query_imprint_count(&self, coords: GridCoords, imprint: GridImprint, query: fn(&Field) -> bool) -> usize {
        let mut count = 0;
        match imprint {
            GridImprint::Rectangle { width, height } => {
                for y in 0..height {
                    for x in 0..width {
                        let inner_coords = coords.shifted((x, y));
                        if inner_coords.is_in_bounds(self.bounds()) && query(&self[inner_coords]) {
                            count += 1;
                        }
                    }
                }
            }
        }
        count
    }
    pub fn query_imprint_element<T>(&self, coords: GridCoords, imprint: GridImprint, query: fn(&Field) -> Option<T>) -> Vec<T> {
        let mut vec = Vec::new();
        match imprint {
            GridImprint::Rectangle { width, height } => {
                for y in 0..height {
                    for x in 0..width {
                        let inner_coords = coords.shifted((x, y));
                        if inner_coords.is_in_bounds(self.bounds()) {
                            if let Some(element) = query(&self[inner_coords]) {
                                vec.push(element);
                            }
                        }
                    }
                }
            }
        }
        vec
    }

    pub fn query_building_placement(&self, coords: GridCoords, building_type: BuildingType, imprint: GridImprint) -> bool {
        match building_type {
            BuildingType::MiningComplex => {
                //MiningComplex requires at least one DarkOre cell and no other obstacles
                let GridImprint::Rectangle{ width, height } = imprint;
                let mut has_dark_ore = false;
                for y in 0..height {
                    for x in 0..width {
                        let inner_coords = coords.shifted((x, y));
                        if !inner_coords.is_in_bounds(self.bounds()) { return false; }
                        
                        let field = &self[inner_coords];
                        if field.is_within_quantum_field() || field.has_structure() { return false }
                        if field.has_dark_ore() { has_dark_ore = true; }
                    }
                }
                return has_dark_ore;
            },
            _ => self.query_imprint_all(coords, imprint, |field| field.is_empty()),
        }
    }

    pub fn imprint_custom(&mut self, coords: GridCoords, imprint: GridImprint, imprint_fn: impl Fn(&mut Field)) {
        match imprint {
            GridImprint::Rectangle { width, height } => {
                for y in 0..height {
                    for x in 0..width {
                        let index = self.index(coords.shifted((x, y)));
                        imprint_fn(&mut self.grid[index]);
                    }
                }
            }
        }
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
        match imprint {
            GridImprint::Rectangle { width, height } => {
                for y in 0..height {
                    for x in 0..width {
                        let inner_coords = coords.shifted((x, y));
                        self.for_structures.insert(inner_coords);
                    }
                }
            }
        }
    }
    pub fn any_reserved(&self, coords: GridCoords, imprint: GridImprint) -> bool {
        match imprint {
            GridImprint::Rectangle { width, height } => {
                for y in 0..height {
                    for x in 0..width {
                        let inner_coords = coords.shifted((x, y));
                        if self.for_structures.contains(&inner_coords) { return true; }
                    }
                }
                false
            }
        }
    }
    fn clear_system(mut reserved_coords: ResMut<ReservedCoords>) {
        reserved_coords.for_structures.clear();
    }
}