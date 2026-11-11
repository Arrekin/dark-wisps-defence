//! # Obstacle grid
//!
//! Each cell stores one [`GridStructure`] per volume. Underground cells default to `Solid`;
//! the two volumes above them default to `Empty`. Walls, buildings and ore keep frequently queried
//! data inline and use typed slots to reference their entity, area and additional data.
//!
//! All cells occupied by the same entity reference one slot. Imprint methods create the entry
//! and fill its cells; [`ObstacleGrid::deprint`] clears the cells and frees the slot.
//!
//! Excavations store their state directly in the underground volume: `Empty` for a pit, or
//! `Excavation` with an inline shape for a ramp or tunnel. [`ObstacleGrid::excavate`] and
//! [`ObstacleGrid::fill_excavation`] update one cell at a time.

use std::ops::Deref;

use bevy::prelude::*;

use game_core::prelude::{Bounds, BuildingType, EdgeSide, EdgeSides, FloorLevel, GridArea, GridCoords, GridVolumePosition, VolumeLevel};
use grids::{GridVersion, base::{BaseGrid, GridCell}, slot_index::{Slot, Slots}};
use map_objects::{prelude::{ExcavationShape, VolumeShape}, wall_style::WallStyleKey};

/// Contents of one cell volume, including empty space and solid earth.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GridStructure {
    /// Open space. The default at `Elevation` and `OnElevation`; represents a pit at `Underground`.
    Empty,
    /// Packed earth, the default at `Underground`.
    Solid,
    DarkOre { slot: Slot<DarkOreData> },
    Wall { slot: Slot<WallData>, style: WallStyleKey, shape: VolumeShape },
    Building { slot: Slot<BuildingData>, building_type: BuildingType },
    /// An underground ramp or tunnel that supports the floor above.
    ///
    /// A flat shape without openings blocks movement like solid earth.
    Excavation { shape: VolumeShape },
}

const _: () = assert!(size_of::<GridStructure>() == 8, "GridStructure sits in every cell volume and is read in hot loops");

impl GridStructure {
    /// GPU code matching `STRUCTURE_*` in `assets/shaders/grids/obstacles.wesl`.
    pub const fn gpu_index(self) -> u32 {
        match self {
            Self::Empty => 0,
            Self::Solid => 1,
            Self::DarkOre { .. } => 2,
            Self::Wall { .. } => 3,
            Self::Building { .. } => 4,
            Self::Excavation { .. } => 5,
        }
    }

    /// Default cell contents: solid earth underground, empty space above.
    pub const fn default_at(level: VolumeLevel) -> Self {
        match level {
            VolumeLevel::Underground => Self::Solid,
            VolumeLevel::Elevation | VolumeLevel::OnElevation => Self::Empty,
        }
    }

    pub const fn is_empty(self) -> bool {
        matches!(self, Self::Empty)
    }

    pub const fn is_building(self) -> bool {
        matches!(self, Self::Building { .. })
    }

    /// True for a wall ramp or an excavated ramp.
    pub const fn is_ramp(self) -> bool {
        self.ramp_high_side().is_some()
    }

    /// The side a ramp rises toward, or `None` for structures that are not ramps.
    pub const fn ramp_high_side(self) -> Option<EdgeSide> {
        match self.shape() {
            Some(VolumeShape::Ramp { high_side }) => Some(high_side),
            _ => None,
        }
    }

    /// Tunnel openings for a wall or excavation; an empty set for other structures.
    pub const fn openings(self) -> EdgeSides {
        match self.shape() {
            Some(shape) => shape.openings(),
            None => EdgeSides::NONE,
        }
    }

    /// Returns the shape of a wall or excavation, or `None` for other structures.
    pub const fn shape(self) -> Option<VolumeShape> {
        match self {
            Self::Wall { shape, .. } | Self::Excavation { shape } => Some(shape),
            _ => None,
        }
    }

    /// Returns the style of a wall, or `None` for other structures.
    pub const fn style(self) -> Option<WallStyleKey> {
        match self {
            Self::Wall { style, .. } => Some(style),
            _ => None,
        }
    }

    /// True if this structure provides the floor of the volume above it.
    pub const fn provides_floor(self) -> bool {
        match self {
            Self::Solid | Self::DarkOre { .. } | Self::Wall { .. } | Self::Excavation { .. } => true,
            Self::Empty | Self::Building { .. } => false,
        }
    }
}

/// A structure's entity, occupied area, and data specific to its kind.
#[derive(Clone, Copy, Debug)]
pub struct StructureEntry<D> {
    pub entity: Entity,
    pub area: GridArea,
    pub data: D,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DarkOreData;
#[derive(Clone, Copy, Debug, Default)]
pub struct WallData;
#[derive(Clone, Copy, Debug, Default)]
pub struct BuildingData {
    pub operational: bool,
}

/// Associates a structure's data type with its entry table in [`ObstacleGrid`].
pub trait StructureData: Sized {
    fn table(grid: &ObstacleGrid) -> &Slots<StructureEntry<Self>, Self>;
    fn table_mut(grid: &mut ObstacleGrid) -> &mut Slots<StructureEntry<Self>, Self>;
}
macro_rules! structure_data {
    ($($data:ty => $table:ident),* $(,)?) => {$(
        impl StructureData for $data {
            fn table(grid: &ObstacleGrid) -> &Slots<StructureEntry<Self>, Self> { &grid.$table }
            fn table_mut(grid: &mut ObstacleGrid) -> &mut Slots<StructureEntry<Self>, Self> { &mut grid.$table }
        }
    )*};
}
structure_data!(
    DarkOreData => dark_ores,
    WallData => walls,
    BuildingData => buildings,
);

#[derive(Clone, Debug, PartialEq)]
pub struct ObstacleCell {
    pub underground: GridStructure,
    pub elevation: GridStructure,
    pub on_elevation: GridStructure,
    pub quantum_field: Option<Entity>,
}
impl Default for ObstacleCell {
    fn default() -> Self {
        Self { underground: GridStructure::default_at(VolumeLevel::Underground), elevation: GridStructure::default_at(VolumeLevel::Elevation), on_elevation: GridStructure::default_at(VolumeLevel::OnElevation), quantum_field: None }
    }
}
impl GridCell for ObstacleCell {
    type Data = GridStructure;
    fn at(&self, level: VolumeLevel) -> &GridStructure {
        match level {
            VolumeLevel::Underground => &self.underground,
            VolumeLevel::Elevation => &self.elevation,
            VolumeLevel::OnElevation => &self.on_elevation,
        }
    }
    fn at_mut(&mut self, level: VolumeLevel) -> &mut GridStructure {
        match level {
            VolumeLevel::Underground => &mut self.underground,
            VolumeLevel::Elevation => &mut self.elevation,
            VolumeLevel::OnElevation => &mut self.on_elevation,
        }
    }
}
impl ObstacleCell {
    pub fn has_dark_ore(&self) -> bool {
        matches!(self.underground, GridStructure::DarkOre { .. })
    }
    pub fn is_within_quantum_field(&self) -> bool {
        self.quantum_field.is_some()
    }
    /// Returns whether this cell has the requested floor.
    /// The underground floor always exists; higher floors require a supporting structure below.
    pub fn has_floor(&self, floor: FloorLevel) -> bool {
        match floor {
            FloorLevel::UndergroundFloor => true,
            FloorLevel::GroundFloor => self.underground.provides_floor(),
            FloorLevel::ElevationFloor => self.elevation.provides_floor(),
        }
    }
    /// The highest volume with a floor under it.
    pub fn top_level(&self) -> VolumeLevel {
        if self.has_floor(FloorLevel::ElevationFloor) {
            VolumeLevel::OnElevation
        } else if self.has_floor(FloorLevel::GroundFloor) {
            VolumeLevel::Elevation
        } else {
            VolumeLevel::Underground
        }
    }
    /// True for solid earth beneath two empty volumes, with no quantum field.
    pub fn is_open_ground(&self) -> bool {
        self.underground == GridStructure::Solid && self.is_clear_above()
    }
    /// Whether the underground structure supports a level ground floor.
    /// Solid earth, ore and flat tunnels qualify; pits and ramps do not.
    pub fn has_flat_ground(&self) -> bool {
        self.underground.provides_floor() && !self.underground.is_ramp()
    }
    /// Whether ground is flat, has no ore, and has empty volumes above it with no quantum field.
    /// Allows placement over tunnels.
    pub fn is_placeable_ground(&self) -> bool {
        self.has_flat_ground() && !self.has_dark_ore() && self.is_clear_above()
    }
    /// Whether both volumes above ground are empty and the cell has no quantum field.
    fn is_clear_above(&self) -> bool {
        self.elevation.is_empty() && self.on_elevation.is_empty() && !self.is_within_quantum_field()
    }
    /// Returns the underground excavation shape, or `None` for solid earth and ore.
    pub fn excavation(&self) -> Option<ExcavationShape> {
        match self.underground {
            GridStructure::Empty => Some(ExcavationShape::Pit),
            GridStructure::Excavation { shape } => Some(ExcavationShape::Shaped(shape)),
            _ => None,
        }
    }
    /// Whether underground is solid earth with no wall or building in the `Elevation` volume.
    pub fn can_be_excavated(&self) -> bool {
        self.underground == GridStructure::Solid && !self.has_structure()
    }
    /// True if a wall or building occupies the cell's `Elevation` volume.
    pub fn has_structure(&self) -> bool {
        matches!(self.elevation, GridStructure::Wall { .. } | GridStructure::Building { .. })
    }
}

/// Structure occupancy and shared entries, with read access through [`BaseGrid`].
///
/// Use the imprint, relocation, and deprint methods to keep cells and entries synchronized.
/// Each structure must occupy a distinct area within its volume; callers must remove an existing
/// structure before overwriting its cells.
#[derive(Resource, Default)]
pub struct ObstacleGrid {
    cells: BaseGrid<ObstacleCell, GridVersion>,
    dark_ores: Slots<StructureEntry<DarkOreData>, DarkOreData>,
    walls: Slots<StructureEntry<WallData>, WallData>,
    buildings: Slots<StructureEntry<BuildingData>, BuildingData>,
}
impl Deref for ObstacleGrid {
    type Target = BaseGrid<ObstacleCell, GridVersion>;
    fn deref(&self) -> &Self::Target { &self.cells }
}
impl ObstacleGrid {
    pub fn new_with_size(bounds: impl Into<Bounds>) -> Self {
        Self { cells: BaseGrid::new_with_size(bounds), ..default() }
    }

    /// Returns the structure entry referenced by `slot`. Panics if the slot is empty or invalid.
    pub fn entry<D: StructureData>(&self, slot: Slot<D>) -> &StructureEntry<D> {
        D::table(self).get(slot).expect("a structure's slot stays filled while its cells hold it")
    }
    /// Returns the entry for updating structure data. Panics if the slot is empty or invalid.
    ///
    /// Changes do not update cell membership or advance the grid version.
    pub fn entry_mut<D: StructureData>(&mut self, slot: Slot<D>) -> &mut StructureEntry<D> {
        D::table_mut(self).get_mut(slot).expect("a structure's slot stays filled while its cells hold it")
    }
    /// The top of the cell at `coords`.
    pub fn top_position(&self, coords: GridCoords) -> GridVolumePosition {
        coords.at(self.cell(coords).top_level())
    }
    /// The top of every in-bounds cell of `area`.
    pub fn tops_in(&self, area: GridArea) -> impl Iterator<Item = GridVolumePosition> {
        area.iter_in_bounds(self.bounds).map(|coords| self.top_position(coords))
    }
    /// True if `position` is the top of its cell.
    pub fn is_top(&self, position: GridVolumePosition) -> bool {
        position == self.top_position(position.coords)
    }
    /// The entity of `structure`, or `None` for `Empty`, `Solid` and `Excavation`.
    pub fn structure_entity(&self, structure: GridStructure) -> Option<Entity> {
        Some(match structure {
            GridStructure::Empty | GridStructure::Solid | GridStructure::Excavation { .. } => return None,
            GridStructure::DarkOre { slot } => self.entry(slot).entity,
            GridStructure::Wall { slot, .. } => self.entry(slot).entity,
            GridStructure::Building { slot, .. } => self.entry(slot).entity,
        })
    }

    pub fn imprint_dark_ore(&mut self, area: GridArea, entity: Entity) {
        self.imprint(area, entity, DarkOreData, |slot| GridStructure::DarkOre { slot });
    }
    pub fn imprint_wall(&mut self, area: GridArea, entity: Entity, style: WallStyleKey, shape: VolumeShape) {
        self.imprint(area, entity, WallData, |slot| GridStructure::Wall { slot, style, shape });
    }
    pub fn imprint_building(&mut self, area: GridArea, entity: Entity, building_type: BuildingType) {
        self.imprint(area, entity, BuildingData::default(), |slot| GridStructure::Building { slot, building_type });
    }

    /// Updates the indexed building's operational state if its entity matches `entity`.
    /// Advances the grid version on change so pathfinding can refresh.
    pub fn set_building_operational(&mut self, area: GridArea, entity: Entity, operational: bool) {
        let Some(GridStructure::Building { slot, .. }) = self.structure_in(area) else { return; };
        let entry = self.entry_mut(slot);
        if entry.entity != entity || entry.data.operational == operational { return; }
        entry.data.operational = operational;
        self.cells.advance_version();
    }

    /// Moves a structure from `old` to `new`, preserving its slot and updating its entry's area.
    ///
    /// `old` must be the structure's full area; `new` must not overwrite another structure.
    pub fn relocate(&mut self, old: GridArea, new: GridArea) {
        let Some(structure) = self.structure_in(old) else { return; };
        self.fill(old, GridStructure::default_at(old.origin.level.volume()));
        self.fill(new, structure);
        self.set_entry_area(structure, new);
        self.cells.advance_version();
    }

    /// Restores default cell contents in the area's volume and frees the structure's slot.
    /// `area` must be the full area of one structure.
    pub fn deprint(&mut self, area: GridArea) {
        if let Some(structure) = self.structure_in(area) {
            self.free_entry(structure);
        }
        self.fill(area, GridStructure::default_at(area.origin.level.volume()));
        self.cells.advance_version();
    }

    /// Replaces underground solid earth with `shape` and advances the grid version.
    /// The caller must ensure `coords` is in bounds and contains solid earth.
    pub fn excavate(&mut self, coords: GridCoords, shape: ExcavationShape) {
        self.cells[coords.at(VolumeLevel::Underground)] = match shape {
            ExcavationShape::Pit => GridStructure::Empty,
            ExcavationShape::Shaped(shape) => GridStructure::Excavation { shape },
        };
        self.cells.advance_version();
    }
    /// Replaces an underground excavation with solid earth and advances the grid version.
    /// The caller must ensure `coords` is in bounds and contains an excavation.
    pub fn fill_excavation(&mut self, coords: GridCoords) {
        self.cells[coords.at(VolumeLevel::Underground)] = GridStructure::Solid;
        self.cells.advance_version();
    }

    /// Records `entity` as the quantum field covering the area's in-bounds cells.
    pub fn imprint_quantum_field(&mut self, area: GridArea, entity: Entity) {
        area.iter_in_bounds(self.cells.bounds).for_each(|coords| self.cells.cell_mut(coords).quantum_field = Some(entity));
        self.cells.advance_version();
    }
    pub fn deprint_quantum_field(&mut self, area: GridArea) {
        area.iter_in_bounds(self.cells.bounds).for_each(|coords| self.cells.cell_mut(coords).quantum_field = None);
        self.cells.advance_version();
    }

    /// Stores the structure's entry and writes its variant into the area's volume.
    fn imprint<D: StructureData>(&mut self, area: GridArea, entity: Entity, data: D, structure: impl FnOnce(Slot<D>) -> GridStructure) {
        let slot = D::table_mut(self).insert(StructureEntry { entity, area, data });
        self.fill(area, structure(slot));
        self.cells.advance_version();
    }

    /// Writes `structure` into the area's in-bounds cells at the area's volume level.
    fn fill(&mut self, area: GridArea, structure: GridStructure) {
        let level = area.origin.level.volume();
        for coords in area.iter_in_bounds(self.cells.bounds) {
            self.cells[coords.at(level)] = structure;
        }
    }

    /// Reads the structure from the area's first in-bounds cell at the area's volume level.
    fn structure_in(&self, area: GridArea) -> Option<GridStructure> {
        let level = area.origin.level.volume();
        let coords = area.iter_in_bounds(self.cells.bounds).next()?;
        Some(self.cells[coords.at(level)])
    }

    fn set_entry_area(&mut self, structure: GridStructure, area: GridArea) {
        match structure {
            GridStructure::Empty | GridStructure::Solid | GridStructure::Excavation { .. } => {}
            GridStructure::DarkOre { slot } => self.entry_mut(slot).area = area,
            GridStructure::Wall { slot, .. } => self.entry_mut(slot).area = area,
            GridStructure::Building { slot, .. } => self.entry_mut(slot).area = area,
        }
    }

    fn free_entry(&mut self, structure: GridStructure) {
        match structure {
            GridStructure::Empty | GridStructure::Solid | GridStructure::Excavation { .. } => {}
            GridStructure::DarkOre { slot } => { self.dark_ores.free(slot); }
            GridStructure::Wall { slot, .. } => { self.walls.free(slot); }
            GridStructure::Building { slot, .. } => { self.buildings.free(slot); }
        }
    }
}
