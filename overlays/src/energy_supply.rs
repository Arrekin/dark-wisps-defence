use bevy::{
    input::common_conditions::input_just_released,
    platform::collections::HashSet,
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
use game_core::prelude::{BuildingType, GridCoords, GridImprint, MapObject, ZDepth, Z_OVERLAY_ENERGY_SUPPLY};
use grids::{
    energy_supply::EnergySupplyGrid,
    prelude::{GridObjectPlacer, GridPlacerChanged, GridVersion, MapInfo},
    search::common::{CARDINAL_DIRECTIONS, VISITED_GRID},
};
use hud::prelude::FocusedMapObject;
use states::prelude::{MapLoadingStage, UiInteraction};

pub struct EnergySupplyOverlayPlugin;
impl Plugin for EnergySupplyOverlayPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins(Material2dPlugin::<EnergySupplyHeatmapMaterial>::default())
            .init_state::<EnergySupplyOverlayState>()
            .init_resource::<EnergySupplyOverlayConfig>()
            .add_systems(OnEnter(MapLoadingStage::LoadResources), EnergySupplyOverlay::create)
            .add_systems(OnEnter(EnergySupplyOverlayState::Show), |visiblitiy: Single<&mut Visibility, With<EnergySupplyOverlay>>| { *visiblitiy.into_inner() = Visibility::Inherited; })
            .add_systems(OnExit(EnergySupplyOverlayState::Show), |visiblitiy: Single<&mut Visibility, With<EnergySupplyOverlay>>| { *visiblitiy.into_inner() = Visibility::Hidden; })
            .add_systems(OnExit(UiInteraction::PlaceGridObject), |mut config: ResMut<EnergySupplyOverlayConfig>| { config.secondary_mode = EnergySupplyOverlaySecondaryMode::None; })
            .add_systems(Update, (
                EnergySupplyOverlayConfig::on_config_change_system.run_if(resource_changed::<EnergySupplyOverlayConfig>),
                refresh_display_system.run_if(in_state(EnergySupplyOverlayState::Show)),
                (|mut config: ResMut<EnergySupplyOverlayConfig>| { config.is_overlay_globally_enabled ^= true; }).run_if(input_just_released(KeyCode::Digit7)), // Switch overlay on/off 
            ))
            .add_observer(EnergySupplyOverlayConfig::on_map_object_focused)
            .add_observer(EnergySupplyOverlayConfig::on_map_object_unfocused)
            .add_observer(on_grid_placer_changed)
            ;

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
    // Determines whether the energy supply overlay is globally enabled. We need that information for example when we are in building placing mode,
    // showing the highlihted building, and then we need to disable the highligh. We have to either hide the overlay or change it to `All` depending on the state
    // before the placing had started.
    pub is_overlay_globally_enabled: bool,
    pub grid_version: GridVersion, // Grid version for which we show the overlay
    pub secondary_mode: EnergySupplyOverlaySecondaryMode,
}
impl EnergySupplyOverlayConfig {
    fn on_config_change_system(
        overlay_config: Res<EnergySupplyOverlayConfig>,
        mut overlay_state: ResMut<NextState<EnergySupplyOverlayState>>,
    ) {
        if overlay_config.is_overlay_globally_enabled || !overlay_config.secondary_mode.is_none() {
            overlay_state.set(EnergySupplyOverlayState::Show);
        } else {
            overlay_state.set(EnergySupplyOverlayState::Hide);
        }
    }
    fn on_map_object_focused(
        trigger: On<Insert, FocusedMapObject>,
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
    fn on_map_object_unfocused(
        _trigger: On<Remove, FocusedMapObject>,
        mut overlay_config: ResMut<EnergySupplyOverlayConfig>,
    ) {
        overlay_config.secondary_mode = EnergySupplyOverlaySecondaryMode::None;
    }
}
#[derive(Default, Clone, Debug, PartialEq)]
pub enum EnergySupplyOverlaySecondaryMode {
    #[default]
    None,
    Highlight{ building: Entity },
    PlacingSupplier{grid_coords: GridCoords, grid_imprint: GridImprint, range: usize},
    PlacingConsumer{grid_coords: GridCoords, grid_imprint: GridImprint},
}
impl EnergySupplyOverlaySecondaryMode {
    pub fn is_none(&self) -> bool { matches!(self, EnergySupplyOverlaySecondaryMode::None) }
}

#[derive(Asset, AsBindGroup, TypePath, Debug, Clone, Default)]
struct EnergySupplyHeatmapMaterial {
    #[storage(0, read_only)]
    pub energy_cells: Handle<ShaderBuffer>,
    #[uniform(1)]
    pub grid_data: super::UniformGridData,
}
impl Material2d for EnergySupplyHeatmapMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/energy_supply_map.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

#[derive(Component)]
pub struct EnergySupplyOverlay;
impl EnergySupplyOverlay {
    fn create(
        mut commands: Commands,
        map_info: Res<MapInfo>,
        mut meshes: ResMut<Assets<Mesh>>,
        mut materials: ResMut<Assets<EnergySupplyHeatmapMaterial>>,
        overlay: Query<Entity, With<EnergySupplyOverlay>>,
    ) {
        if let Ok(overlay_entity) = overlay.single() {
            commands.entity(overlay_entity).despawn();
        };

        commands.spawn((
            super::overlay_bundle(&mut meshes, &mut materials, &map_info),
            EnergySupplyOverlay,
            ZDepth(Z_OVERLAY_ENERGY_SUPPLY),
        ));
    }
}

fn refresh_display_system(
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    mut materials: ResMut<Assets<EnergySupplyHeatmapMaterial>>,
    energy_supply_grid: Res<EnergySupplyGrid>,
    mut overlay_config: ResMut<EnergySupplyOverlayConfig>,
    energy_supply_overlay: Single<&MeshMaterial2d<EnergySupplyHeatmapMaterial>, With<EnergySupplyOverlay>>,
    mut last_secondary_mode: Local<EnergySupplyOverlaySecondaryMode>,
    mut local_buffer_data: Local<Vec<EnergySupplyCell>>, // To avoid re-allocations every frame
) {
    if overlay_config.grid_version == energy_supply_grid.version && overlay_config.secondary_mode == *last_secondary_mode { return; }
    
    *last_secondary_mode = overlay_config.secondary_mode.clone();
    overlay_config.grid_version = energy_supply_grid.version;

    let mut overlay_material = materials.get_mut(energy_supply_overlay.into_inner()).unwrap();
    
    // Generate buffer data
    let mut overlay_creator = OverlayBufferCreator::new(&energy_supply_grid, &mut local_buffer_data);
    match &overlay_config.secondary_mode {
        EnergySupplyOverlaySecondaryMode::None => {
            overlay_creator.generate_buffer_data(&HighlightMode::All)
        }
        EnergySupplyOverlaySecondaryMode::Highlight{ building } => {
            overlay_creator.generate_buffer_data(&HighlightMode::Selected(vec![*building]))
        }
        EnergySupplyOverlaySecondaryMode::PlacingConsumer{grid_coords, grid_imprint} => {
            let suppliers = grid_imprint.iter_in_bounds(*grid_coords, energy_supply_grid.bounds())
                .flat_map(|coords| energy_supply_grid[coords].suppliers())
                .collect::<HashSet<_>>()
                .into_iter()
                .cloned()
                .collect::<Vec<_>>();
            overlay_creator.generate_buffer_data(&HighlightMode::Selected(suppliers));
        }
        EnergySupplyOverlaySecondaryMode::PlacingSupplier{grid_coords, grid_imprint, range} => {
            if grid_coords.is_in_bounds(energy_supply_grid.bounds()) {
                overlay_creator.flood_potential_energy_supply_to_overlay_heatmap(
                    grid_imprint.iter_in_bounds(*grid_coords, energy_supply_grid.bounds()),
                    *range,
                )
            } else {
                overlay_creator.generate_buffer_data(&HighlightMode::All)
            }
        }
    };
    let buffer_handle = &overlay_material.energy_cells;
    if let Some(mut buffer) = buffers.get_mut(buffer_handle) {
        buffer.set_data(&*local_buffer_data);
    } else {
        // Create ShaderBuffer
        let storage_buffer = ShaderBuffer::from(local_buffer_data.as_slice());
        let buffer_handle = buffers.add(storage_buffer);
        overlay_material.energy_cells = buffer_handle;
    }
    
    // Update uniforms
    let bounds = energy_supply_grid.bounds();
    overlay_material.grid_data.grid_width = bounds.0 as u32;
    overlay_material.grid_data.grid_height = bounds.1 as u32;
}

fn on_grid_placer_changed(
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
            overlay_config.secondary_mode = EnergySupplyOverlaySecondaryMode::PlacingSupplier{
                grid_coords: *grid_coords,
                grid_imprint: building_info.grid_imprint,
                range: building_info.baseline[&ModifierType::EnergySupplyRange] as usize,
            };
        }
        else if building_type.is_energy_consumer() {
            overlay_config.secondary_mode = EnergySupplyOverlaySecondaryMode::PlacingConsumer{
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

/// Energy supply cell data for GPU buffer
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable, ShaderType)]
struct EnergySupplyCell {
    /// Whether this cell has energy supply (0 = false, 1 = true)
    has_supply: u32,
    /// Whether this cell has power connection (0 = false, 1 = true) 
    has_power: u32,
    /// Highlight level: 0 = None, 1 = Dimmed, 2 = Highlighted
    highlight_level: u32,
}
impl EnergySupplyCell {
    /// Create a cell representing no energy supply
    fn none() -> Self {
        Self {
            has_supply: 0,
            has_power: 0,
            highlight_level: HighlightLevel::None as u32,
        }
    }

    /// Create a cell with supply and power status
    fn with_supply(has_power: bool, highlight_level: HighlightLevel) -> Self {
        Self {
            has_supply: 1,
            has_power: has_power as u32,
            highlight_level: highlight_level as u32,
        }
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

// Global mode of display. Dictates the way HighlightLevel is set
#[derive(PartialEq)]
enum HighlightMode {
    All, // Every cell with supply is highlighted
    Selected(Vec<Entity>), // Only cells covered by the selected suppliers are highlighted
}

pub struct OverlayBufferCreator<'a> {
    energy_supply_grid: &'a EnergySupplyGrid,
    local_buffer_data: Option<&'a mut Vec<EnergySupplyCell>>,
}
impl<'a> OverlayBufferCreator<'a> {
    fn new(energy_supply_grid: &'a EnergySupplyGrid, local_buffer_data: &'a mut Vec<EnergySupplyCell>) -> Self {
        Self { energy_supply_grid, local_buffer_data: Some(local_buffer_data) }
    }
    /// Generate buffer data for the entire energy supply grid for GPU usage
    fn generate_buffer_data(&mut self, highlight_mode: &HighlightMode) {
        let buffer_data = self.local_buffer_data.take().unwrap(); // To avoid double mutable borrows on the struct's fields
        buffer_data.clear();
        let buffer_size = self.energy_supply_grid.grid.len();
        let new_content = (0..buffer_size).map(|idx| self.create_cell_for_grid_field(idx, highlight_mode));
        buffer_data.extend(new_content);
        self.local_buffer_data = Some(buffer_data);
    }
    
    /// If `highlight_supplier` is provided, only its range will be shown at full color, other ranges will be dimmed
    fn create_cell_for_grid_field(&self, idx: usize, highlight_mode: &HighlightMode) -> EnergySupplyCell {
        let grid_field = &self.energy_supply_grid.grid[idx];
        
        if !grid_field.has_supply() {
            return EnergySupplyCell::none();
        }
        
        let highlight_level = match highlight_mode {
            HighlightMode::All => HighlightLevel::Highlighted,
            HighlightMode::Selected(suppliers) if suppliers.iter().any(|supplier| grid_field.has_supplier(*supplier)) => HighlightLevel::Highlighted,
            HighlightMode::Selected(_) => HighlightLevel::Dimmed, // There is supplier, but it's not one of our suppliers.
        };
        
        EnergySupplyCell::with_supply(grid_field.has_power(), highlight_level)
    }
    
    /// Special version of `flooding::flood_energy_supply` to add the energy supply of a building we are currently placing to the overlay heatmap.
    /// It writes directly to the overlay texture, so it's only a visual cue that does not affect the actual grid.
    fn flood_potential_energy_supply_to_overlay_heatmap(
        &mut self,
        start_coords: impl IntoIterator<Item = GridCoords>,
        range: usize,
    ) {
        use std::collections::VecDeque;
        // Collected because this function needs two passes over the start coords.
        let start_coords: Vec<GridCoords> = start_coords.into_iter().collect();

        // Start with base buffer data
        self.generate_buffer_data(&HighlightMode::All);
        let buffer_data = self.local_buffer_data.take().unwrap();
        let bounds = self.energy_supply_grid.bounds();

        VISITED_GRID.with_borrow_mut(|visited_grid| {
            visited_grid.resize_and_reset(bounds);
            let mut queue = VecDeque::new();

            // Start Flood from all fields to ensure event distance from buildings that are bigger than one cell
            start_coords.iter().for_each(|coords| {
                let index = self.energy_supply_grid.index(*coords);
                buffer_data[index] = EnergySupplyCell::with_supply(false, HighlightLevel::Highlighted);
                queue.push_back((0, *coords));
                visited_grid.set_visited(*coords);
            });
            
            // First flood fill: determine reachability and check for power connection
            let mut has_power = false;
            while let Some((distance, coords)) = queue.pop_front() {
                for (delta_x, delta_y) in CARDINAL_DIRECTIONS {
                    let new_coords = coords.shifted((delta_x, delta_y));
                    if !new_coords.is_in_bounds(bounds) || visited_grid.is_visited(new_coords) {
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
                    if buffer_data[buffer_index].highlight_level != 2 {
                        buffer_data[buffer_index] = EnergySupplyCell::with_supply(false, HighlightLevel::Highlighted);
                    }
                    
                    let new_distance = distance + 1;
                    if new_distance < range || (new_distance == range && !has_power) {
                        queue.push_back((new_distance, new_coords));
                    }
                }
            }
            
            // If we found that we have power, we need to do another flood to update the data
            // as the new building being placed may be a bridge between a stranded suppliers and the main power grid
            // Note as this flood is not distance-restriced as the stranded suppliers may form a chain over entire map and putting new one can connect all of them.
            if has_power {
                visited_grid.reset();
                queue.clear();
                
                start_coords.iter().for_each(|coords| {
                    let index = self.energy_supply_grid.index(*coords);
                    buffer_data[index] = EnergySupplyCell::with_supply(true, HighlightLevel::Highlighted);
                    queue.push_back((0, *coords));
                    visited_grid.set_visited(*coords);
                });

                while let Some((_, coords)) = queue.pop_front() {
                    for (delta_x, delta_y) in CARDINAL_DIRECTIONS {
                        let new_coords = coords.shifted((delta_x, delta_y));
                        if !new_coords.is_in_bounds(bounds) || visited_grid.is_visited(new_coords) {
                            continue;
                        }
                        visited_grid.set_visited(new_coords);
                        
                        let buffer_index = self.energy_supply_grid.index(new_coords);
                        // Only update cells that were marked as highlighted in the previous passes
                        if buffer_data[buffer_index].highlight_level == 2 {
                            buffer_data[buffer_index] = EnergySupplyCell::with_supply(true, HighlightLevel::Highlighted);
                            queue.push_back((0, new_coords));
                        }
                    }
                }
            }
        });
        
        self.local_buffer_data = Some(buffer_data);
    }
}
