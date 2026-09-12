use bevy::{
    prelude::*,
    reflect::TypePath,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
    sprite_render::{AlphaMode2d, Material2d, Material2dPlugin, MeshMaterial2d},
};

use almanach::prelude::Almanach;
use game_core::prelude::{Bounds, BuildingType, CELL_SIZE, GridCoords, GridImprint, MapObject, ObjectFaceRequest, TowerType, ZDepth};
use grids::placement::*;
use states::prelude::UiInteraction;
use viewport::MouseInfo;

pub struct GridObjectPlacerPlugin;
impl Plugin for GridObjectPlacerPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins(Material2dPlugin::<GridPlacerMaterial>::default())
            .insert_resource(GridObjectPlacerRequest::default())
            .add_systems(Startup, spawn_placer)
            .add_systems(PreUpdate, (
                follow_mouse_system.run_if(in_state(UiInteraction::PlaceGridObject)),
                keyboard_input_system,
            ))
            .add_systems(Update, (
                begin_placement.run_if(GridObjectPlacerRequest::there_is_request()),
                handle_placement_click.run_if(in_state(UiInteraction::PlaceGridObject)),
            ))
            .add_systems(OnEnter(UiInteraction::PlaceGridObject), show_placer)
            .add_systems(OnExit(UiInteraction::PlaceGridObject), hide_placer)
            .add_observer(revalidate_placement)
            .add_observer(on_modify_apply_placer_override)
            .add_observer(on_ghost_stale_respawn_ghost)
            ;
    }
}

// ============================================================================
// MATERIAL
// ============================================================================

#[derive(ShaderType, Clone, Debug, Default)]
struct GridPlacerUniform {
    cell_data: UVec4,
    cell_columns: u32,
    cell_rows: u32,
    /// One of the `VALIDITY_*` constants in `assets/shaders/grid_placer.wgsl`.
    validity: u32,
}
impl GridPlacerUniform {
    /// Updates the grid dimensions and clears all packed cell states.
    fn reset_to_imprint(&mut self, imprint: GridImprint) {
        (self.cell_columns, self.cell_rows) = Bounds::from(imprint).as_u32();
        self.cell_data = UVec4::ZERO;
    }
    fn bounds_match(&self, bounds: Bounds) -> bool {
        (self.cell_columns, self.cell_rows) == bounds.as_u32()
    }
}

#[derive(Asset, AsBindGroup, TypePath, Debug, Clone)]
#[derive(Default)]
pub(crate) struct GridPlacerMaterial {
    #[uniform(0)]
    uniform: GridPlacerUniform,
}
impl Material2d for GridPlacerMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/grid_placer.wgsl".into()
    }
    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

/// Packs the imprint shape and its annotations two bits per cell, up to 64 cells. The values are
/// the `CELL_*` constants in `assets/shaders/grid_placer.wgsl`.
fn build_cell_data(imprint: GridImprint, origin: GridCoords, annotations: &[(GridCoords, CellHighlight)]) -> UVec4 {
    let imprint_bounds = Bounds::from(imprint);
    let mut words = [0u32; 4];
    for (cell_index, local_coords) in imprint_bounds.iter().enumerate() {
        if cell_index >= 64 { break; }

        let cell_coords = origin.shifted(local_coords.into());
        let state: u32 = if imprint.covers_coords(origin, cell_coords) {
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

// ============================================================================
// SYSTEMS
// ============================================================================

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

fn on_modify_apply_placer_override(
    trigger: On<GridPlacerOverridePropertyRequest>,
    mut commands: Commands,
    placer: Single<(&mut GridImprint, &mut PlacementStyle), With<GridObjectPlacer>>,
) {
    let (mut grid_imprint, mut placement_style) = placer.into_inner();
    match *trigger.event() {
        GridPlacerOverridePropertyRequest::OverrideImprint(imprint) => {
            *grid_imprint = imprint;
        }
        GridPlacerOverridePropertyRequest::OverrideStyle(style) => {
            placement_style.0 = style;
        }
    }

    commands.trigger(PlacementGhostStale);
    commands.trigger(GridPlacerChanged);
}

/// Signals that the placement ghost must be rebuilt after its object, imprint, or style changes.
#[derive(Event)]
struct PlacementGhostStale;

/// Replaces the placer's ghost child and requests its world-space face.
///
/// The ghost carries the current imprint and style required by domain renderers. This assumes the
/// ghost is the placer's only child.
fn on_ghost_stale_respawn_ghost(
    _trigger: On<PlacementGhostStale>,
    mut commands: Commands,
    placer: Single<(Entity, &GridObjectPlacer, &GridImprint, &PlacementStyle)>,
) {
    let (placer_entity, grid_object_placer, grid_imprint, placement_style) = placer.into_inner();
    let mut placer_commands = commands.entity(placer_entity);
    placer_commands.despawn_children();

    let Some(map_object) = grid_object_placer.map_object() else { return };
    placer_commands.with_children(|placer| {
        placer
            .spawn((ZDepth::GRID_PLACER_GHOST, *grid_imprint, *placement_style))
            .trigger(|ghost| ObjectFaceRequest::ghost(ghost, map_object));
    });
}

fn revalidate_placement(
    _trigger: On<GridPlacerChanged>,
    mut commands: Commands,
    mut materials: ResMut<Assets<GridPlacerMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    grids: GridsCollectionParam,
    placer: Single<(Entity, &GridObjectPlacer, &GridImprint, &GridCoords, &MeshMaterial2d<GridPlacerMaterial>)>,
) {
    let (placer_entity, grid_object_placer, grid_imprint, grid_coords, material_handle) = placer.into_inner();
    let Some(active_placement) = &grid_object_placer.active_placement else { return; };
    let Some(mut material) = materials.get_mut(material_handle) else { return; };

    let imprint_bounds = Bounds::from(*grid_imprint);
    if !material.uniform.bounds_match(imprint_bounds) {
        commands.entity(placer_entity).insert(Mesh2d(meshes.add(Rectangle::from_size(grid_imprint.world_size()))));
        material.uniform.reset_to_imprint(*grid_imprint);
    }

    let validity = (active_placement.placement_info.validate)(active_placement.map_object, *grid_coords, *grid_imprint, &grids);
    let annotations = (active_placement.placement_info.annotate)(active_placement.map_object, *grid_coords, *grid_imprint, validity, &grids);

    material.uniform.validity = validity.shader_index();
    material.uniform.cell_data = build_cell_data(*grid_imprint, *grid_coords, &annotations);
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

fn keyboard_input_system(
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
    placer: Single<(&mut GridObjectPlacer, &mut GridImprint, &mut PlacementStyle)>,
) {
    let Some(map_object) = placer_request.take() else { return; };
    let (mut grid_object_placer, mut grid_imprint, mut placement_style) = placer.into_inner();

    if grid_object_placer.active_placement.is_some() {
        commands.trigger(StopPlacing);
    }

    let placement_info = almanach.get_placement_info_for(map_object);
    *grid_imprint = placement_info.imprint;
    *placement_style = PlacementStyle::default();

    grid_object_placer.active_placement = Some(ActivePlacement { map_object, placement_info });

    commands.trigger(PlacementGhostStale);
    commands.trigger(BeginPlacing(map_object));
    commands.trigger(GridPlacerChanged);

    (*ui_interaction_state).set_if_neq(UiInteraction::PlaceGridObject);
}

fn handle_placement_click(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    mouse_info: Res<MouseInfo>,
    placer: Single<&GridObjectPlacer>,
) {
    if mouse_info.is_over_ui { return; }

    let Some(ref active_placement) = placer.active_placement else { return };
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
