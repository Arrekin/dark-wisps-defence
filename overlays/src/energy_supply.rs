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
use game_core::prelude::{BuildingType, GridCoords, GridImprint, MapInfo, MapObject, ZDepth};
use grids::{
    energy_supply::{EnergySupplyField, EnergySupplyGrid},
    placement::{GridObjectPlacer, GridPlacerChanged},
    prelude::GridVersion,
    search::common::VISITED_GRID,
};
use hud::prelude::FocusedMapObject;
use states::prelude::{MapLoadingStage, UiInteraction};
use visuals::{canvas::MapCanvasBundle, specialize_with_shared_gpu};

pub struct EnergySupplyOverlayPlugin;
impl Plugin for EnergySupplyOverlayPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins(Material2dPlugin::<EnergySupplyHeatmapMaterial>::default())
            .init_state::<EnergySupplyOverlayState>()
            .init_resource::<EnergySupplyOverlayConfig>()
            .add_systems(OnEnter(MapLoadingStage::LoadResources), EnergySupplyOverlay::create)
            .add_systems(OnEnter(EnergySupplyOverlayState::Show), |visibility: Single<&mut Visibility, With<EnergySupplyOverlay>>| { *visibility.into_inner() = Visibility::Inherited; })
            .add_systems(OnExit(EnergySupplyOverlayState::Show), |visibility: Single<&mut Visibility, With<EnergySupplyOverlay>>| { *visibility.into_inner() = Visibility::Hidden; })
            .add_systems(OnExit(UiInteraction::PlaceGridObject), |mut config: ResMut<EnergySupplyOverlayConfig>| { config.secondary_mode = EnergySupplyOverlaySecondaryMode::None; })
            .add_systems(Update, (
                EnergySupplyOverlayConfig::on_config_change_system.run_if(resource_changed::<EnergySupplyOverlayConfig>),
                refresh_display_system.run_if(in_state(EnergySupplyOverlayState::Show)),
                (|mut config: ResMut<EnergySupplyOverlayConfig>| { config.is_overlay_globally_enabled ^= true; }).run_if(input_just_released(KeyCode::Digit7)),
            ))
            .add_observer(EnergySupplyOverlayConfig::on_insert_focused_map_object_set_highlight)
            .add_observer(EnergySupplyOverlayConfig::on_remove_focused_map_object_clear_highlight)
            .add_observer(on_grid_placer_changed_preview_placement);
    }
}

#[derive(States, Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum EnergySupplyOverlayState {
    #[default]
    Hide,
    Show,
}

#[derive(Resource, Default)]
pub struct EnergySupplyOverlayConfig {
    /// Whether the player turned the overlay on. A secondary mode (placement, highlight) shows the
    /// overlay temporarily; when it ends, this decides whether the overlay stays or hides.
    pub is_overlay_globally_enabled: bool,
    pub secondary_mode: EnergySupplyOverlaySecondaryMode,
}
impl EnergySupplyOverlayConfig {
    fn on_config_change_system(
        overlay_config: Res<EnergySupplyOverlayConfig>,
        mut overlay_state: ResMut<NextState<EnergySupplyOverlayState>>,
    ) {
        let shown = overlay_config.is_overlay_globally_enabled || !overlay_config.secondary_mode.is_none();
        overlay_state.set_if_different(if shown { EnergySupplyOverlayState::Show } else { EnergySupplyOverlayState::Hide });
    }
    fn on_insert_focused_map_object_set_highlight(
        trigger: On<Insert<FocusedMapObject>>,
        mut overlay_config: ResMut<EnergySupplyOverlayConfig>,
        buildings: Query<&BuildingType>,
    ) {
        let focused_building = trigger.entity;
        if buildings.contains(focused_building) {
            overlay_config.secondary_mode = EnergySupplyOverlaySecondaryMode::Highlight { building: focused_building };
        } else {
            overlay_config.secondary_mode = EnergySupplyOverlaySecondaryMode::None;
        }
    }
    fn on_remove_focused_map_object_clear_highlight(
        _trigger: On<Remove<FocusedMapObject>>,
        mut overlay_config: ResMut<EnergySupplyOverlayConfig>,
    ) {
        overlay_config.secondary_mode = EnergySupplyOverlaySecondaryMode::None;
    }
}

#[derive(Default, Clone, Debug, PartialEq)]
pub enum EnergySupplyOverlaySecondaryMode {
    #[default]
    None,
    Highlight { building: Entity },
    PlacingSupplier { grid_coords: GridCoords, grid_imprint: GridImprint, range: usize },
    PlacingConsumer { grid_coords: GridCoords, grid_imprint: GridImprint },
}
impl EnergySupplyOverlaySecondaryMode {
    pub fn is_none(&self) -> bool { matches!(self, EnergySupplyOverlaySecondaryMode::None) }
}

#[derive(Asset, AsBindGroup, TypePath, Debug, Clone, Default)]
struct EnergySupplyHeatmapMaterial {
    #[storage(0, read_only)]
    pub energy_cells: Handle<ShaderBuffer>,
}
impl Material2d for EnergySupplyHeatmapMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/overlays/energy_supply_map.wesl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }

    specialize_with_shared_gpu!();
}

#[derive(Component, Default)]
#[require(ZDepth::OverlayEnergySupply)]
pub struct EnergySupplyOverlay {
    /// Grid version and mode used for the last refresh; `None` before the first refresh.
    shown: Option<(GridVersion, EnergySupplyOverlaySecondaryMode)>,
}
impl EnergySupplyOverlay {
    /// Records the requested version and mode. Returns whether the caller needs to refresh the buffer.
    fn record_shown(&mut self, version: GridVersion, mode: &EnergySupplyOverlaySecondaryMode) -> bool {
        if self.shown.as_ref().is_some_and(|(shown_version, shown_mode)| *shown_version == version && shown_mode == mode) { return false; }
        self.shown = Some((version, mode.clone()));
        true
    }

    fn create(
        mut commands: Commands,
        map_info: Res<MapInfo>,
        overlay_state: Res<State<EnergySupplyOverlayState>>,
        mut materials: ResMut<Assets<EnergySupplyHeatmapMaterial>>,
        mut buffers: ResMut<Assets<ShaderBuffer>>,
    ) {
        commands.despawn_all::<With<EnergySupplyOverlay>>();

        // Bind an initialized buffer on the material's spawn frame.
        let energy_cells = buffers.add(ShaderBuffer::from(vec![EnergySupplyCell::none(); map_info.grid_bounds.area()]));
        let material = materials.add(EnergySupplyHeatmapMaterial { energy_cells });
        commands.spawn((
            MapCanvasBundle::new(material, &map_info),
            EnergySupplyOverlay::default(),
            if *overlay_state.get() == EnergySupplyOverlayState::Show { Visibility::Inherited } else { Visibility::Hidden },
        ));
    }
}

fn refresh_display_system(
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    materials: Res<Assets<EnergySupplyHeatmapMaterial>>,
    energy_supply_grid: Res<EnergySupplyGrid>,
    overlay_config: Res<EnergySupplyOverlayConfig>,
    energy_supply_overlay: Single<(&MeshMaterial2d<EnergySupplyHeatmapMaterial>, &mut EnergySupplyOverlay)>,
) {
    let (overlay_material, mut overlay) = energy_supply_overlay.into_inner();
    if !overlay.record_shown(energy_supply_grid.version, &overlay_config.secondary_mode) { return; }

    let overlay_material = materials.get(overlay_material).unwrap();
    let mut buffer = buffers.get_mut(&overlay_material.energy_cells).unwrap();

    let mut overlay_creator = OverlayBufferCreator::new(&energy_supply_grid, &mut buffer);
    match &overlay_config.secondary_mode {
        EnergySupplyOverlaySecondaryMode::None => {
            overlay_creator.generate_buffer_data(&HighlightMode::All)
        }
        EnergySupplyOverlaySecondaryMode::Highlight { building } => {
            overlay_creator.generate_buffer_data(&HighlightMode::Selected(HashSet::from([*building])))
        }
        EnergySupplyOverlaySecondaryMode::PlacingConsumer { grid_coords, grid_imprint } => {
            // Include disabled suppliers so the player can see which disabled ranges would cover the new consumer.
            let suppliers = grid_imprint.iter_in_bounds(*grid_coords, energy_supply_grid.bounds)
                .flat_map(|coords| {
                    let field = &energy_supply_grid[coords];
                    field.suppliers().iter().chain(field.disabled_suppliers().iter())
                })
                .copied()
                .collect();
            overlay_creator.generate_buffer_data(&HighlightMode::Selected(suppliers));
        }
        EnergySupplyOverlaySecondaryMode::PlacingSupplier { grid_coords, grid_imprint, range } => {
            if grid_coords.are_in_bounds(energy_supply_grid.bounds) {
                overlay_creator.flood_potential_energy_supply_to_overlay_heatmap(
                    grid_imprint.iter_in_bounds(*grid_coords, energy_supply_grid.bounds),
                    *range,
                )
            } else {
                overlay_creator.generate_buffer_data(&HighlightMode::All)
            }
        }
    };
}

fn on_grid_placer_changed_preview_placement(
    _trigger: On<GridPlacerChanged>,
    almanach: Res<Almanach>,
    mut overlay_config: ResMut<EnergySupplyOverlayConfig>,
    grid_object_placer: Single<(&GridObjectPlacer, &GridCoords)>,
) {
    let (grid_object_placer, grid_coords) = grid_object_placer.into_inner();
    let map_object = grid_object_placer.map_object();
    if let Some(MapObject::Building(building_type)) = map_object {
        let building_info = almanach.get_building_info(building_type);
        if building_type.is_energy_supplier() {
            overlay_config.secondary_mode = EnergySupplyOverlaySecondaryMode::PlacingSupplier {
                grid_coords: *grid_coords,
                grid_imprint: building_info.grid_imprint,
                range: building_info.baseline[&ModifierType::EnergySupplyRange] as usize,
            };
        } else if building_type.is_energy_consumer() {
            overlay_config.secondary_mode = EnergySupplyOverlaySecondaryMode::PlacingConsumer {
                grid_coords: *grid_coords,
                grid_imprint: building_info.grid_imprint,
            };
        } else {
            overlay_config.secondary_mode = EnergySupplyOverlaySecondaryMode::None;
        }
    } else {
        overlay_config.secondary_mode = EnergySupplyOverlaySecondaryMode::None;
    }
}

/// Field order and types mirror `EnergySupplyCell` in `assets/shaders/overlays/energy_supply_map.wesl`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
struct EnergySupplyCell {
    /// Whether this cell has energy supply (0 = false, 1 = true)
    has_supply: u32,
    /// Whether this cell has power connection (0 = false, 1 = true)
    has_power: u32,
    /// Highlight level: 0 = None, 1 = Dimmed, 2 = Highlighted
    highlight_level: u32,
    /// Whether this cell is covered by at least one intentionally disabled supplier (0 = false, 1 = true)
    has_disabled: u32,
    /// Highlight level of the disabled coverage: 0 = None, 1 = Dimmed, 2 = Highlighted
    disabled_highlight_level: u32,
}
impl EnergySupplyCell {
    /// A cell with no supply and no disabled coverage.
    fn none() -> Self {
        Self {
            has_supply: 0,
            has_power: 0,
            highlight_level: HighlightLevel::None as u32,
            has_disabled: 0,
            disabled_highlight_level: HighlightLevel::None as u32,
        }
    }

    /// Sets the supply fields and keeps the disabled-coverage fields.
    fn set_supply(&mut self, has_power: bool, highlight_level: HighlightLevel) {
        self.has_supply = 1;
        self.has_power = has_power as u32;
        self.highlight_level = highlight_level as u32;
    }
}

/// Per grid cell level of display
#[derive(Debug, Clone, PartialEq)]
enum HighlightLevel {
    /// No supply - completely transparent
    None = 0,
    /// Has supply but another building is highlighted - dimmed display
    Dimmed = 1,
    /// This building's energy supply is highlighted
    Highlighted = 2,
}

/// Global mode of display; decides each cell's `HighlightLevel`.
#[derive(PartialEq)]
enum HighlightMode {
    /// Every cell with supply is highlighted.
    All,
    /// Only cells covered by the selected suppliers are highlighted; other supply is dimmed.
    Selected(HashSet<Entity>),
}

struct OverlayBufferCreator<'a> {
    energy_supply_grid: &'a EnergySupplyGrid,
    buffer: &'a mut ShaderBuffer,
}
impl<'a> OverlayBufferCreator<'a> {
    fn new(energy_supply_grid: &'a EnergySupplyGrid, buffer: &'a mut ShaderBuffer) -> Self {
        Self { energy_supply_grid, buffer }
    }
    /// Rebuilds the whole buffer from the energy supply grid.
    fn generate_buffer_data(&mut self, highlight_mode: &HighlightMode) {
        let energy_supply_grid = self.energy_supply_grid;
        self.buffer.clear();
        self.buffer.extend(energy_supply_grid.grid.iter().map(|grid_field| Self::create_cell_for_grid_field(grid_field, highlight_mode)));
    }

    /// Builds one cell; in `Selected` mode only the selected suppliers' ranges are highlighted.
    fn create_cell_for_grid_field(grid_field: &EnergySupplyField, highlight_mode: &HighlightMode) -> EnergySupplyCell {
        let mut cell = EnergySupplyCell::none();

        if grid_field.has_supply() {
            let highlight_level = match highlight_mode {
                HighlightMode::All => HighlightLevel::Highlighted,
                HighlightMode::Selected(suppliers) if suppliers.iter().any(|supplier| grid_field.has_supplier(*supplier)) => HighlightLevel::Highlighted,
                HighlightMode::Selected(_) => HighlightLevel::Dimmed, // There is supplier, but it's not one of our suppliers.
            };
            cell.set_supply(grid_field.has_power(), highlight_level);
        }

        if grid_field.has_disabled_supply() {
            cell.has_disabled = 1;
            cell.disabled_highlight_level = match highlight_mode {
                HighlightMode::All => HighlightLevel::Highlighted,
                HighlightMode::Selected(suppliers) if suppliers.iter().any(|supplier| grid_field.has_disabled_supplier(*supplier)) => HighlightLevel::Highlighted,
                HighlightMode::Selected(_) => HighlightLevel::Dimmed,
            } as u32;
        }

        cell
    }

    /// Previews a building's potential supply by flooding from its imprint into the overlay buffer.
    /// Updates only the visual buffer; the energy supply grid is unchanged.
    fn flood_potential_energy_supply_to_overlay_heatmap(
        &mut self,
        start_coords: impl IntoIterator<Item = GridCoords>,
        range: usize,
    ) {
        // Collected because this function needs two passes over the start coords.
        let start_coords: Vec<GridCoords> = start_coords.into_iter().collect();

        // Start with base buffer data
        self.generate_buffer_data(&HighlightMode::All);
        let buffer_data = self.buffer.cast_slice_mut::<EnergySupplyCell>().unwrap();
        let grid_bounds = self.energy_supply_grid.bounds;

        VISITED_GRID.with_borrow_mut(|visited_grid| {
            visited_grid.resize_and_clear(grid_bounds);
            let mut queue = VecDeque::new();

            // Flood from every imprint cell, so the distance is even around buildings.
            start_coords.iter().for_each(|coords| {
                let index = self.energy_supply_grid.index(*coords);
                buffer_data[index].set_supply(false, HighlightLevel::Highlighted);
                queue.push_back((0, *coords));
                visited_grid.set_visited(*coords);
            });

            // First flood fill: determine reachability and check for power connection
            let mut has_power = false;
            while let Some((distance, coords)) = queue.pop_front() {
                for new_coords in grid_bounds.cardinal_neighbors(coords) {
                    if visited_grid.is_visited(new_coords) {
                        continue;
                    }
                    visited_grid.set_visited(new_coords);

                    // If we are right at the range boundary - check if that field has power
                    if distance == range {
                        if self.energy_supply_grid[new_coords].has_power() {
                            has_power = true;
                            break;
                        }
                        continue;
                    }

                    let buffer_index = self.energy_supply_grid.index(new_coords);
                    // By default we assume supply but no power.  Don't overwrite if data from the original pass is set.
                    if buffer_data[buffer_index].highlight_level != HighlightLevel::Highlighted as u32 {
                        buffer_data[buffer_index].set_supply(false, HighlightLevel::Highlighted);
                    }

                    let new_distance = distance + 1;
                    if new_distance < range || (new_distance == range && !has_power) {
                        queue.push_back((new_distance, new_coords));
                    }
                }
            }

            // If we found that we have power, we need to do another flood to update the data
            // as the new building being placed may be a bridge between a stranded suppliers and the main power grid
            // Note as this flood is not distance-restricted, as the stranded suppliers may form a chain over entire map and putting new one can connect all of them.
            if has_power {
                visited_grid.clear();
                queue.clear();

                start_coords.iter().for_each(|coords| {
                    let index = self.energy_supply_grid.index(*coords);
                    buffer_data[index].set_supply(true, HighlightLevel::Highlighted);
                    queue.push_back((0, *coords));
                    visited_grid.set_visited(*coords);
                });

                while let Some((_, coords)) = queue.pop_front() {
                    for new_coords in grid_bounds.cardinal_neighbors(coords) {
                        if visited_grid.is_visited(new_coords) {
                            continue;
                        }
                        visited_grid.set_visited(new_coords);

                        let buffer_index = self.energy_supply_grid.index(new_coords);
                        // Only update cells that were marked as highlighted in the previous passes
                        if buffer_data[buffer_index].highlight_level == HighlightLevel::Highlighted as u32 {
                            buffer_data[buffer_index].set_supply(true, HighlightLevel::Highlighted);
                            queue.push_back((0, new_coords));
                        }
                    }
                }
            }
        });
    }
}
