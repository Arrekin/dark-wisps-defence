use lib_grid::grids::energy_supply::EnergySupplyGrid;
use lib_grid::grids::obstacles::ObstacleGrid;

use crate::prelude::*;
use crate::map_objects::dark_ore::DARK_ORE_GRID_IMPRINT;
use crate::map_objects::quantum_field::QuantumFieldImprintSelector;
use crate::map_objects::walls::WALL_GRID_IMPRINT;
use crate::wisps::components::WispType;
use crate::wisps::spawning::WISP_GRID_IMPRINT;

pub struct GridObjectPlacerPlugin;
impl Plugin for GridObjectPlacerPlugin {
    fn build(&self, app: &mut App) {
        app
            .insert_resource(GridObjectPlacerRequest::default())
            .add_systems(Startup, (
                |mut commands: Commands| { commands.spawn(GridObjectPlacer::default()); },
            ))
            .add_systems(PreUpdate, (
                GridObjectPlacer::follow_mouse_system.run_if(in_state(UiInteraction::PlaceGridObject)),
                keyboard_input_system,
            ))
            .add_systems(Update, (
                on_request_grid_object_placer_system.run_if(GridObjectPlacerRequest::there_is_request()),
            ))
            .add_systems(OnEnter(UiInteraction::PlaceGridObject), on_placing_enter_system)
            .add_systems(OnExit(UiInteraction::PlaceGridObject), on_placing_exit_system)
            .add_observer(GridObjectPlacer::on_coords_changed);
    }
}


#[derive(Resource, Default)]
pub struct GridObjectPlacerRequest(Option<GridObjectPlacer>);
impl GridObjectPlacerRequest {
    pub fn is_set(&self) -> bool { self.0.is_some() }
    pub fn set(&mut self, request: GridObjectPlacer) { self.0 = Some(request); }
    pub fn take(&mut self) -> Option<GridObjectPlacer> { self.0.take() }

    pub fn there_is_request() -> fn(Res<GridObjectPlacerRequest>) -> bool {
        |placer_request: Res<GridObjectPlacerRequest>| placer_request.is_set()
    }
}

#[derive(Component, Default, Clone, Debug, PartialEq)]
#[require(GridImprint, GridCoords, Sprite, ZDepth = ZDepth(10.), AutoGridTransformSync)]
pub enum GridObjectPlacer {
    #[default]
    None,
    Building(BuildingType),
    Wall,
    DarkOre,
    QuantumField(QuantumFieldImprintSelector),
    Wisp(WispType),
}
impl GridObjectPlacer {
    pub fn as_grid_imprint(&self, almanach: &Almanach) -> GridImprint {
        match self {
            GridObjectPlacer::Building(building_type) => almanach.get_building_info(*building_type).grid_imprint,
            GridObjectPlacer::Wall => WALL_GRID_IMPRINT,
            GridObjectPlacer::DarkOre => DARK_ORE_GRID_IMPRINT,
            GridObjectPlacer::QuantumField(imprint_selector) => imprint_selector.get(),
            GridObjectPlacer::Wisp(_) => WISP_GRID_IMPRINT,
            GridObjectPlacer::None => unreachable!(),
        }
    }

    fn follow_mouse_system(
        mut commands: Commands,
        mouse_info: Res<MouseInfo>,
        placer: Single<(Entity, &GridCoords), With<GridObjectPlacer>>,
    ) {
        let (placer_entity, placer_coords) = placer.into_inner();
        if *placer_coords != mouse_info.grid_coords {
            commands.entity(placer_entity).insert(mouse_info.grid_coords);
        }
    }

    fn on_coords_changed(
        _trigger: On<Insert, GridCoords>,
        obstacle_grid: Res<ObstacleGrid>,
        energy_supply_grid: Res<EnergySupplyGrid>,
        mouse_info: Res<MouseInfo>,
        placer: Single<(&mut Sprite, &GridObjectPlacer, &GridImprint, &GridCoords)>,
    ) {
        let (mut sprite, grid_object_placer, grid_imprint, grid_coords) = placer.into_inner();
        let is_imprint_in_bounds = mouse_info.grid_coords.is_imprint_in_bounds(grid_imprint, obstacle_grid.bounds());
        let is_imprint_placable = match &*grid_object_placer {
            GridObjectPlacer::None => false,
            GridObjectPlacer::Building(building_type) => obstacle_grid.query_building_placement(*grid_coords, *building_type, *grid_imprint),
            _ => obstacle_grid.query_imprint_all(*grid_coords, *grid_imprint, |field| field.is_empty()),
        };
    
        let (needs_energy_supply, is_imprint_powered) = match &*grid_object_placer {
            GridObjectPlacer::Building(building_type) => match building_type {
                BuildingType::MainBase | BuildingType::EnergyRelay => (false, false),
                _ => (true, energy_supply_grid.is_imprint_powered(*grid_coords, *grid_imprint)),
            },
            _ => (false, false)
        };
    
        sprite.color = if is_imprint_placable && is_imprint_in_bounds {
            if needs_energy_supply && !is_imprint_powered {
                Color::srgba(1.0, 1.0, 0.0, 0.2)
            } else {
                Color::srgba(0.0, 1.0, 0.0, 0.2)
            }
        } else {
            Color::srgba(1.0, 0.0, 0.0, 0.2)
        };
    }
}
impl From<BuildingType> for GridObjectPlacer {
    fn from(building_type: BuildingType) -> Self {
        GridObjectPlacer::Building(building_type)
    }
}
impl From<WispType> for GridObjectPlacer {
    fn from(wisp_type: WispType) -> Self {
        GridObjectPlacer::Wisp(wisp_type)
    }
}


fn on_placing_enter_system(
    placer: Single<&mut Visibility, With<GridObjectPlacer>>,
) {
    *placer.into_inner() = Visibility::Inherited;
}

fn on_placing_exit_system(
    placer: Single<(&mut Visibility, &mut GridObjectPlacer)>,
) {
    let (mut visibility, mut placer) = placer.into_inner();
    *visibility = Visibility::Hidden;
    *placer = GridObjectPlacer::None;
}

fn keyboard_input_system(
    mut grid_object_placer_request: ResMut<GridObjectPlacerRequest>,
    keys: Res<ButtonInput<KeyCode>>,
) {
    let placer_request = {
        if keys.just_pressed(KeyCode::KeyW) {
            GridObjectPlacer::Wall
        } else if keys.just_pressed(KeyCode::KeyO) {
            GridObjectPlacer::DarkOre
        } else if keys.just_pressed(KeyCode::KeyQ) {
            GridObjectPlacer::QuantumField(QuantumFieldImprintSelector::default())
        } else if keys.just_pressed(KeyCode::KeyM) {
            GridObjectPlacer::Building(BuildingType::MiningComplex.into())
        } else if keys.just_pressed(KeyCode::KeyE) {
            GridObjectPlacer::Building(BuildingType::EnergyRelay.into())
        } else if keys.just_pressed(KeyCode::KeyX) {
            GridObjectPlacer::Building((BuildingType::ExplorationCenter).into())
        } else if keys.just_pressed(KeyCode::Digit1) {
            GridObjectPlacer::Building(BuildingType::Tower(TowerType::Blaster).into())
        } else if keys.just_pressed(KeyCode::Digit2) {
            GridObjectPlacer::Building(BuildingType::Tower(TowerType::Cannon).into())
        } else if keys.just_pressed(KeyCode::Digit3) {
            GridObjectPlacer::Building(BuildingType::Tower(TowerType::RocketLauncher).into())
        } else {
            return
        }
    };
    grid_object_placer_request.set(placer_request);
}

fn on_request_grid_object_placer_system(
    almanach: Res<Almanach>,
    mut ui_interaction_state: ResMut<NextState<UiInteraction>>,
    placer: Single<(&mut Sprite, &mut GridObjectPlacer, &mut GridImprint)>,
    mut placer_request: ResMut<GridObjectPlacerRequest>,
) {
    let Some(placer_request) = placer_request.take() else { return; };
    let (mut sprite, mut grid_object_placer, mut grid_imprint) = placer.into_inner();
    *grid_object_placer = placer_request;
    match &*grid_object_placer {
        GridObjectPlacer::None => panic!("GridObjectPlacer::None should not be possible here"),
        placer => {
            *grid_imprint = placer.as_grid_imprint(&almanach);
            sprite.custom_size = Some(grid_imprint.world_size());
        }
    }
    ui_interaction_state.set(UiInteraction::PlaceGridObject);
}
