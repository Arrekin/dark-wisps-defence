use lib_core::placement::{GridPlacerChanged, GridPlacerOverridePropertyRequest};
use lib_grid::grids::energy_supply::EnergySupplyGrid;
use lib_grid::grids::obstacles::{ObstacleGrid, ReservedCoords};
use lib_inventory::placement::{GridsCollection, ObjectPlacementInfo, PlacementMode};

use crate::prelude::*;

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
                on_click_place_system.run_if(in_state(UiInteraction::PlaceGridObject)),
            ))
            .add_systems(OnEnter(UiInteraction::PlaceGridObject), on_placing_enter_system)
            .add_systems(OnExit(UiInteraction::PlaceGridObject), on_placing_exit_system)
            .add_observer(GridObjectPlacer::revalidate_on_change)
            .add_observer(GridObjectPlacer::on_modify);
    }
}


#[derive(Resource, Default)]
pub struct GridObjectPlacerRequest(Option<MapObject>);
impl GridObjectPlacerRequest {
    pub fn is_set(&self) -> bool { self.0.is_some() }
    pub fn set(&mut self, request: MapObject) { self.0 = Some(request); }
    pub fn take(&mut self) -> Option<MapObject> { self.0.take() }

    pub fn there_is_request() -> fn(Res<GridObjectPlacerRequest>) -> bool {
        |placer_request: Res<GridObjectPlacerRequest>| placer_request.is_set()
    }
}

/// Active placement session data.
pub struct ActivePlacement {
    pub map_object: MapObject,
    pub placement_info: ObjectPlacementInfo,
}

#[derive(Component, Default)]
#[require(GridImprint, GridCoords, Sprite, ZDepth = ZDepth(10.), AutoGridTransformSync)]
pub struct GridObjectPlacer {
    pub active: Option<ActivePlacement>,
}

impl GridObjectPlacer {
    pub fn map_object(&self) -> Option<MapObject> {
        self.active.as_ref().map(|a| a.map_object)
    }

    fn follow_mouse_system(
        mut commands: Commands,
        mouse_info: Res<MouseInfo>,
        placer: Single<(Entity, &GridCoords), With<GridObjectPlacer>>,
    ) {
        let (placer_entity, placer_coords) = placer.into_inner();
        if *placer_coords != mouse_info.grid_coords {
            commands.entity(placer_entity).insert(mouse_info.grid_coords);
            commands.trigger(GridPlacerChanged);
        }
    }

    fn on_modify(
        trigger: On<GridPlacerOverridePropertyRequest>,
        mut commands: Commands,
        placer: Single<(&mut Sprite, &mut GridImprint), With<GridObjectPlacer>>,
    ) {
        let (mut sprite, mut grid_imprint) = placer.into_inner();
        
        match *trigger.event() {
            GridPlacerOverridePropertyRequest::OverrideImprint(imprint) => {
                *grid_imprint = imprint;
                sprite.custom_size = Some(grid_imprint.world_size());
            }
        }
        
        commands.trigger(GridPlacerChanged);
    }

    fn revalidate_on_change(
        _trigger: On<GridPlacerChanged>,
        obstacle_grid: Res<ObstacleGrid>,
        energy_supply_grid: Res<EnergySupplyGrid>,
        reserved_coords: Res<ReservedCoords>,
        placer: Single<(&mut Sprite, &GridObjectPlacer, &GridImprint, &GridCoords)>,
    ) {
        let (mut sprite, grid_object_placer, grid_imprint, grid_coords) = placer.into_inner();
        let Some(active) = &grid_object_placer.active else { return; };
        // run validation
        let map_data = GridsCollection {
            map_object: active.map_object,
            obstacle_grid: &*obstacle_grid,
            energy_supply_grid: &*energy_supply_grid,
            reserved_coords: &*reserved_coords,
        };
        let result = (active.placement_info.validate)(*grid_coords, *grid_imprint, &map_data);
        sprite.color = result.color;
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
    placer.active = None;
}

fn keyboard_input_system(
    mut grid_object_placer_request: ResMut<GridObjectPlacerRequest>,
    keys: Res<ButtonInput<KeyCode>>,
) {
    let map_object = if keys.just_pressed(KeyCode::KeyW) {
        MapObject::Wall
    } else if keys.just_pressed(KeyCode::KeyO) {
        MapObject::DarkOre
    } else if keys.just_pressed(KeyCode::KeyQ) {
        MapObject::QuantumField
    } else if keys.just_pressed(KeyCode::KeyM) {
        MapObject::Building(BuildingType::MiningComplex)
    } else if keys.just_pressed(KeyCode::KeyE) {
        MapObject::Building(BuildingType::EnergyRelay)
    } else if keys.just_pressed(KeyCode::KeyX) {
        MapObject::Building(BuildingType::ExplorationCenter)
    } else if keys.just_pressed(KeyCode::Digit1) {
        MapObject::Building(BuildingType::Tower(TowerType::Blaster))
    } else if keys.just_pressed(KeyCode::Digit2) {
        MapObject::Building(BuildingType::Tower(TowerType::Cannon))
    } else if keys.just_pressed(KeyCode::Digit3) {
        MapObject::Building(BuildingType::Tower(TowerType::RocketLauncher))
    } else {
        return
    };
    grid_object_placer_request.set(map_object);
}

fn on_request_grid_object_placer_system(
    mut commands: Commands,
    almanach: Res<Almanach>,
    current_state: Res<State<UiInteraction>>,
    mut ui_interaction_state: ResMut<NextState<UiInteraction>>,
    placer: Single<(&mut Sprite, &mut GridObjectPlacer, &mut GridImprint)>,
    mut placer_request: ResMut<GridObjectPlacerRequest>,
) {
    let Some(map_object) = placer_request.take() else { return; };
    let (mut sprite, mut grid_object_placer, mut grid_imprint) = placer.into_inner();
    
    let placement_info = almanach.get_placement_info_for(map_object);
    
    *grid_imprint = placement_info.imprint;
    sprite.custom_size = Some(grid_imprint.world_size());
    
    grid_object_placer.active = Some(ActivePlacement {map_object, placement_info});
    
    // Only change state if not already in PlaceGridObject (avoid re-entry clearing active and the blink effect when it gets hidden for a frame)
    if *current_state.get() != UiInteraction::PlaceGridObject {
        ui_interaction_state.set(UiInteraction::PlaceGridObject);
    }
    commands.trigger(GridPlacerChanged);
}

fn on_click_place_system(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    mouse_info: Res<MouseInfo>,
    placer: Single<&GridObjectPlacer>,
) {
    // Block clicks through UI
    if mouse_info.is_over_ui { return; }
    
    let Some(ref active) = placer.active else { return };
    
    // Check placement mode for place/remove
    let should_place = match active.placement_info.place_mode {
        PlacementMode::OnRelease => mouse.just_released(MouseButton::Left),
        PlacementMode::OnPress => mouse.pressed(MouseButton::Left),
    };
    let should_remove = match active.placement_info.remove_mode {
        PlacementMode::OnRelease => mouse.just_released(MouseButton::Right),
        PlacementMode::OnPress => mouse.pressed(MouseButton::Right),
    };
    
    if should_place {
        active.placement_info.place_emitter.clone_box().emit(&mut commands);
    } else if should_remove {
        if let Some(remove_emitter) = &active.placement_info.remove_emitter {
            remove_emitter.clone_box().emit(&mut commands);
        }
    }
}
