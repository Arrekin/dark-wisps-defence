//! # Tower Range Overlay
//!
//! Uploads cell coverage and circular boundaries from [`TowerRangeIndex`] for the range shader.
//! Index versions control buffer refreshes. A separate uniform describes the highlight a caller
//! wants, so changing the highlight does not require rebuilding the coverage buffers.

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

use game_core::prelude::{AreaRange, CELL_SIZE, DrawLevel, GridCoords, GridImprint, MapInfo, RangeType, ZDepth};
use grids::prelude::GridVersion;
use indexes::tower_ranges::TowerRangeIndex;
use overlays::towers_range::{TowersRangeOverlayConfig, TowersRangeOverlaySecondaryMode};
use states::prelude::MapLoadingStage;
use visuals::{canvas::MapCanvasBundle, specialize_with_shared_gpu};

pub(crate) struct TowersRangeOverlayPlugin;
impl Plugin for TowersRangeOverlayPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins(Material2dPlugin::<TowersRangeMaterial>::default())
            .init_state::<TowersRangeOverlayState>()
            .init_resource::<TowersRangeOverlayConfig>()
            .add_systems(OnEnter(MapLoadingStage::LoadResources), TowersRangeOverlay::create)
            .add_systems(OnEnter(TowersRangeOverlayState::Show), |visibility: Single<&mut Visibility, With<TowersRangeOverlay>>| { *visibility.into_inner() = Visibility::Inherited; })
            .add_systems(OnExit(TowersRangeOverlayState::Show), |visibility: Single<&mut Visibility, With<TowersRangeOverlay>>| { *visibility.into_inner() = Visibility::Hidden; })
            .add_systems(Update, (
                apply_config_change.run_if(resource_changed::<TowersRangeOverlayConfig>),
                refresh_display.run_if(in_state(TowersRangeOverlayState::Show)),
                (|mut config: ResMut<TowersRangeOverlayConfig>| { config.is_overlay_globally_enabled ^= true; }).run_if(input_just_released(KeyCode::Digit8)),
            ));
    }
}

/// Outline width in cells, used as the margin when finding circle edges.
/// Must match `OUTLINE_RATIO` in `assets/shaders/overlays/towers_ranges_map.wesl`.
const OUTLINE_RATIO: f32 = 2.0 / 16.0;

#[derive(States, Debug, Clone, PartialEq, Eq, Hash, Default)]
enum TowersRangeOverlayState {
    #[default]
    Hide,
    Show,
}

/// Shows the overlay while the player enabled it or a caller wants it.
fn apply_config_change(
    overlay_config: Res<TowersRangeOverlayConfig>,
    mut overlay_state: ResMut<NextState<TowersRangeOverlayState>>,
) {
    let shown = overlay_config.is_overlay_globally_enabled || overlay_config.secondary_mode.is_some();
    overlay_state.set_if_different(if shown { TowersRangeOverlayState::Show } else { TowersRangeOverlayState::Hide });
}

#[derive(Asset, AsBindGroup, TypePath, Debug, Clone, Default)]
struct TowersRangeMaterial {
    #[storage(0, read_only)]
    cells: Handle<ShaderBuffer>,
    /// Circle data keyed by tower range slot.
    #[storage(1, read_only)]
    circles: Handle<ShaderBuffer>,
    /// Circle slot indexes grouped by cell, addressed by each cell's start offset and count.
    #[storage(2, read_only)]
    circle_indexes: Handle<ShaderBuffer>,
    #[uniform(3)]
    highlight: TowerRangeHighlight,
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
#[require(ZDepth::OverlayTowerRanges, DrawLevel::OnGround)]
struct TowersRangeOverlay {
    /// Index version used to populate the buffers; `None` before the first refresh.
    shown_version: Option<GridVersion>,
    /// Mode used to populate the highlight uniform. The first refresh always populates it, because
    /// `shown_version` starts empty.
    shown_mode: Option<TowersRangeOverlaySecondaryMode>,
    /// Staging buffer for circle slot indexes. Retained between refreshes to reuse the allocation.
    circle_indexes: Vec<u32>,
}
impl TowersRangeOverlay {
    /// Records the index version and returns whether the buffers need a refresh.
    fn record_version(&mut self, version: GridVersion) -> bool {
        if self.shown_version == Some(version) { return false; }
        self.shown_version = Some(version);
        true
    }

    /// Records the mode and returns whether the highlight needs a refresh.
    fn record_mode(&mut self, mode: &Option<TowersRangeOverlaySecondaryMode>) -> bool {
        if self.shown_mode == *mode { return false; }
        self.shown_mode = mode.clone();
        true
    }

    /// Writes cell coverage to the GPU buffer and stages each cell's circle slot indexes.
    fn stage_cells(&mut self, index: &TowerRangeIndex, cells: &mut ShaderBuffer) {
        self.circle_indexes.clear();
        cells.clear();
        cells.extend(index.bounds().iter().map(|coords| Self::create_cell(index, coords, &mut self.circle_indexes)));
        // Storage buffers cannot be empty.
        if self.circle_indexes.is_empty() { self.circle_indexes.push(0); }
    }

    /// Writes circle data at each tower's slot index, leaving unused entries zeroed.
    fn write_circles(index: &TowerRangeIndex, circles: &mut ShaderBuffer) {
        circles.clear();
        // Keep one unused entry when the index is empty: storage buffers require nonzero size.
        circles.extend(std::iter::repeat_n(TowerRangeCircle::default(), index.slot_count().max(1)));
        let table = circles.cast_slice_mut::<TowerRangeCircle>().unwrap();
        for indexed in index.ranges().filter(|indexed| indexed.range.range_type == RangeType::Circle) {
            table[indexed.slot] = indexed.range.into();
        }
    }

    /// Summarizes cell range coverage and appends circle slots with an outline near this cell.
    fn create_cell(index: &TowerRangeIndex, coords: GridCoords, circle_indexes: &mut Vec<u32>) -> TowerRangeCell {
        let mut cell = TowerRangeCell { circle_start: circle_indexes.len() as u32, ..default() };
        for indexed in index.ranges_reaching(coords) {
            match indexed.range.range_type {
                RangeType::Cells => {
                    cell.signature ^= indexed.entity.index_u32();
                    cell.cover_count += 1;
                }
                RangeType::Circle => {
                    if indexed.range.crosses_edge(coords, OUTLINE_RATIO) { circle_indexes.push(indexed.slot.index); }
                }
            }
        }
        cell.circle_count = circle_indexes.len() as u32 - cell.circle_start;
        cell
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
        // Storage buffers cannot be empty, so these start with one unused row.
        let circles = buffers.add(ShaderBuffer::from(vec![TowerRangeCircle::default()]));
        let circle_indexes = buffers.add(ShaderBuffer::from(vec![0u32]));
        let material = materials.add(TowersRangeMaterial { cells, circles, circle_indexes, highlight: default() });
        commands.spawn((
            MapCanvasBundle::new(material, &map_info),
            TowersRangeOverlay::default(),
            if *overlay_state.get() == TowersRangeOverlayState::Show { Visibility::Inherited } else { Visibility::Hidden },
        ));
    }
}

fn refresh_display(
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    mut materials: ResMut<Assets<TowersRangeMaterial>>,
    tower_range_index: Res<TowerRangeIndex>,
    overlay_config: Res<TowersRangeOverlayConfig>,
    towers_range_overlay: Single<(&MeshMaterial2d<TowersRangeMaterial>, &mut TowersRangeOverlay)>,
) {
    let (overlay_material, mut overlay) = towers_range_overlay.into_inner();
    let version_changed = overlay.record_version(tower_range_index.version());
    let mode_changed = overlay.record_mode(&overlay_config.secondary_mode);

    // Refresh after index changes even when the same tower remains highlighted.
    if version_changed || mode_changed {
        let highlighted_range = match &overlay_config.secondary_mode {
            None => None,
            Some(TowersRangeOverlaySecondaryMode::HighlightTower { tower }) => tower_range_index.get(*tower).map(|indexed| indexed.range),
            Some(TowersRangeOverlaySecondaryMode::HighlightRange { range }) => Some(*range),
        };
        materials.get_mut(overlay_material).unwrap().highlight = highlighted_range.map(TowerRangeHighlight::from).unwrap_or_default();
    }
    if !version_changed { return; }

    let material = materials.get(overlay_material).unwrap();
    overlay.stage_cells(&tower_range_index, &mut buffers.get_mut(&material.cells).unwrap());
    TowersRangeOverlay::write_circles(&tower_range_index, &mut buffers.get_mut(&material.circles).unwrap());
    let mut circle_indexes = buffers.get_mut(&material.circle_indexes).unwrap();
    circle_indexes.clear();
    circle_indexes.extend_from_slice(&overlay.circle_indexes);
}

/// Cell coverage and circle list offsets for the GPU.
/// Field order and types must match `TowerRangeCell` in `assets/shaders/overlays/towers_ranges_map.wesl`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable, Default)]
struct TowerRangeCell {
    /// XOR of the entity indexes of towers whose cell ranges cover this cell.
    signature: u32,
    /// Number of cell ranges covering this cell.
    cover_count: u32,
    /// Offset of this cell's first entry in the circle indexes buffer.
    circle_start: u32,
    /// Number of circle slot indexes stored for this cell.
    circle_count: u32,
}

/// Circular range data in world units. Free slots and cell ranges use zeroed entries.
/// Field order and types must match `TowerRangeCircle` in `assets/shaders/overlays/towers_ranges_map.wesl`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable, Default)]
struct TowerRangeCircle {
    center: Vec2,
    min_radius: f32,
    max_radius: f32,
}
impl From<AreaRange> for TowerRangeCircle {
    fn from(range: AreaRange) -> Self {
        Self {
            center: range.area.world_center(),
            min_radius: range.min * CELL_SIZE,
            max_radius: range.max * CELL_SIZE,
        }
    }
}

/// Uniform describing the highlighted range. The default disables highlighting.
/// Field order and types must match `TowerRangeHighlight` in `assets/shaders/overlays/towers_ranges_map.wesl`.
#[derive(ShaderType, Clone, Copy, Debug, Default)]
struct TowerRangeHighlight {
    /// 0: no highlight, 1: cell range, 2: circle. Must match `HIGHLIGHT_*` in the shader.
    kind: u32,
    /// 1 for a plus imprint, 0 for a rectangle.
    is_plus: u32,
    /// Bottom-left cell of the imprint's bounding box.
    origin: IVec2,
    /// Rectangle dimensions in cells, or plus arm extents repeated in both components.
    size: IVec2,
    /// Centre of the range in world units.
    center: Vec2,
    /// Inclusive minimum distance in cells.
    min: f32,
    /// Inclusive maximum distance in cells.
    max: f32,
}
impl From<AreaRange> for TowerRangeHighlight {
    fn from(range: AreaRange) -> Self {
        let (is_plus, size) = match range.area.imprint {
            GridImprint::Rectangle { width, height } => (0, IVec2::new(width, height)),
            GridImprint::Plus { extents } => (1, IVec2::splat(extents)),
        };
        Self {
            kind: match range.range_type { RangeType::Cells => 1, RangeType::Circle => 2 },
            is_plus,
            origin: range.area.origin.coords.into(),
            size,
            center: range.area.world_center(),
            min: range.min,
            max: range.max,
        }
    }
}
