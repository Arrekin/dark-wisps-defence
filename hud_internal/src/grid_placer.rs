//! # Grid object placer
//!
//! Owns the placement session, keeps its target area under the cursor, and sends `PlaceRequest`
//! and `RemoveRequest` on mouse input.
//!
//! Each object's domain validates and handles requests. The tool renders the domain's
//! `PlacementFeedback` without using it to gate input.

use bevy::{
    platform::collections::HashSet,
    prelude::*,
    reflect::TypePath,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
    sprite_render::{AlphaMode2d, Material2d, Material2dPlugin, MeshMaterial2d},
};

use game_core::prelude::{Bounds, BuildingType, CELL_SIZE, DrawLevel, GridArea, GridCoords, GridPosition, MapObject, ObjectFaceRequest, TowerType, ZDepth};
use grids::{ObstacleChangeSystems, placement::*};
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
                begin_placement.run_if(GridObjectPlacerRequest::is_requested).before(PlacementFeedbackSystems),
                handle_placement_click.in_set(ObstacleChangeSystems).run_if(in_state(UiInteraction::PlaceGridObject)),
            ))
            .configure_sets(Update, PlacementFeedbackSystems.run_if(in_state(UiInteraction::PlaceGridObject)))
            .add_systems(OnEnter(UiInteraction::PlaceGridObject), show_placer)
            .add_systems(OnExit(UiInteraction::PlaceGridObject), hide_placer)
            .add_observer(on_stop_placing_despawn_controls)
            .add_observer(on_grid_placer_changed_fit_overlay)
            .add_observer(on_insert_placement_feedback_redraw_overlay)
            .add_observer(on_grid_placer_ghost_rebuild_request_do_so);
    }
}

// Validity and cell-state encodings shared with `assets/shaders/grids/grid_placer.wesl`.
const VALIDITY_VALID: u32 = 0;
const VALIDITY_VALID_UNPOWERED: u32 = 1;
const VALIDITY_INVALID: u32 = 2;
const VALIDITY_REPLACES: u32 = 3;
const CELL_OUTSIDE: u32 = 0;
const CELL_PLAIN: u32 = 1;
const CELL_NEGATIVE: u32 = 2;
const CELL_FRAMED: u32 = 3;

// Keys that start placing an object
const PLACEMENT_HOTKEYS: [(KeyCode, MapObject); 10] = [
    (KeyCode::KeyW, MapObject::Wall),
    (KeyCode::KeyU, MapObject::Excavation),
    (KeyCode::KeyO, MapObject::DarkOre),
    (KeyCode::KeyQ, MapObject::QuantumField),
    (KeyCode::KeyM, MapObject::Building(BuildingType::MiningComplex)),
    (KeyCode::KeyE, MapObject::Building(BuildingType::EnergyRelay)),
    (KeyCode::KeyX, MapObject::Building(BuildingType::ExplorationCenter)),
    (KeyCode::Digit1, MapObject::Building(BuildingType::Tower(TowerType::Blaster))),
    (KeyCode::Digit2, MapObject::Building(BuildingType::Tower(TowerType::Cannon))),
    (KeyCode::Digit3, MapObject::Building(BuildingType::Tower(TowerType::RocketLauncher))),
];

// ============================================================================
// MATERIAL
// ============================================================================

/// Field order and types mirror `GridPlacerData` in `assets/shaders/grids/grid_placer.wesl`.
#[derive(ShaderType, Clone, Debug, Default)]
struct GridPlacerUniform {
    cell_data: UVec4,
    bounds: UVec2,
    /// One of the `VALIDITY_*` constants.
    validity: u32,
    /// Affordability encoded as 1 (affordable) or 0 (unaffordable).
    is_affordable: u32,
}
impl GridPlacerUniform {
    /// Encodes feedback for `area`, or clears the overlay when feedback is absent.
    fn show(&mut self, area: GridArea, feedback: Option<&PlacementFeedback>) {
        let Some(feedback) = feedback else {
            self.cell_data = UVec4::ZERO;
            return;
        };
        self.validity = match feedback.validity {
            PlacementValidity::Valid => VALIDITY_VALID,
            PlacementValidity::ValidUnpowered => VALIDITY_VALID_UNPOWERED,
            PlacementValidity::Invalid => VALIDITY_INVALID,
            PlacementValidity::Replaces => VALIDITY_REPLACES,
        };
        self.is_affordable = feedback.is_affordable.into();
        self.cell_data = build_cell_data(area, &feedback.highlights);
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

/// Encodes the first 64 cells of the area's bounding box as two-bit `CELL_*` values.
/// Cells outside the imprint remain transparent; positive and removed highlights share a frame.
fn build_cell_data(area: GridArea, highlights: &[(GridCoords, CellHighlight)]) -> UVec4 {
    let mut words = [0u32; 4];
    for (cell_index, cell_coords) in area.bounding_box().iter().enumerate().take(64) {
        let state = if area.contains(cell_coords) {
            match highlights.iter().find(|(coords, _)| *coords == cell_coords) {
                Some((_, CellHighlight::Negative)) => CELL_NEGATIVE,
                Some((_, CellHighlight::Positive | CellHighlight::Removed)) => CELL_FRAMED,
                None => CELL_PLAIN,
            }
        } else {
            CELL_OUTSIDE
        };

        words[cell_index / 16] |= state << ((cell_index % 16) * 2);
    }
    UVec4::new(words[0], words[1], words[2], words[3])
}

// ============================================================================
// COMPONENTS
// ============================================================================

/// The placement tool. Its child session holds what is being placed and where.
#[derive(Component, Default)]
#[require(Transform, ZDepth::GridPlacer, DrawLevel::Airborne)]
pub(crate) struct GridObjectPlacer {
    /// Origin cells that got a place request since the left mouse button was pressed, including
    /// rejected placements.
    cells_requested_this_press: HashSet<GridCoords>,
}

/// Marks the placer child used for the object preview.
/// The object's domain supplies its appearance in response to `ObjectFaceRequest::ghost`.
#[derive(Component)]
#[require(ZDepth::GridPlacerGhost, DrawLevel::Airborne)]
pub(crate) struct GridPlacerGhost;

#[derive(Resource, Default)]
pub(crate) struct GridObjectPlacerRequest(Option<MapObject>);
impl GridObjectPlacerRequest {
    pub(crate) fn set(&mut self, request: MapObject) { self.0 = Some(request); }
    fn take(&mut self) -> Option<MapObject> { self.0.take() }

    fn is_requested(request: Res<Self>) -> bool {
        request.0.is_some()
    }
}

// ============================================================================
// SYSTEMS
// ============================================================================

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
        children![PlacementSession],
    ));
}

fn show_placer(placer: Single<&mut Visibility, With<GridObjectPlacer>>) {
    *placer.into_inner() = Visibility::Inherited;
}

fn hide_placer(
    mut commands: Commands,
    placer: Single<&mut Visibility, With<GridObjectPlacer>>,
) {
    *placer.into_inner() = Visibility::Hidden;
    commands.trigger(StopPlacing);
}

fn handle_keyboard_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut grid_object_placer_request: ResMut<GridObjectPlacerRequest>,
) {
    let Some(&(_, map_object)) = PLACEMENT_HOTKEYS.iter().find(|(key, _)| keys.just_pressed(*key)) else { return; };
    grid_object_placer_request.set(map_object);
}

fn begin_placement(
    mut commands: Commands,
    mouse_info: Res<MouseInfo>,
    mut placer_request: ResMut<GridObjectPlacerRequest>,
    ui_interaction: Res<State<UiInteraction>>,
    mut next_ui_interaction: ResMut<NextState<UiInteraction>>,
    placer: Single<&mut GridObjectPlacer>,
    placement_session: Single<Entity, With<PlacementSession>>,
) {
    let Some(map_object) = placer_request.take() else { return; };

    if *ui_interaction.get() == UiInteraction::PlaceGridObject {
        commands.trigger(StopPlacing);
    }

    let origin = GridPosition { coords: mouse_info.grid_coords, ..default() };
    // Reset the previous object's session data without detaching the reusable session entity.
    commands.entity(*placement_session)
        .retain::<(PlacementSession, ChildOf)>()
        .insert(PlacementObject(map_object));
    placer.into_inner().cells_requested_this_press.clear();

    commands.trigger(BeginPlacing { object: map_object, origin });
    commands.trigger(GridPlacerGhostRebuildRequest);
    commands.trigger(GridPlacerChanged);

    next_ui_interaction.set_if_different(UiInteraction::PlaceGridObject);
}

fn follow_mouse(
    mut commands: Commands,
    mouse_info: Res<MouseInfo>,
    placement_session: Single<(Entity, &GridArea), With<PlacementSession>>,
) {
    let (placement_session_entity, area) = placement_session.into_inner();
    if area.origin.coords != mouse_info.grid_coords {
        commands.entity(placement_session_entity).insert(GridArea::new(mouse_info.grid_coords.at(area.origin.level), area.imprint));
        commands.trigger(GridPlacerChanged);
    }
}

/// Sends requests according to the object's input modes, ignoring input over UI.
/// Drag placement sends at most one request per origin cell until the left button is released.
fn handle_placement_click(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    mouse_info: Res<MouseInfo>,
    placer: Single<&mut GridObjectPlacer>,
    placement_session: Single<(&PlacementObject, &GridArea, &PlacementModes), With<PlacementSession>>,
) {
    let mut placer = placer.into_inner();
    if !mouse.pressed(MouseButton::Left) && !placer.cells_requested_this_press.is_empty() {
        placer.cells_requested_this_press.clear();
    }
    if mouse_info.is_over_ui { return; }

    let (&PlacementObject(map_object), area, modes) = placement_session.into_inner();

    let should_place = match modes.place {
        PlacementMode::OnRelease => mouse.just_released(MouseButton::Left),
        PlacementMode::OnPress => mouse.pressed(MouseButton::Left) && placer.cells_requested_this_press.insert(area.origin.coords),
    };
    let should_remove = match modes.remove {
        PlacementMode::OnRelease => mouse.just_released(MouseButton::Right),
        PlacementMode::OnPress => mouse.pressed(MouseButton::Right),
    };

    if should_place {
        commands.trigger(PlaceRequest(map_object));
    } else if should_remove {
        commands.trigger(RemoveRequest(map_object));
    }
}

fn on_stop_placing_despawn_controls(
    _trigger: On<StopPlacing>,
    mut commands: Commands,
) {
    commands.despawn_all::<With<PlacementControls>>();
}

/// Replaces the placer's ghost and requests its world-space appearance.
///
/// Copies the session's imprint to the new ghost. Domain renderers read their settings from the
/// session.
fn on_grid_placer_ghost_rebuild_request_do_so(
    _trigger: On<GridPlacerGhostRebuildRequest>,
    mut commands: Commands,
    placer: Single<Entity, With<GridObjectPlacer>>,
    placement_session: Single<(&PlacementObject, &GridArea), With<PlacementSession>>,
) {
    commands.despawn_all::<With<GridPlacerGhost>>();

    let (&PlacementObject(map_object), area) = placement_session.into_inner();
    commands
        .spawn((GridPlacerGhost, area.imprint, ChildOf(*placer)))
        .trigger(|ghost| ObjectFaceRequest::ghost(ghost, map_object));
}

/// Moves and resizes the overlay to the session's area, then redraws it.
fn on_grid_placer_changed_fit_overlay(
    _trigger: On<GridPlacerChanged>,
    mut commands: Commands,
    mut materials: ResMut<Assets<GridPlacerMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    placer: Single<(Entity, &mut Transform, &MeshMaterial2d<GridPlacerMaterial>), With<GridObjectPlacer>>,
    placement_session: Single<(&GridArea, Option<&PlacementFeedback>), With<PlacementSession>>,
) {
    let (placer_entity, mut transform, material_handle) = placer.into_inner();
    let (&area, feedback) = placement_session.into_inner();
    let Some(mut material) = materials.get_mut(material_handle) else { return; };

    transform.translation = area.world_center().extend(transform.translation.z);

    let imprint_bounds = UVec2::from(Bounds::from(area.imprint));
    if material.uniform.bounds != imprint_bounds {
        commands.entity(placer_entity).insert(Mesh2d(meshes.add(Rectangle::from_size(area.imprint.world_size()))));
        material.uniform.bounds = imprint_bounds;
    }

    material.uniform.show(area, feedback);
}

fn on_insert_placement_feedback_redraw_overlay(
    _trigger: On<Insert<PlacementFeedback>>,
    mut materials: ResMut<Assets<GridPlacerMaterial>>,
    placer: Single<&MeshMaterial2d<GridPlacerMaterial>, With<GridObjectPlacer>>,
    placement_session: Single<(&GridArea, &PlacementFeedback), With<PlacementSession>>,
) {
    let (&area, feedback) = placement_session.into_inner();
    let Some(mut material) = materials.get_mut(*placer) else { return; };
    material.uniform.show(area, Some(feedback));
}
