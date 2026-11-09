use bevy::{
    prelude::*,
    reflect::TypePath,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
    sprite_render::{AlphaMode2d, Material2d, Material2dPlugin, MeshMaterial2d},
};

use almanach::prelude::Almanach;
use game_core::prelude::{Bounds, BuildingType, CELL_SIZE, DrawLevel, GridArea, GridCoords, MapObject, ObjectFaceRequest, TowerType, ZDepth};
use grids::{ObstacleChangeSystems, placement::*};
use indexes::placement::*;
use states::prelude::UiInteraction;
use viewport::MouseInfo;

pub(crate) struct GridObjectPlacerPlugin;
impl Plugin for GridObjectPlacerPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins(Material2dPlugin::<GridPlacerMaterial>::default())
            .init_resource::<GridObjectPlacerRequest>()
            .add_systems(Startup, spawn_placer)
            .add_systems(PreUpdate, (
                follow_mouse.run_if(in_state(UiInteraction::PlaceGridObject)),
                handle_keyboard_input,
            ))
            .add_systems(Update, (
                begin_placement.run_if(GridObjectPlacerRequest::is_requested),
                handle_placement_click.in_set(ObstacleChangeSystems).run_if(in_state(UiInteraction::PlaceGridObject)),
            ))
            .add_systems(OnEnter(UiInteraction::PlaceGridObject), show_placer)
            .add_systems(OnExit(UiInteraction::PlaceGridObject), hide_placer)
            .add_observer(on_grid_placer_changed_revalidate_placement)
            .add_observer(on_grid_placer_override_property_request_do_so)
            .add_observer(on_grid_placer_ghost_rebuild_request_do_so);
    }
}

// ============================================================================
// MATERIAL
// ============================================================================

/// Field order and types mirror `GridPlacerData` in `assets/shaders/grids/grid_placer.wesl`.
#[derive(ShaderType, Clone, Debug, Default)]
struct GridPlacerUniform {
    cell_data: UVec4,
    bounds: UVec2,
    /// One of the `VALIDITY_*` constants in `assets/shaders/grids/grid_placer.wesl`.
    validity: u32,
}
impl GridPlacerUniform {
    /// Updates the grid dimensions and clears all packed cell states.
    fn reset_to_bounds(&mut self, bounds: Bounds) {
        self.bounds = bounds.into();
        self.cell_data = UVec4::ZERO;
    }
    fn bounds_match(&self, bounds: Bounds) -> bool {
        self.bounds == UVec2::from(bounds)
    }
}

#[derive(Asset, AsBindGroup, TypePath, Debug, Clone, Default)]
pub(crate) struct GridPlacerMaterial {
    #[uniform(0)]
    uniform: GridPlacerUniform,
}
impl Material2d for GridPlacerMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/grids/grid_placer.wesl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

/// Packs the area's shape and its annotations two bits per cell, up to 64 cells. The values are
/// the `CELL_*` constants in `assets/shaders/grids/grid_placer.wesl`.
fn build_cell_data(area: GridArea, annotations: &[(GridCoords, CellHighlight)]) -> UVec4 {
    let mut words = [0u32; 4];
    for (cell_index, cell_coords) in area.bounding_box().iter().enumerate().take(64) {
        let state: u32 = if area.contains(cell_coords) {
            match annotations.iter().find(|(coords, _)| *coords == cell_coords) {
                Some((_, CellHighlight::Negative)) => 2,
                Some((_, CellHighlight::Positive)) => 3,
                None => 1,
            }
        } else {
            0
        };

        words[cell_index / 16] |= state << ((cell_index % 16) * 2);
    }
    UVec4::new(words[0], words[1], words[2], words[3])
}

/// Marks the placer child used for the object preview.
/// The object's domain supplies its appearance in response to `ObjectFaceRequest::ghost`.
#[derive(Component)]
pub(crate) struct GridPlacerGhost;

// ============================================================================
// SYSTEMS
// ============================================================================

fn follow_mouse(
    mut commands: Commands,
    mouse_info: Res<MouseInfo>,
    placer: Single<(Entity, &GridArea), With<GridObjectPlacer>>,
) {
    let (placer_entity, placer_area) = placer.into_inner();
    if placer_area.origin.coords != mouse_info.grid_coords {
        commands.entity(placer_entity).insert(GridArea::new(mouse_info.grid_coords.at(placer_area.origin.level), placer_area.imprint));
        commands.trigger(GridPlacerChanged);
    }
}

fn on_grid_placer_override_property_request_do_so(
    trigger: On<GridPlacerOverridePropertyRequest>,
    mut commands: Commands,
    placer: Single<(Entity, &GridArea), With<GridObjectPlacer>>,
) {
    let (placer_entity, placer_area) = placer.into_inner();
    match *trigger.event() {
        GridPlacerOverridePropertyRequest::Imprint(imprint) => {
            commands.entity(placer_entity).insert(GridArea::new(placer_area.origin, imprint));
        }
    }

    commands.trigger(GridPlacerGhostRebuildRequest);
    commands.trigger(GridPlacerChanged);
}

/// Replaces the placer's ghost and requests its world-space appearance.
///
/// Copies the current imprint to the new ghost. Domain renderers read object-specific parameters
/// from the session entity.
fn on_grid_placer_ghost_rebuild_request_do_so(
    _trigger: On<GridPlacerGhostRebuildRequest>,
    mut commands: Commands,
    placer: Single<(Entity, &GridObjectPlacer, &GridArea)>,
) {
    let (placer_entity, grid_object_placer, placer_area) = placer.into_inner();
    commands.despawn_all::<With<GridPlacerGhost>>();

    if let Some(map_object) = grid_object_placer.map_object() {
        commands.entity(placer_entity).with_children(|placer| {
            placer
                .spawn((GridPlacerGhost, ZDepth::GridPlacerGhost, DrawLevel::Airborne, placer_area.imprint))
                .trigger(|ghost| ObjectFaceRequest::ghost(ghost, map_object));
        });
    }
}

fn on_grid_placer_changed_revalidate_placement(
    _trigger: On<GridPlacerChanged>,
    mut commands: Commands,
    mut materials: ResMut<Assets<GridPlacerMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    grids: GridsCollectionParam,
    placer: Single<(Entity, &GridObjectPlacer, &GridArea, &MeshMaterial2d<GridPlacerMaterial>)>,
) {
    let (placer_entity, grid_object_placer, placer_area, material_handle) = placer.into_inner();
    let Some(active_placement) = &grid_object_placer.active_placement else { return; };
    let Some(mut material) = materials.get_mut(material_handle) else { return; };

    let imprint_bounds = Bounds::from(placer_area.imprint);
    if !material.uniform.bounds_match(imprint_bounds) {
        commands.entity(placer_entity).insert(Mesh2d(meshes.add(Rectangle::from_size(placer_area.imprint.world_size()))));
        material.uniform.reset_to_bounds(imprint_bounds);
    }

    let validity = (active_placement.placement_info.validate)(active_placement.map_object, *placer_area, &grids);
    let annotations = (active_placement.placement_info.annotate)(active_placement.map_object, *placer_area, validity, &grids);

    material.uniform.validity = validity.gpu_index();
    material.uniform.cell_data = build_cell_data(*placer_area, &annotations);
}

fn spawn_placer(
    mut commands: Commands,
    mut materials: ResMut<Assets<GridPlacerMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let mesh = meshes.add(Rectangle::new(CELL_SIZE, CELL_SIZE));
    let material = materials.add(GridPlacerMaterial::default());
    commands.spawn((
        GridObjectPlacer::default(),
        Mesh2d(mesh),
        MeshMaterial2d(material),
        Visibility::Hidden,
        children![GridPlacerSession],
    ));
}

fn show_placer(placer: Single<&mut Visibility, With<GridObjectPlacer>>) {
    *placer.into_inner() = Visibility::Inherited;
}

fn hide_placer(
    mut commands: Commands,
    placer: Single<(&mut Visibility, &mut GridObjectPlacer)>,
) {
    let (mut visibility, mut placer) = placer.into_inner();
    *visibility = Visibility::Hidden;
    placer.active_placement = None;
    commands.trigger(StopPlacing);
}

fn handle_keyboard_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut grid_object_placer_request: ResMut<GridObjectPlacerRequest>,
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
        return;
    };
    grid_object_placer_request.set(map_object);
}

fn begin_placement(
    mut commands: Commands,
    almanach: Res<Almanach>,
    mut placer_request: ResMut<GridObjectPlacerRequest>,
    mut ui_interaction_state: ResMut<NextState<UiInteraction>>,
    placer: Single<(Entity, &mut GridObjectPlacer, &GridArea)>,
    placer_session: Single<Entity, With<GridPlacerSession>>,
) {
    let Some(map_object) = placer_request.take() else { return; };
    let (placer_entity, mut grid_object_placer, placer_area) = placer.into_inner();

    if grid_object_placer.active_placement.is_some() {
        commands.trigger(StopPlacing);
    }
    // Clear session parameters while preserving the marker and the parent relationship.
    commands.entity(*placer_session).retain::<(GridPlacerSession, ChildOf)>();

    let placement_info = almanach.get_placement_info_for(map_object);
    commands.entity(placer_entity).insert(GridArea::new(placer_area.origin, placement_info.imprint));

    grid_object_placer.active_placement = Some(ActivePlacement { map_object, placement_info });

    commands.trigger(BeginPlacing(map_object));
    commands.trigger(GridPlacerGhostRebuildRequest);
    commands.trigger(GridPlacerChanged);

    ui_interaction_state.set_if_different(UiInteraction::PlaceGridObject);
}

fn handle_placement_click(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    mouse_info: Res<MouseInfo>,
    placer: Single<&GridObjectPlacer>,
) {
    if mouse_info.is_over_ui { return; }

    let Some(active_placement) = &placer.active_placement else { return; };
    let map_object = active_placement.map_object;

    let should_place = match active_placement.placement_info.placement.place {
        PlacementMode::OnRelease => mouse.just_released(MouseButton::Left),
        PlacementMode::OnPress => mouse.pressed(MouseButton::Left),
    };
    let should_remove = match active_placement.placement_info.placement.remove {
        PlacementMode::OnRelease => mouse.just_released(MouseButton::Right),
        PlacementMode::OnPress => mouse.pressed(MouseButton::Right),
    };

    if should_place {
        commands.trigger(PlaceRequest(map_object));
    } else if should_remove {
        commands.trigger(RemoveRequest(map_object));
    }
}
