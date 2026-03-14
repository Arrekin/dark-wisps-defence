use bevy::{
    reflect::TypePath,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
    sprite_render::{AlphaMode2d, Material2d, Material2dPlugin, MeshMaterial2d},
};
use lib_core::placement::{GridPlacerChanged, GridPlacerOverridePropertyRequest, StopPlacing};
use lib_inventory::placement::{
    CellHighlight, GridsCollectionParam, ObjectPlacementInfo, PlacementMode, PlacementValidity,
};

use crate::prelude::*;

pub struct GridObjectPlacerPlugin;
impl Plugin for GridObjectPlacerPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins(Material2dPlugin::<GridPlacerMaterial>::default())
            .insert_resource(GridObjectPlacerRequest::default())
            .add_systems(Startup, spawn_placer)
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
            .add_observer(GridObjectPlacer::on_modify)
            ;
    }
}

// ============================================================================
// MATERIAL
// ============================================================================

#[derive(ShaderType, Clone, Debug, Default)]
struct GridPlacerUniform {
    base_color: Vec4,
    cell_data: UVec4,  // 2 bits/cell: 0=inactive, 1=active, 2=highlighted
    cell_columns: u32,
    cell_rows: u32,
    use_texture: u32,
}

#[derive(Asset, AsBindGroup, TypePath, Debug, Clone)]
pub struct GridPlacerMaterial {
    #[uniform(0)]
    uniform: GridPlacerUniform,
    #[texture(1)]
    #[sampler(2)]
    preview_texture: Option<Handle<Image>>,
}
impl Default for GridPlacerMaterial {
    fn default() -> Self {
        Self { uniform: GridPlacerUniform::default(), preview_texture: None }
    }
}
impl Material2d for GridPlacerMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/grid_placer.wgsl".into()
    }
    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

fn update_material_imprint(material: &mut GridPlacerMaterial, imprint: GridImprint) {
    let (w, h) = imprint.bounds();
    material.uniform.cell_columns = w as u32;
    material.uniform.cell_rows = h as u32;
    material.uniform.cell_data = UVec4::ZERO;
}

/// Builds the packed cell_data for the shader from the imprint shape and annotations.
/// 2 bits per cell (up to 64 cells): 0=inactive, 1=active, 2=highlighted.
/// Annotated cells (positive or negative) map to state 2. The base_color already encodes validity.
fn build_cell_data(imprint: GridImprint, origin: GridCoords, annotations: &[(GridCoords, CellHighlight)]) -> UVec4 {
    let (width, height) = imprint.bounds();
    let mut words = [0u32; 4];
    for iy in 0..height {
        for ix in 0..width {
            let cell_coords = origin.shifted((ix, iy));
            let cell_index = (iy * width + ix) as usize;
            if cell_index >= 64 { continue; }

            let state: u32 = if imprint.covers_coords(origin, cell_coords) {
                if annotations.iter().any(|(c, _)| *c == cell_coords) { 2 } else { 1 }
            } else {
                0
            };

            words[cell_index / 16] |= state << ((cell_index % 16) * 2);
        }
    }
    UVec4::new(words[0], words[1], words[2], words[3])
}

// ============================================================================
// RESOURCE
// ============================================================================

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

// ============================================================================
// COMPONENT
// ============================================================================

/// Active placement session data.
pub struct ActivePlacement {
    pub map_object: MapObject,
    pub placement_info: ObjectPlacementInfo,
}

#[derive(Component, Default)]
#[require(GridImprint, GridCoords, ZDepth = ZDepth(10.), AutoGridTransformSync)]
pub struct GridObjectPlacer {
    pub active_placement: Option<ActivePlacement>,
}

impl GridObjectPlacer {
    pub fn map_object(&self) -> Option<MapObject> {
        self.active_placement.as_ref().map(|a| a.map_object)
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
        placer: Single<&mut GridImprint, With<GridObjectPlacer>>,
    ) {
        let mut grid_imprint = placer.into_inner();
        match *trigger.event() {
            GridPlacerOverridePropertyRequest::OverrideImprint(imprint) => {
                *grid_imprint = imprint;
            }
        }
        commands.trigger(GridPlacerChanged);
    }

    fn revalidate_on_change(
        _trigger: On<GridPlacerChanged>,
        mut commands: Commands,
        mut meshes: ResMut<Assets<Mesh>>,
        grids: GridsCollectionParam,
        mut materials: ResMut<Assets<GridPlacerMaterial>>,
        placer: Single<(Entity, &GridObjectPlacer, &GridImprint, &GridCoords, &MeshMaterial2d<GridPlacerMaterial>)>,
    ) {
        let (placer_entity, grid_object_placer, grid_imprint, grid_coords, material_handle) = placer.into_inner();
        let Some(active_placement) = &grid_object_placer.active_placement else { return; };
        let Some(material) = materials.get_mut(material_handle) else { return; };

        let (w, h) = grid_imprint.bounds();
        if material.uniform.cell_columns != w as u32 || material.uniform.cell_rows != h as u32 {
            commands.entity(placer_entity).insert(Mesh2d(meshes.add(Rectangle::from_size(grid_imprint.world_size()))));
            update_material_imprint(material, *grid_imprint);
        }

        let new_preview = active_placement.placement_info.preview_image.clone();
        if material.preview_texture != new_preview {
            material.uniform.use_texture = if new_preview.is_some() { 1 } else { 0 };
            material.preview_texture = new_preview;
        }

        let validity = (active_placement.placement_info.validate)(active_placement.map_object, *grid_coords, *grid_imprint, &grids);
        let annotations = (active_placement.placement_info.annotate)(active_placement.map_object, *grid_coords, *grid_imprint, validity, &grids);

        let base = match validity {
            PlacementValidity::Valid          => LinearRgba::new(0.0, 1.0, 0.0, 0.2),
            PlacementValidity::ValidUnpowered => LinearRgba::new(1.0, 1.0, 0.0, 0.2),
            PlacementValidity::Invalid        => LinearRgba::new(1.0, 0.0, 0.0, 0.2),
        };
        material.uniform.base_color = Vec4::new(base.red, base.green, base.blue, base.alpha);
        material.uniform.cell_data = build_cell_data(*grid_imprint, *grid_coords, &annotations);
    }
}

// ============================================================================
// SYSTEMS
// ============================================================================

fn spawn_placer(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<GridPlacerMaterial>>,
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

fn on_placing_enter_system(placer: Single<&mut Visibility, With<GridObjectPlacer>>) {
    *placer.into_inner() = Visibility::Inherited;
}

fn on_placing_exit_system(
    mut commands: Commands,
    placer: Single<(&mut Visibility, &mut GridObjectPlacer)>,
) {
    let (mut visibility, mut placer) = placer.into_inner();
    *visibility = Visibility::Hidden;
    placer.active_placement = None;
    commands.trigger(StopPlacing);
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
        return;
    };
    grid_object_placer_request.set(map_object);
}

fn on_request_grid_object_placer_system(
    mut commands: Commands,
    almanach: Res<Almanach>,
    mut ui_interaction_state: ResMut<NextState<UiInteraction>>,
    placer: Single<(&mut GridObjectPlacer, &mut GridImprint)>,
    mut placer_request: ResMut<GridObjectPlacerRequest>,
) {
    let Some(map_object) = placer_request.take() else { return; };
    let (mut grid_object_placer, mut grid_imprint) = placer.into_inner();

    if grid_object_placer.active_placement.is_some() {
        commands.trigger(StopPlacing);
    }

    let mut placement_info = almanach.get_placement_info_for(map_object);
    *grid_imprint = placement_info.imprint;

    if let Some(emitter) = placement_info.begin_placing_emitter.take() {
        emitter.emit(&mut commands);
    }

    grid_object_placer.active_placement = Some(ActivePlacement { map_object, placement_info });

    (*ui_interaction_state).set_if_neq(UiInteraction::PlaceGridObject);
    commands.trigger(GridPlacerChanged);
}

fn on_click_place_system(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    mouse_info: Res<MouseInfo>,
    placer: Single<&GridObjectPlacer>,
) {
    if mouse_info.is_over_ui { return; }

    let Some(ref active_placement) = placer.active_placement else { return };

    let should_place = match active_placement.placement_info.place_mode {
        PlacementMode::OnRelease => mouse.just_released(MouseButton::Left),
        PlacementMode::OnPress => mouse.pressed(MouseButton::Left),
    };
    let should_remove = match active_placement.placement_info.remove_mode {
        PlacementMode::OnRelease => mouse.just_released(MouseButton::Right),
        PlacementMode::OnPress => mouse.pressed(MouseButton::Right),
    };

    if should_place {
        active_placement.placement_info.place_emitter.emit(&mut commands);
    } else if should_remove {
        if let Some(remove_emitter) = &active_placement.placement_info.remove_emitter {
            remove_emitter.emit(&mut commands);
        }
    }
}
