use std::collections::VecDeque;

use bevy::{
    input::common_conditions::input_just_released,
    platform::collections::HashSet,
    prelude::*,
    reflect::TypePath,
    render::{
        render_resource::AsBindGroup,
        storage::ShaderBuffer,
    },
    shader::ShaderRef,
    sprite_render::{AlphaMode2d, Material2d, Material2dPlugin, MeshMaterial2d},
};

use almanach::prelude::Almanach;
use alteration::modifiers::prelude::ModifierType;
use buildings::prelude::Tower;
use game_core::prelude::{BuildingType, GridCoords, GridImprint, MapInfo, MapObject, ZDepth};
use grids::{
    placement::{GridObjectPlacer, GridPlacerChanged},
    prelude::GridVersion,
    search::common::VISITED_GRID,
    tower_ranges::TowerRangesGrid,
};
use hud::prelude::FocusedMapObject;
use states::prelude::{MapLoadingStage, UiInteraction};
use visuals::{canvas::MapCanvasBundle, specialize_with_shared_gpu};

pub struct TowersRangeOverlayPlugin;
impl Plugin for TowersRangeOverlayPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins(Material2dPlugin::<TowersRangeMaterial>::default())
            .init_state::<TowersRangeOverlayState>()
            .init_resource::<TowersRangeOverlayConfig>()
            .add_systems(OnEnter(MapLoadingStage::LoadResources), TowersRangeOverlay::create)
            .add_systems(OnEnter(TowersRangeOverlayState::Show), |visibility: Single<&mut Visibility, With<TowersRangeOverlay>>| { *visibility.into_inner() = Visibility::Inherited; })
            .add_systems(OnExit(TowersRangeOverlayState::Show), |visibility: Single<&mut Visibility, With<TowersRangeOverlay>>| { *visibility.into_inner() = Visibility::Hidden; })
            .add_systems(OnExit(UiInteraction::PlaceGridObject), |mut config: ResMut<TowersRangeOverlayConfig>| { config.secondary_mode = TowersRangeOverlaySecondaryMode::None; })
            .add_systems(Update, (
                TowersRangeOverlayConfig::on_config_change_system.run_if(resource_changed::<TowersRangeOverlayConfig>),
                refresh_display_system.run_if(in_state(TowersRangeOverlayState::Show)),
                (|mut config: ResMut<TowersRangeOverlayConfig>| { config.is_overlay_globally_enabled ^= true; }).run_if(input_just_released(KeyCode::Digit8)),
            ))
            .add_observer(TowersRangeOverlayConfig::on_insert_focused_map_object_set_highlight)
            .add_observer(TowersRangeOverlayConfig::on_remove_focused_map_object_clear_highlight)
            .add_observer(on_grid_placer_changed_preview_placement);
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
    pub secondary_mode: TowersRangeOverlaySecondaryMode,
}
impl TowersRangeOverlayConfig {
    fn on_config_change_system(
        overlay_config: Res<TowersRangeOverlayConfig>,
        mut overlay_state: ResMut<NextState<TowersRangeOverlayState>>,
    ) {
        let shown = overlay_config.is_overlay_globally_enabled || !overlay_config.secondary_mode.is_none();
        overlay_state.set_if_different(if shown { TowersRangeOverlayState::Show } else { TowersRangeOverlayState::Hide });
    }
    fn on_insert_focused_map_object_set_highlight(
        trigger: On<Insert<FocusedMapObject>>,
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
    fn on_remove_focused_map_object_clear_highlight(
        _trigger: On<Remove<FocusedMapObject>>,
        mut overlay_config: ResMut<TowersRangeOverlayConfig>,
    ) {
        overlay_config.secondary_mode = TowersRangeOverlaySecondaryMode::None;
    }
}

/// Temporary override of what the overlay shows.
/// - `None`: not active
/// - `Highlight { tower }`: emphasizes the selected tower's range
/// - `PlacingTower { .. }`: previews the range flood from the planned building footprint
#[derive(Default, Clone, Debug, PartialEq)]
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
}
impl Material2d for TowersRangeMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/overlays/towers_ranges_map.wesl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }

    specialize_with_shared_gpu!();
}

#[derive(Component, Default)]
#[require(ZDepth::OverlayTowerRanges)]
pub struct TowersRangeOverlay {
    /// Grid version and mode used for the last refresh; `None` before the first refresh.
    shown: Option<(GridVersion, TowersRangeOverlaySecondaryMode)>,
}
impl TowersRangeOverlay {
    /// Records the requested version and mode. Returns whether the caller needs to refresh the buffer.
    fn record_shown(&mut self, version: GridVersion, mode: &TowersRangeOverlaySecondaryMode) -> bool {
        if self.shown.as_ref().is_some_and(|(shown_version, shown_mode)| *shown_version == version && shown_mode == mode) { return false; }
        self.shown = Some((version, mode.clone()));
        true
    }

    fn create(
        mut commands: Commands,
        map_info: Res<MapInfo>,
        overlay_state: Res<State<TowersRangeOverlayState>>,
        mut materials: ResMut<Assets<TowersRangeMaterial>>,
        mut buffers: ResMut<Assets<ShaderBuffer>>,
    ) {
        commands.despawn_all::<With<TowersRangeOverlay>>();

        // Bind an initialized buffer on the material's spawn frame.
        let cells = buffers.add(ShaderBuffer::from(vec![TowerRangeCell::default(); map_info.grid_bounds.area()]));
        let material = materials.add(TowersRangeMaterial { cells });
        commands.spawn((
            MapCanvasBundle::new(material, &map_info),
            TowersRangeOverlay::default(),
            if *overlay_state.get() == TowersRangeOverlayState::Show { Visibility::Inherited } else { Visibility::Hidden },
        ));
    }
}

fn refresh_display_system(
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    materials: Res<Assets<TowersRangeMaterial>>,
    tower_ranges_grid: Res<TowerRangesGrid>,
    overlay_config: Res<TowersRangeOverlayConfig>,
    towers_range_overlay: Single<(&MeshMaterial2d<TowersRangeMaterial>, &mut TowersRangeOverlay)>,
) {
    let (overlay_material, mut overlay) = towers_range_overlay.into_inner();
    if !overlay.record_shown(tower_ranges_grid.version, &overlay_config.secondary_mode) { return; }

    let overlay_material = materials.get(overlay_material).unwrap();
    let mut buffer = buffers.get_mut(&overlay_material.cells).unwrap();

    let mut overlay_creator = OverlayBufferCreator::new(&tower_ranges_grid, &mut buffer);
    match &overlay_config.secondary_mode {
        TowersRangeOverlaySecondaryMode::None => overlay_creator.generate_buffer_data(&HighlightMode::None),
        TowersRangeOverlaySecondaryMode::Highlight { tower } => {
            overlay_creator.generate_buffer_data(&HighlightMode::Selected(HashSet::from([*tower])))
        }
        TowersRangeOverlaySecondaryMode::PlacingTower {
            grid_coords,
            grid_imprint,
            range,
        } => {
            if grid_coords.are_in_bounds(tower_ranges_grid.bounds) {
                overlay_creator.flood_preview_to_overlay(
                    grid_imprint.iter_in_bounds(*grid_coords, tower_ranges_grid.bounds),
                    *range,
                )
            } else {
                overlay_creator.generate_buffer_data(&HighlightMode::None)
            }
        }
    };
}

fn on_grid_placer_changed_preview_placement(
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

/// Field order and types mirror `TowerRangeCell` in `assets/shaders/overlays/towers_ranges_map.wesl`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable, Default)]
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
    Selected(HashSet<Entity>),
}

/// Builds the tower range overlay buffer for the shader.
/// Writes into the caller's `ShaderBuffer`, reusing its allocation, and can
/// optionally apply a preview flood to set the `highlight` bit.
struct OverlayBufferCreator<'a> {
    grid: &'a TowerRangesGrid,
    buffer: &'a mut ShaderBuffer,
}
impl<'a> OverlayBufferCreator<'a> {
    fn new(grid: &'a TowerRangesGrid, buffer: &'a mut ShaderBuffer) -> Self {
        Self { grid, buffer }
    }

    /// Rebuilds the entire buffer for the current grid and highlight mode in O(n) over cells.
    fn generate_buffer_data(&mut self, highlight_mode: &HighlightMode) {
        let grid = self.grid;
        self.buffer.clear();
        self.buffer.extend(grid.grid.iter().map(|set| Self::create_cell_for_grid_field(set, highlight_mode)));
    }

    fn create_cell_for_grid_field(set: &HashSet<Entity>, highlight_mode: &HighlightMode) -> TowerRangeCell {
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
                if selected_entities.iter().any(|entity| set.contains(entity)) { 1 } else { 0 }
            }
        };
        TowerRangeCell { signature, cover_count, highlight }
    }

    /// Distance-limited BFS flood from all provided start cells (the building's imprint).
    /// Only sets the `highlight` bit; leaves `signature` intact.
    fn flood_preview_to_overlay(
        &mut self,
        start_coords: impl IntoIterator<Item = GridCoords>,
        range: usize,
    ) {
        // Start with base buffer data (all signatures, nothing highlighted)
        self.generate_buffer_data(&HighlightMode::None);
        let buffer_data = self.buffer.cast_slice_mut::<TowerRangeCell>().unwrap();
        let grid_bounds = self.grid.bounds;

        VISITED_GRID.with_borrow_mut(|visited_grid| {
            visited_grid.resize_and_clear(grid_bounds);
            let mut queue = VecDeque::new();

            // Flood from every imprint cell, so the distance is even around buildings.
            start_coords.into_iter().for_each(|coords| {
                let index = self.grid.index(coords);
                // Do not alter signature; set highlight flag
                buffer_data[index].highlight = 1;
                queue.push_back((0, coords));
                visited_grid.set_visited(coords);
            });

            while let Some((distance, coords)) = queue.pop_front() {
                for new_coords in grid_bounds.cardinal_neighbors(coords) {
                    if visited_grid.is_visited(new_coords) {
                        continue;
                    }
                    visited_grid.set_visited(new_coords);

                    let buffer_index = self.grid.index(new_coords);
                    buffer_data[buffer_index].highlight = 1;

                    let new_distance = distance + 1;
                    if new_distance < range {
                        queue.push_back((new_distance, new_coords));
                    }
                }
            }
        });
    }
}
