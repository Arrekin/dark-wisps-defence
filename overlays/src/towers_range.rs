use bevy::{
    input::common_conditions::input_just_released,
    prelude::*,
    reflect::TypePath,
    render::{
        render_resource::{AsBindGroup, ShaderType},
        storage::ShaderBuffer,
    },
    shader::ShaderRef,
    sprite_render::{AlphaMode2d, Material2d, Material2dPlugin, MeshMaterial2d},
};

use almanach::prelude::Almanach;
use alteration::modifiers::prelude::ModifierType;
use buildings::prelude::Tower;
use game_core::prelude::{BuildingType, GridCoords, GridImprint, MapObject, ZDepth, Z_OVERLAY_TOWER_RANGES};
use grids::{
    placement::{GridObjectPlacer, GridPlacerChanged},
    prelude::{GridVersion, MapInfo},
    search::common::{CARDINAL_DIRECTIONS, VISITED_GRID},
    tower_ranges::TowerRangesGrid,
};
use hud::prelude::FocusedMapObject;
use states::prelude::{MapLoadingStage, UiInteraction};

pub struct TowersRangeOverlayPlugin;
impl Plugin for TowersRangeOverlayPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins(Material2dPlugin::<TowersRangeMaterial>::default())
            .init_state::<TowersRangeOverlayState>()
            .init_resource::<TowersRangeOverlayConfig>()
            .add_systems(OnEnter(MapLoadingStage::LoadResources), TowersRangeOverlay::create)
            .add_systems(OnEnter(TowersRangeOverlayState::Show), |visibility: Single<&mut Visibility, With<TowersRangeOverlay>>| { *visibility.into_inner() = Visibility::Inherited; },)
            .add_systems(OnExit(TowersRangeOverlayState::Show),|visibility: Single<&mut Visibility, With<TowersRangeOverlay>>| { *visibility.into_inner() = Visibility::Hidden; },)
            .add_systems(OnExit(UiInteraction::PlaceGridObject),|mut config: ResMut<TowersRangeOverlayConfig>| { config.secondary_mode = TowersRangeOverlaySecondaryMode::None; },)
            .add_systems(
                Update,
                (
                    TowersRangeOverlayConfig::on_config_change_system.run_if(resource_changed::<TowersRangeOverlayConfig>),
                    refresh_display_system.run_if(in_state(TowersRangeOverlayState::Show)),
                    (|mut config: ResMut<TowersRangeOverlayConfig>| { config.is_overlay_globally_enabled ^= true; }).run_if(input_just_released(KeyCode::Digit8)), // Switch overlay on/off 
                ),
            )
            .add_observer(TowersRangeOverlayConfig::on_map_object_focused)
            .add_observer(TowersRangeOverlayConfig::on_map_object_unfocused)
            .add_observer(on_grid_placer_changed)
            ;
    }
}

#[derive(States, Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum TowersRangeOverlayState {
    #[default]
    Hide,
    Show,
}
#[derive(Resource, Default)]
pub struct TowersRangeOverlayConfig {
    pub is_overlay_globally_enabled: bool,
    pub grid_version: GridVersion, // Grid version for which we show the overlay
    pub secondary_mode: TowersRangeOverlaySecondaryMode,
}
impl TowersRangeOverlayConfig {
    fn on_config_change_system(
        overlay_config: Res<TowersRangeOverlayConfig>,
        mut overlay_state: ResMut<NextState<TowersRangeOverlayState>>,
    ) {
        if overlay_config.is_overlay_globally_enabled || !overlay_config.secondary_mode.is_none() {
            overlay_state.set(TowersRangeOverlayState::Show);
        } else {
            overlay_state.set(TowersRangeOverlayState::Hide);
        }
    }
    fn on_map_object_focused(
        trigger: On<Insert, FocusedMapObject>,
        mut overlay_config: ResMut<TowersRangeOverlayConfig>,
        towers: Query<(), With<Tower>>,
    ) {
        let focused_entity = trigger.entity;
        if towers.contains(focused_entity) {
            overlay_config.secondary_mode = TowersRangeOverlaySecondaryMode::Highlight { tower: focused_entity };
        } else {
            overlay_config.secondary_mode = TowersRangeOverlaySecondaryMode::None;
        }
    }
    fn on_map_object_unfocused(
        _trigger: On<Remove, FocusedMapObject>,
        mut overlay_config: ResMut<TowersRangeOverlayConfig>,
    ) {
        overlay_config.secondary_mode = TowersRangeOverlaySecondaryMode::None;
    }
}
#[derive(Default, Clone, Debug, PartialEq)]
/// Secondary mode is temporary override 
/// - `None`: Mode not active
/// - `Highlight { tower }`: emphasize the selected tower's range
/// - `PlacingTower { .. }`: preview of the range flood from the planned building footprint
pub enum TowersRangeOverlaySecondaryMode {
    #[default]
    None,
    Highlight { tower: Entity },
    PlacingTower {
        grid_coords: GridCoords,
        grid_imprint: GridImprint,
        range: usize,
    },
}
impl TowersRangeOverlaySecondaryMode {
    pub fn is_none(&self) -> bool {
        matches!(self, TowersRangeOverlaySecondaryMode::None)
    }
}

#[derive(Asset, AsBindGroup, TypePath, Debug, Clone, Default)]
struct TowersRangeMaterial {
    #[storage(0, read_only)]
    pub cells: Handle<ShaderBuffer>,
    #[uniform(1)]
    pub grid_data: super::UniformGridData,
}
impl Material2d for TowersRangeMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/towers_ranges_map.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

#[derive(Component)]
pub struct TowersRangeOverlay;
impl TowersRangeOverlay {
    fn create(
        mut commands: Commands,
        map_info: Res<MapInfo>,
        mut meshes: ResMut<Assets<Mesh>>,
        mut materials: ResMut<Assets<TowersRangeMaterial>>,
        overlay: Query<Entity, With<TowersRangeOverlay>>,
    ) {
        if let Ok(overlay_entity) = overlay.single() {
            commands.entity(overlay_entity).despawn();
        };

        commands.spawn((
            super::overlay_bundle(&mut meshes, &mut materials, &map_info),
            TowersRangeOverlay,
            ZDepth(Z_OVERLAY_TOWER_RANGES),
        ));
    }
}

fn refresh_display_system(
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    mut materials: ResMut<Assets<TowersRangeMaterial>>,
    tower_ranges_grid: Res<TowerRangesGrid>,
    mut overlay_config: ResMut<TowersRangeOverlayConfig>,
    towers_range_overlay: Single<&MeshMaterial2d<TowersRangeMaterial>, With<TowersRangeOverlay>>,
    mut last_secondary_mode: Local<TowersRangeOverlaySecondaryMode>,
    mut local_buffer_data: Local<Vec<TowerRangeCell>>, // To avoid re-allocations every frame
) {
    if overlay_config.grid_version == tower_ranges_grid.version && overlay_config.secondary_mode == *last_secondary_mode {
        return;
    }

    *last_secondary_mode = overlay_config.secondary_mode.clone();
    overlay_config.grid_version = tower_ranges_grid.version;
    let mut overlay_material = materials.get_mut(towers_range_overlay.into_inner()).unwrap();

    // Generate buffer data
    let mut overlay_creator = OverlayBufferCreator::new(&tower_ranges_grid, &mut local_buffer_data);
    match &overlay_config.secondary_mode {
        TowersRangeOverlaySecondaryMode::None => overlay_creator.generate_buffer_data(&HighlightMode::None),
        TowersRangeOverlaySecondaryMode::Highlight { tower } => {
            overlay_creator.generate_buffer_data(&HighlightMode::Selected(vec![*tower]))
        }
        TowersRangeOverlaySecondaryMode::PlacingTower {
            grid_coords,
            grid_imprint,
            range,
        } => {
            if grid_coords.is_in_bounds(tower_ranges_grid.bounds()) {
                overlay_creator.flood_preview_to_overlay(
                    grid_imprint.iter_in_bounds(*grid_coords, tower_ranges_grid.bounds()),
                    *range,
                )
            } else {
                overlay_creator.generate_buffer_data(&HighlightMode::None)
            }
        }
    };

    let buffer_handle = &overlay_material.cells;
    if let Some(mut buffer) = buffers.get_mut(buffer_handle) {
        buffer.set_data(&*local_buffer_data);
    } else {
        // Create ShaderBuffer
        let storage_buffer = ShaderBuffer::from(local_buffer_data.as_slice());
        let buffer_handle = buffers.add(storage_buffer);
        overlay_material.cells = buffer_handle;
    }

    // Update uniforms
    let bounds = tower_ranges_grid.bounds();
    overlay_material.grid_data.grid_width = bounds.0 as u32;
    overlay_material.grid_data.grid_height = bounds.1 as u32;
}

fn on_grid_placer_changed(
    _trigger: On<GridPlacerChanged>,
    almanach: Res<Almanach>,
    mut overlay_config: ResMut<TowersRangeOverlayConfig>,
    grid_object_placer: Single<(&GridObjectPlacer, &GridCoords)>,
) {
    let (grid_object_placer, grid_coords) = grid_object_placer.into_inner();
    let map_object = grid_object_placer.map_object();
    if let Some(MapObject::Building(building_type @ BuildingType::Tower(_))) = map_object {
        let building_info = almanach.get_building_info(building_type);
        overlay_config.secondary_mode = TowersRangeOverlaySecondaryMode::PlacingTower {
            grid_coords: *grid_coords,
            grid_imprint: building_info.grid_imprint,
            range: building_info.baseline[&ModifierType::AttackRange] as usize,
        };
    } else {
        overlay_config.secondary_mode = TowersRangeOverlaySecondaryMode::None;
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable, ShaderType, Default)]
struct TowerRangeCell {
    /// XOR signature of towers covering this cell
    signature: u32,
    /// Number of towers covering this cell
    cover_count: u32,
    /// 1 if the cell is highlighted (either selected range or preview), otherwise 0
    highlight: u32,
}

#[derive(PartialEq)]
enum HighlightMode {
    None,
    Selected(Vec<Entity>),
}

/// Builds the tower range overlay buffer for the shader.
/// Reuses a caller-provided `Vec<TowerRangeCell>` to avoid allocations and can
/// optionally apply a preview flood to set the `highlight` bit.
struct OverlayBufferCreator<'a> {
    grid: &'a TowerRangesGrid,
    local_buffer_data: Option<&'a mut Vec<TowerRangeCell>>,
}
impl<'a> OverlayBufferCreator<'a> {
    /// Helper that produces the GPU buffer content for the shader.
    /// Owns no memory; reuses a caller-provided `Vec<TowerRangeCell>` to avoid re-allocation.
    fn new(grid: &'a TowerRangesGrid, local_buffer_data: &'a mut Vec<TowerRangeCell>) -> Self {
        Self { grid, local_buffer_data: Some(local_buffer_data) }
    }

    /// Rebuilds the entire buffer for the current grid and highlight mode in O(n) over cells.
    fn generate_buffer_data(&mut self, highlight_mode: &HighlightMode) {
        let buffer_data = self.local_buffer_data.take().unwrap(); // Avoid double mutable borrows
        buffer_data.clear();
        let buffer_size = self.grid.grid.len();
        let new_content = (0..buffer_size).map(|idx| self.create_cell_for_grid_index(idx, highlight_mode));
        buffer_data.extend(new_content);
        self.local_buffer_data = Some(buffer_data);
    }

    fn create_cell_for_grid_index(&self, idx: usize, highlight_mode: &HighlightMode) -> TowerRangeCell {
        let set = &self.grid.grid[idx];
        if set.is_empty() {
            return TowerRangeCell::default();
        }
        let mut signature: u32 = 0;
        for &entity in set.iter() {
            signature ^= entity.index_u32();
        }
        let cover_count = set.len() as u32;
        let highlight = match highlight_mode {
            HighlightMode::None => 0u32,
            HighlightMode::Selected(selected_entities) => {
                if selected_entities.iter().any(|e| set.contains(e)) { 1 } else { 0 }
            }
        };
        TowerRangeCell { signature, cover_count, highlight }
    }

    /// Distance-limited BFS flood from all provided start cells (building footprint).
    /// Accepts owned `GridCoords` so callers can pass a `Vec<GridCoords>` directly.
    /// Only sets the `highlight` bit; leaves `signature` intact.
    fn flood_preview_to_overlay(
        &mut self,
        start_coords: impl IntoIterator<Item = GridCoords>,
        range: usize,
    ) {
        use std::collections::VecDeque;

        // Start with base buffer data (all signatures + optional selection)
        self.generate_buffer_data(&HighlightMode::None);
        let buffer_data = self.local_buffer_data.take().unwrap();
        let bounds = self.grid.bounds();

        VISITED_GRID.with_borrow_mut(|visited_grid| {
            visited_grid.resize_and_reset(bounds);
            let mut queue = VecDeque::new();

            // Start flood from all cells to ensure even distance from buildings bigger than one cell
            start_coords.into_iter().for_each(|coords| {
                let index = self.grid.index(coords);
                // Do not alter signature; set highlight flag
                let mut cell = buffer_data[index];
                cell.highlight = 1;
                buffer_data[index] = cell;
                queue.push_back((0, coords));
                visited_grid.set_visited(coords);
            });

            while let Some((distance, coords)) = queue.pop_front() {
                for (dx, dy) in CARDINAL_DIRECTIONS {
                    let new_coords = coords.shifted((dx, dy));
                    if !new_coords.is_in_bounds(bounds) || visited_grid.is_visited(new_coords) {
                        continue;
                    }
                    visited_grid.set_visited(new_coords);

                    let buffer_index = self.grid.index(new_coords);
                    let mut cell = buffer_data[buffer_index];
                    cell.highlight = 1;
                    buffer_data[buffer_index] = cell;

                    let new_distance = distance + 1;
                    if new_distance < range {
                        queue.push_back((new_distance, new_coords));
                    }
                }
            }
        });

        self.local_buffer_data = Some(buffer_data);
    }
}
