//! # Obstacle grid
//!
//! Each cell stores one [`GridStructure`] per level. The `Underground` level defaults to `Solid`;
//! the two levels above it default to `Empty`. Walls, buildings and ore keep frequently queried
//! data inline and use typed slots to reference their entity, area and additional data.
//!
//! All cells occupied by the same entity reference one slot. Imprint methods create the entry
//! and fill its cells; [`ObstacleGrid::deprint`] clears the cells and frees the slot.
//!
//! Excavations store their state directly in the `Underground` level: `Empty` for a pit, or
//! `Excavation` with an inline shape for a ramp or a flat excavation. [`ObstacleGrid::excavate`] and
//! [`ObstacleGrid::fill_excavation`] update one cell at a time.
//!
//! ## Geometry vs. traversal
//!
//! [`ObstacleCell`] and [`GridStructure`] expose raw cell geometry: open sides, hollowed passages,
//! floor support, and edge-to-edge floor continuity ([`ObstacleGrid::connects_floor`]).
//!
//! Traversal queries build on top of those primitives. [`ObstacleGrid::walkable_neighbors`] and
//! [`ObstacleGrid::walkable_cardinals`] implement the standard ground-unit and emission step rules;
//! systems with different movement constraints (such as blasts) query the cell primitives directly.

use std::ops::Deref;

use bevy::prelude::*;
use strum::IntoEnumIterator;

use game_core::prelude::{AltitudeLevel, Bounds, BuildingType, EdgeSide, EdgeSides, GridArea, GridCoords, GridPosition};
use grids::{GridVersion, base::{BaseGrid, GridCell}, slot_index::{Slot, Slots}};
use map_objects::{prelude::{ExcavationShape, StructureShape}, wall_style::WallStyleKey};

/// Contents of one level of a cell, including empty space and solid earth.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GridStructure {
    /// Open space. The default at `OnGround` and `OnElevation`; represents a pit at `Underground`.
    Empty,
    /// Packed earth, the default at `Underground`.
    Solid,
    DarkOre { slot: Slot<DarkOreData> },
    Wall { slot: Slot<WallData>, style: WallStyleKey, shape: StructureShape },
    Building { slot: Slot<BuildingData>, building_type: BuildingType },
    /// An underground ramp or flat excavation that supports the ground floor above.
    ///
    /// A [`StructureShape::Flat`] with no hollowed sides represents a refilled excavation, kept
    /// distinct from [`Self::Solid`] earth.
    Excavation { shape: StructureShape },
}

const _: () = assert!(size_of::<GridStructure>() == 8, "GridStructure sits in every level of every cell and is read in hot loops");

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
    pub const fn default_at(level: AltitudeLevel) -> Self {
        match level {
            AltitudeLevel::Underground => Self::Solid,
            AltitudeLevel::OnGround | AltitudeLevel::OnElevation | AltitudeLevel::Airborne | AltitudeLevel::AllEncompassing | AltitudeLevel::TopLevel => Self::Empty,
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
            Some(StructureShape::Ramp { high_side }) => Some(high_side),
            _ => None,
        }
    }

    /// True if the cell is open on `side` at this structure's level.
    pub const fn has_side_open(self, side: EdgeSide) -> bool {
        self.open_sides().contains(side)
    }

    /// Cell edges open at this structure's level: all sides for empty space and buildings, none for
    /// solid earth and ore, and [`StructureShape::open_sides`] for walls and excavations.
    const fn open_sides(self) -> EdgeSides {
        match self {
            Self::Empty | Self::Building { .. } => EdgeSides::ALL,
            Self::Solid | Self::DarkOre { .. } => EdgeSides::NONE,
            Self::Wall { shape, .. } | Self::Excavation { shape } => shape.open_sides(),
        }
    }

    /// Returns the shape of a wall or excavation, or `None` for other structures.
    pub const fn shape(self) -> Option<StructureShape> {
        match self {
            Self::Wall { shape, .. } | Self::Excavation { shape } => Some(shape),
            _ => None,
        }
    }

    /// Returns the style of a wall, or `None` for other structures.
    pub const fn wall_style(self) -> Option<WallStyleKey> {
        match self {
            Self::Wall { style, .. } => Some(style),
            _ => None,
        }
    }

    /// True if floors supported by `self` and `other` can connect across an edge.
    /// Walls of different styles remain separated by a gap; all other floor pairs connect.
    fn wall_style_meets(self, other: Self) -> bool {
        match (self.wall_style(), other.wall_style()) {
            (Some(style), Some(other_style)) => style == other_style,
            _ => true,
        }
    }

    /// True if this structure provides the floor of the level above it.
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
    pub on_ground: GridStructure,
    pub on_elevation: GridStructure,
    pub quantum_field: Option<Entity>,
}
impl Default for ObstacleCell {
    fn default() -> Self {
        Self { underground: GridStructure::default_at(AltitudeLevel::Underground), on_ground: GridStructure::default_at(AltitudeLevel::OnGround), on_elevation: GridStructure::default_at(AltitudeLevel::OnElevation), quantum_field: None }
    }
}
impl GridCell for ObstacleCell {
    type Data = GridStructure;
    fn at(&self, level: AltitudeLevel) -> &GridStructure {
        match level {
            AltitudeLevel::Underground => &self.underground,
            AltitudeLevel::OnGround => &self.on_ground,
            AltitudeLevel::OnElevation | AltitudeLevel::Airborne | AltitudeLevel::AllEncompassing | AltitudeLevel::TopLevel => &self.on_elevation,
        }
    }
    fn at_mut(&mut self, level: AltitudeLevel) -> &mut GridStructure {
        match level {
            AltitudeLevel::Underground => &mut self.underground,
            AltitudeLevel::OnGround => &mut self.on_ground,
            AltitudeLevel::OnElevation | AltitudeLevel::Airborne | AltitudeLevel::AllEncompassing | AltitudeLevel::TopLevel => &mut self.on_elevation,
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
    /// The structure at the layer below `level`. Solid earth lies below the lowest level.
    pub fn structure_under(&self, level: AltitudeLevel) -> GridStructure {
        level.three_layer_below().map_or(GridStructure::Solid, |below| *self.at(below))
    }
    /// Returns whether `level` has a floor in this cell, provided by the structure beneath it.
    pub fn has_floor(&self, level: AltitudeLevel) -> bool {
        self.structure_under(level).provides_floor()
    }
    /// Elevation at which the floor of `level` meets `side`:
    /// - `Some(level)` for a flat floor or a ramp's crest (`high_side`),
    /// - `level.three_layer_below()` at a ramp's foot (`high_side.opposite()`),
    /// - `None` if `level` has no floor or `side` is a ramp's sloped side.
    fn floor_edge_base(&self, level: AltitudeLevel, side: EdgeSide) -> Option<AltitudeLevel> {
        let floor = self.structure_under(level);
        if !floor.provides_floor() { return None; }
        match floor.ramp_high_side() {
            None => Some(level),
            Some(high_side) if side == high_side => Some(level),
            Some(high_side) if side == high_side.opposite() => level.three_layer_below(),
            Some(_) => None,
        }
    }
    /// Level in this cell whose floor meets `side` at elevation `edge_level`.
    ///
    /// A ramp at `edge_level` covers the floor beneath it: if its foot faces `side`, the connected
    /// surface is the ramp itself (occupying `edge_level.three_layer_above()`); otherwise the ramp
    /// blocks the edge. Without a ramp at `edge_level`, returns `Some(edge_level)` when this cell's
    /// floor at `edge_level` meets `side` at that height.
    fn floor_level_reaching(&self, edge_level: AltitudeLevel, side: EdgeSide) -> Option<AltitudeLevel> {
        match self.at(edge_level).ramp_high_side() {
            Some(high_side) => if side == high_side.opposite() { edge_level.three_layer_above() } else { None },
            None => (self.floor_edge_base(edge_level, side) == Some(edge_level)).then_some(edge_level),
        }
    }
    /// The highest of `Underground`, `OnGround` and `OnElevation` with a floor.
    pub fn top_level(&self) -> AltitudeLevel {
        if self.has_floor(AltitudeLevel::OnElevation) {
            AltitudeLevel::OnElevation
        } else if self.has_floor(AltitudeLevel::OnGround) {
            AltitudeLevel::OnGround
        } else {
            AltitudeLevel::Underground
        }
    }
    /// True for solid earth beneath two empty levels, with no quantum field.
    pub fn is_open_ground(&self) -> bool {
        self.underground == GridStructure::Solid && self.is_clear_above()
    }
    /// Whether the underground structure supports a flat floor at `OnGround`.
    /// Solid earth, ore and flat excavations qualify; pits and ramps do not.
    pub fn has_flat_ground(&self) -> bool {
        self.underground.provides_floor() && !self.underground.is_ramp()
    }
    /// Whether ground is flat, has no ore, and has empty levels above it with no quantum field.
    /// Allows placement over flat excavations.
    pub fn is_placeable_ground(&self) -> bool {
        self.has_flat_ground() && !self.has_dark_ore() && self.is_clear_above()
    }
    /// Whether both levels above ground are empty and the cell has no quantum field.
    fn is_clear_above(&self) -> bool {
        self.on_ground.is_empty() && self.on_elevation.is_empty() && !self.is_within_quantum_field()
    }
    /// Returns the underground excavation shape, or `None` for solid earth and ore.
    pub fn excavation(&self) -> Option<ExcavationShape> {
        match self.underground {
            GridStructure::Empty => Some(ExcavationShape::Pit),
            GridStructure::Excavation { shape } => Some(ExcavationShape::Shaped(shape)),
            _ => None,
        }
    }
    /// True if a wall or building occupies the cell's `OnGround` level.
    pub fn has_structure(&self) -> bool {
        matches!(self.on_ground, GridStructure::Wall { .. } | GridStructure::Building { .. })
    }
}

/// Structure occupancy and shared entries, with read access through [`BaseGrid`].
///
/// Use the imprint, relocation, and deprint methods to keep cells and entries synchronized.
/// Each structure must occupy a distinct area within its level; callers must remove an existing
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
    pub fn top_position(&self, coords: GridCoords) -> GridPosition {
        coords.at(self.cell(coords).top_level())
    }
    /// The top of every in-bounds cell of `area`.
    pub fn tops_in(&self, area: GridArea) -> impl Iterator<Item = GridPosition> {
        area.iter_in_bounds(self.bounds).map(|coords| self.top_position(coords))
    }
    /// Returns the neighbor position across `side` whose floor connects with `position`'s floor,
    /// or `None` if either floor is missing, the shared edge is blocked, or adjacent walls use
    /// different styles.
    ///
    /// Floors connect when they meet the shared edge at the same elevation and both cells are open
    /// at that elevation. Because a ramp's surface belongs to the level above the ramp structure,
    /// stepping onto a ramp's foot transitions to the level above, and stepping off its foot
    /// transitions to the level below.
    pub fn connects_floor(&self, position: GridPosition, side: EdgeSide) -> Option<GridPosition> {
        let from_cell = self.get(position.coords)?;
        let coords = position.coords.neighbor(side);
        let target_cell = self.get(coords)?;
        let base = from_cell.floor_edge_base(position.level, side)?;
        if !from_cell.at(base).has_side_open(side) || !target_cell.at(base).has_side_open(side.opposite()) { return None; }
        let level = target_cell.floor_level_reaching(base, side.opposite())?;
        let meets = from_cell.structure_under(position.level).wall_style_meets(target_cell.structure_under(level));
        meets.then(|| coords.at(level))
    }
    /// Cardinal neighbors reachable from `position` via [`Self::connects_floor`], in [`EdgeSide`]
    /// iteration order.
    pub fn walkable_cardinals(&self, position: GridPosition) -> impl Iterator<Item = GridPosition> + '_ {
        EdgeSide::iter().filter_map(move |side| self.connects_floor(position, side))
    }
    /// All neighbors reachable from `position` in one step: [`Self::walkable_cardinals`] followed
    /// by [`Self::walkable_diagonals`]. `position.coords` must be in bounds.
    pub fn walkable_neighbors(&self, position: GridPosition) -> impl Iterator<Item = GridPosition> + '_ {
        self.walkable_cardinals(position).chain(self.walkable_diagonals(position))
    }
    /// Diagonal neighbors reachable from `position` in one step, in [`EdgeSide::DIAGONALS`] order.
    ///
    /// A diagonal step requires all four cardinal edges around the shared corner to connect to the
    /// same target position via [`Self::connects_floor`], both orthogonal side cells to be empty at
    /// their connected levels, and neither `position` nor the target to be inside a hollow
    /// structure. `position.coords` must be in bounds.
    pub fn walkable_diagonals(&self, position: GridPosition) -> impl Iterator<Item = GridPosition> + '_ {
        let is_hollow = |position: GridPosition| self[position].shape().is_some_and(StructureShape::is_hollow);
        let diagonals: &[_] = if is_hollow(position) { &[] } else { &EdgeSide::DIAGONALS };
        diagonals.iter().filter_map(move |&(first_side, second_side)| {
            let first = self.connects_floor(position, first_side)?;
            let second = self.connects_floor(position, second_side)?;
            let target = self.connects_floor(first, second_side)?;
            let is_corner_free = self[first].is_empty() && self[second].is_empty() && !is_hollow(target);
            (is_corner_free && self.connects_floor(second, first_side) == Some(target)).then_some(target)
        })
    }
    /// True if `position` is the top of its cell.
    pub fn is_top(&self, position: GridPosition) -> bool {
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
    pub fn imprint_wall(&mut self, area: GridArea, entity: Entity, style: WallStyleKey, shape: StructureShape) {
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
        self.fill(old, GridStructure::default_at(old.origin.level));
        self.fill(new, structure);
        self.set_entry_area(structure, new);
        self.cells.advance_version();
    }

    /// Restores default cell contents at the area's level and frees the structure's slot.
    /// `area` must be the full area of one structure.
    pub fn deprint(&mut self, area: GridArea) {
        if let Some(structure) = self.structure_in(area) {
            self.free_entry(structure);
        }
        self.fill(area, GridStructure::default_at(area.origin.level));
        self.cells.advance_version();
    }

    /// Replaces the underground solid earth or excavation with `shape` and advances the grid
    /// version. The caller must ensure `coords` is in bounds.
    pub fn excavate(&mut self, coords: GridCoords, shape: ExcavationShape) {
        self.cells[coords.at(AltitudeLevel::Underground)] = match shape {
            ExcavationShape::Pit => GridStructure::Empty,
            ExcavationShape::Shaped(shape) => GridStructure::Excavation { shape },
        };
        self.cells.advance_version();
    }
    /// Replaces an underground excavation with solid earth and advances the grid version.
    /// The caller must ensure `coords` is in bounds and contains an excavation.
    pub fn fill_excavation(&mut self, coords: GridCoords) {
        self.cells[coords.at(AltitudeLevel::Underground)] = GridStructure::Solid;
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

    /// Stores the structure's entry and writes its variant into the area's cells at the area's level.
    fn imprint<D: StructureData>(&mut self, area: GridArea, entity: Entity, data: D, structure: impl FnOnce(Slot<D>) -> GridStructure) {
        let slot = D::table_mut(self).insert(StructureEntry { entity, area, data });
        self.fill(area, structure(slot));
        self.cells.advance_version();
    }

    /// Writes `structure` into the area's in-bounds cells at the area's level.
    fn fill(&mut self, area: GridArea, structure: GridStructure) {
        let level = area.origin.level;
        for coords in area.iter_in_bounds(self.cells.bounds) {
            self.cells[coords.at(level)] = structure;
        }
    }

    /// Reads the structure from the area's first in-bounds cell at the area's level.
    fn structure_in(&self, area: GridArea) -> Option<GridStructure> {
        let level = area.origin.level;
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
