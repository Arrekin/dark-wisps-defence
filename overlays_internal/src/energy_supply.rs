//! # Energy supply overlay
//!
//! Draws the energy supply grid, or its preview while one exists, with the highlight a caller wants.

use bevy::{
    input::common_conditions::input_just_released,
    prelude::*,
    reflect::TypePath,
    render::{
        render_resource::AsBindGroup,
        storage::ShaderBuffer,
    },
    shader::ShaderRef,
    sprite_render::{AlphaMode2d, Material2d, Material2dPlugin, MeshMaterial2d},
};
use smallvec::{SmallVec, smallvec};

use game_core::prelude::{DrawLevel, MapInfo, ZDepth};
use grids::prelude::GridVersion;
use indexes::energy_supply::{self, EnergySupply};
use overlays::energy_supply::{EnergySupplyOverlayConfig, EnergySupplyOverlaySecondaryMode};
use states::prelude::MapLoadingStage;
use visuals::{canvas::MapCanvasBundle, specialize_with_shared_gpu};

pub(crate) struct EnergySupplyOverlayPlugin;
impl Plugin for EnergySupplyOverlayPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins(Material2dPlugin::<EnergySupplyHeatmapMaterial>::default())
            .init_state::<EnergySupplyOverlayState>()
            .init_resource::<EnergySupplyOverlayConfig>()
            .add_systems(OnEnter(MapLoadingStage::LoadResources), EnergySupplyOverlay::create)
            .add_systems(OnEnter(EnergySupplyOverlayState::Show), |visibility: Single<&mut Visibility, With<EnergySupplyOverlay>>| { *visibility.into_inner() = Visibility::Inherited; })
            .add_systems(OnExit(EnergySupplyOverlayState::Show), |visibility: Single<&mut Visibility, With<EnergySupplyOverlay>>| { *visibility.into_inner() = Visibility::Hidden; })
            .add_systems(Update, (
                apply_config_change.run_if(resource_changed::<EnergySupplyOverlayConfig>),
                refresh_display.run_if(in_state(EnergySupplyOverlayState::Show)),
                (|mut config: ResMut<EnergySupplyOverlayConfig>| { config.is_overlay_globally_enabled ^= true; }).run_if(input_just_released(KeyCode::Digit7)),
            ));
    }
}

#[derive(States, Debug, Clone, PartialEq, Eq, Hash, Default)]
enum EnergySupplyOverlayState {
    #[default]
    Hide,
    Show,
}

/// Shows the overlay while the player enabled it or a caller wants it.
fn apply_config_change(
    overlay_config: Res<EnergySupplyOverlayConfig>,
    mut overlay_state: ResMut<NextState<EnergySupplyOverlayState>>,
) {
    let shown = overlay_config.is_overlay_globally_enabled || overlay_config.secondary_mode.is_some();
    overlay_state.set_if_different(if shown { EnergySupplyOverlayState::Show } else { EnergySupplyOverlayState::Hide });
}

#[derive(Asset, AsBindGroup, TypePath, Debug, Clone, Default)]
struct EnergySupplyHeatmapMaterial {
    #[storage(0, read_only)]
    energy_cells: Handle<ShaderBuffer>,
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

/// What the overlay last drew.
#[derive(PartialEq)]
struct ShownState {
    is_preview: bool,
    version: GridVersion,
    mode: Option<EnergySupplyOverlaySecondaryMode>,
}

#[derive(Component, Default)]
#[require(ZDepth::OverlayEnergySupply, DrawLevel::Elevation)]
struct EnergySupplyOverlay {
    /// `None` before the first refresh.
    shown: Option<ShownState>,
}
impl EnergySupplyOverlay {
    /// Records what is about to be drawn. Returns whether it differs from the last drawing.
    fn record_shown(&mut self, shown: ShownState) -> bool {
        if self.shown.as_ref() == Some(&shown) { return false; }
        self.shown = Some(shown);
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

/// Draws the preview while one exists, otherwise the real grid, with the wanted highlight.
fn refresh_display(
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    materials: Res<Assets<EnergySupplyHeatmapMaterial>>,
    energy_supply: Res<EnergySupply>,
    overlay_config: Res<EnergySupplyOverlayConfig>,
    energy_supply_overlay: Single<(&MeshMaterial2d<EnergySupplyHeatmapMaterial>, &mut EnergySupplyOverlay)>,
) {
    let (overlay_material, mut overlay) = energy_supply_overlay.into_inner();
    let preview = energy_supply.preview();
    let grid = preview.unwrap_or(&energy_supply.grid);
    let shown = ShownState { is_preview: preview.is_some(), version: grid.version, mode: overlay_config.secondary_mode.clone() };
    if !overlay.record_shown(shown) { return; }

    let highlight_mode = match &overlay_config.secondary_mode {
        None | Some(EnergySupplyOverlaySecondaryMode::HighlightAll) => HighlightMode::All,
        Some(EnergySupplyOverlaySecondaryMode::HighlightNone) => HighlightMode::Selected(SmallVec::new()),
        Some(EnergySupplyOverlaySecondaryMode::HighlightSupplier { supplier }) => HighlightMode::Selected(smallvec![*supplier]),
        Some(EnergySupplyOverlaySecondaryMode::HighlightAreaSuppliers { area }) => {
            // Include disabled suppliers so the player can see which disabled ranges cover the area.
            let mut suppliers = SmallVec::new();
            for &supplier in grid.cells_in(*area).flat_map(|cell| cell.suppliers().iter().chain(cell.disabled_suppliers().iter())) {
                if !suppliers.contains(&supplier) {
                    suppliers.push(supplier);
                }
            }
            HighlightMode::Selected(suppliers)
        }
    };

    let Some(overlay_material) = materials.get(overlay_material) else { return; };
    let Some(mut buffer) = buffers.get_mut(&overlay_material.energy_cells) else { return; };
    buffer.clear();
    buffer.extend(grid.grid.iter().map(|grid_cell| EnergySupplyCell::from_grid_cell(grid_cell, &highlight_mode)));
}

/// Field order and types mirror `EnergySupplyCell` in `assets/shaders/overlays/energy_supply_map.wesl`.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
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

    /// Builds one cell; in `Selected` mode only the selected suppliers' ranges are highlighted.
    fn from_grid_cell(grid_cell: &energy_supply::EnergySupplyCell, highlight_mode: &HighlightMode) -> Self {
        let mut cell = Self::none();

        if grid_cell.has_supply() {
            cell.has_supply = 1;
            cell.has_power = grid_cell.has_power() as u32;
            cell.highlight_level = highlight_mode.level_of(grid_cell.suppliers()) as u32;
        }

        if grid_cell.has_disabled_supply() {
            cell.has_disabled = 1;
            cell.disabled_highlight_level = highlight_mode.level_of(grid_cell.disabled_suppliers()) as u32;
        }

        cell
    }
}

/// Per grid cell level of display
enum HighlightLevel {
    /// No supply - completely transparent
    None = 0,
    /// Supply that is not highlighted - dimmed display
    Dimmed = 1,
    /// Highlighted supply
    Highlighted = 2,
}

/// Global mode of display; decides each cell's `HighlightLevel`.
enum HighlightMode {
    /// Every cell with supply is highlighted.
    All,
    /// Only cells covered by the selected suppliers are highlighted; other supply is dimmed.
    /// Few suppliers are selected, so the list typically stays inline.
    Selected(SmallVec<[Entity; 4]>),
}
impl HighlightMode {
    /// The level of supply whose suppliers are `listed`. Supply none of whose suppliers is
    /// selected is dimmed.
    fn level_of(&self, listed: &[Entity]) -> HighlightLevel {
        match self {
            Self::All => HighlightLevel::Highlighted,
            Self::Selected(selected) if selected.iter().any(|supplier| listed.contains(supplier)) => HighlightLevel::Highlighted,
            Self::Selected(_) => HighlightLevel::Dimmed,
        }
    }
}
