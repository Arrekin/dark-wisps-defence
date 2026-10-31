//! # Obstacle grid
//!
//! Each cell stores one [`GridStructure`] per volume. Underground cells default to `Solid`;
//! the two volumes above them default to `Empty`. Structure variants keep frequently queried
//! data inline and use typed slots to reference an entity, area, and additional data in a
//! table for that structure kind.
//!
//! All cells occupied by a structure reference the same slot. Imprint methods create the entry
//! and fill its cells; [`ObstacleGrid::deprint`] clears the cells and frees the slot.

use std::ops::Deref;

use bevy::prelude::*;

use game_core::prelude::{Bounds, BuildingType, CellSide, GridArea, VolumeLevel};
use grids::{GridVersion, base::BaseGrid, slot_index::{Slot, Slots}};
use map_objects::wall_style::WallStyleKey;

/// Contents of one cell volume, including empty space and solid earth.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GridStructure {
    /// Open space, the default at `Elevation` and `OnElevation`.
    Empty,
    /// Packed earth, the default at `Underground`.
    Solid,
    DarkOre { slot: Slot<DarkOreData> },
    /// An underground passage under intact ground.
    Tunnel { slot: Slot<TunnelData> },
    /// An open pit.
    Excavation { slot: Slot<ExcavationData> },
    /// At `Underground`, spans a pit at ground height.
    /// At `Elevation`, spans open ground at wall-top height.
    Bridge { slot: Slot<BridgeData> },
    /// A slope connecting adjacent heights, rising toward `high_side`.
    Ramp { slot: Slot<RampData>, high_side: CellSide },
    Wall { slot: Slot<WallData> },
    /// A passage through a wall at ground height.
    WallTunnel { slot: Slot<WallTunnelData> },
    Building { slot: Slot<BuildingData>, building_type: BuildingType },
}

impl GridStructure {
    /// GPU code matching `STRUCTURE_*` in `assets/shaders/grids/obstacles.wesl`.
    pub const fn gpu_index(self) -> u32 {
        match self {
            Self::Empty => 0,
            Self::Solid => 1,
            Self::DarkOre { .. } => 2,
            Self::Tunnel { .. } => 3,
            Self::Excavation { .. } => 4,
            Self::Bridge { .. } => 5,
            Self::Ramp { .. } => 6,
            Self::Wall { .. } => 7,
            Self::WallTunnel { .. } => 8,
            Self::Building { .. } => 9,
        }
    }

    /// Default cell contents: solid earth underground, empty space above.
    pub const fn default_at(level: VolumeLevel) -> Self {
        match level {
            VolumeLevel::Underground => Self::Solid,
            VolumeLevel::Elevation | VolumeLevel::OnElevation => Self::Empty,
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
pub struct TunnelData;
#[derive(Clone, Copy, Debug, Default)]
pub struct ExcavationData;
#[derive(Clone, Copy, Debug, Default)]
pub struct BridgeData;
#[derive(Clone, Copy, Debug, Default)]
pub struct RampData;
#[derive(Clone, Copy, Debug, Default)]
pub struct WallData {
    pub style: WallStyleKey,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct WallTunnelData;
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
    TunnelData => tunnels,
    ExcavationData => excavations,
    BridgeData => bridges,
    RampData => ramps,
    WallData => walls,
    WallTunnelData => wall_tunnels,
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
impl ObstacleCell {
    pub fn structure(&self, level: VolumeLevel) -> GridStructure {
        match level {
            VolumeLevel::Underground => self.underground,
            VolumeLevel::Elevation => self.elevation,
            VolumeLevel::OnElevation => self.on_elevation,
        }
    }
    fn structure_mut(&mut self, level: VolumeLevel) -> &mut GridStructure {
        match level {
            VolumeLevel::Underground => &mut self.underground,
            VolumeLevel::Elevation => &mut self.elevation,
            VolumeLevel::OnElevation => &mut self.on_elevation,
        }
    }

    pub fn has_dark_ore(&self) -> bool {
        matches!(self.underground, GridStructure::DarkOre { .. })
    }
    pub fn is_within_quantum_field(&self) -> bool {
        self.quantum_field.is_some()
    }
    /// True for solid earth beneath two empty volumes, with no quantum field.
    pub fn is_empty(&self) -> bool {
        self.elevation == GridStructure::Empty
            && self.on_elevation == GridStructure::Empty
            && self.underground == GridStructure::Solid
            && !self.is_within_quantum_field()
    }
    pub fn has_building(&self) -> bool {
        matches!(self.elevation, GridStructure::Building { .. })
    }
    pub fn has_wall(&self) -> bool {
        matches!(self.elevation, GridStructure::Wall { .. })
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
    tunnels: Slots<StructureEntry<TunnelData>, TunnelData>,
    excavations: Slots<StructureEntry<ExcavationData>, ExcavationData>,
    bridges: Slots<StructureEntry<BridgeData>, BridgeData>,
    ramps: Slots<StructureEntry<RampData>, RampData>,
    walls: Slots<StructureEntry<WallData>, WallData>,
    wall_tunnels: Slots<StructureEntry<WallTunnelData>, WallTunnelData>,
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
    /// The entity of `structure`, or `None` for `Empty` and `Solid`.
    pub fn structure_entity(&self, structure: GridStructure) -> Option<Entity> {
        Some(match structure {
            GridStructure::Empty | GridStructure::Solid => return None,
            GridStructure::DarkOre { slot } => self.entry(slot).entity,
            GridStructure::Tunnel { slot } => self.entry(slot).entity,
            GridStructure::Excavation { slot } => self.entry(slot).entity,
            GridStructure::Bridge { slot } => self.entry(slot).entity,
            GridStructure::Ramp { slot, .. } => self.entry(slot).entity,
            GridStructure::Wall { slot, .. } => self.entry(slot).entity,
            GridStructure::WallTunnel { slot } => self.entry(slot).entity,
            GridStructure::Building { slot, .. } => self.entry(slot).entity,
        })
    }

    pub fn imprint_dark_ore(&mut self, area: GridArea, entity: Entity) {
        self.imprint(area, entity, DarkOreData, |slot| GridStructure::DarkOre { slot });
    }
    pub fn imprint_tunnel(&mut self, area: GridArea, entity: Entity) {
        self.imprint(area, entity, TunnelData, |slot| GridStructure::Tunnel { slot });
    }
    pub fn imprint_excavation(&mut self, area: GridArea, entity: Entity) {
        self.imprint(area, entity, ExcavationData, |slot| GridStructure::Excavation { slot });
    }
    pub fn imprint_bridge(&mut self, area: GridArea, entity: Entity) {
        self.imprint(area, entity, BridgeData, |slot| GridStructure::Bridge { slot });
    }
    pub fn imprint_ramp(&mut self, area: GridArea, entity: Entity, high_side: CellSide) {
        self.imprint(area, entity, RampData, |slot| GridStructure::Ramp { slot, high_side });
    }
    pub fn imprint_wall(&mut self, area: GridArea, entity: Entity, style: WallStyleKey) {
        self.imprint(area, entity, WallData { style }, |slot| GridStructure::Wall { slot });
    }
    pub fn imprint_wall_tunnel(&mut self, area: GridArea, entity: Entity) {
        self.imprint(area, entity, WallTunnelData, |slot| GridStructure::WallTunnel { slot });
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
        let old_level = VolumeLevel::try_from(old.origin.level).expect("a structure stands in a volume");
        let new_level = VolumeLevel::try_from(new.origin.level).expect("a structure stands in a volume");
        for coords in old.iter_in_bounds(self.cells.bounds) {
            *self.cells[coords].structure_mut(old_level) = GridStructure::default_at(old_level);
        }
        for coords in new.iter_in_bounds(self.cells.bounds) {
            *self.cells[coords].structure_mut(new_level) = structure;
        }
        self.set_entry_area(structure, new);
        self.cells.advance_version();
    }

    /// Restores default cell contents in the area's volume and frees the structure's slot.
    /// `area` must be the full area of one structure.
    pub fn deprint(&mut self, area: GridArea) {
        if let Some(structure) = self.structure_in(area) {
            self.free_entry(structure);
        }
        let level = VolumeLevel::try_from(area.origin.level).expect("a structure stands in a volume");
        for coords in area.iter_in_bounds(self.cells.bounds) {
            *self.cells[coords].structure_mut(level) = GridStructure::default_at(level);
        }
        self.cells.advance_version();
    }

    /// Records `entity` as the quantum field covering the area's in-bounds cells.
    pub fn imprint_quantum_field(&mut self, area: GridArea, entity: Entity) {
        area.iter_in_bounds(self.cells.bounds).for_each(|coords| self.cells[coords].quantum_field = Some(entity));
        self.cells.advance_version();
    }
    pub fn deprint_quantum_field(&mut self, area: GridArea) {
        area.iter_in_bounds(self.cells.bounds).for_each(|coords| self.cells[coords].quantum_field = None);
        self.cells.advance_version();
    }

    /// True if `query` returns true for every in-bounds cell of the area.
    pub fn query_area_all(&self, area: GridArea, query: fn(&ObstacleCell) -> bool) -> bool {
        area.iter_in_bounds(self.cells.bounds).all(|cell| query(&self.cells[cell]))
    }
    /// True if `query` returns true for at least one in-bounds cell of the area.
    pub fn query_area_any(&self, area: GridArea, query: fn(&ObstacleCell) -> bool) -> bool {
        area.iter_in_bounds(self.cells.bounds).any(|cell| query(&self.cells[cell]))
    }
    /// Number of in-bounds cells for which `query` returns true.
    pub fn query_area_count(&self, area: GridArea, query: fn(&ObstacleCell) -> bool) -> usize {
        area.iter_in_bounds(self.cells.bounds).filter(|cell| query(&self.cells[*cell])).count()
    }
    /// Collects the `Some` results of `query` over the area's in-bounds cells.
    pub fn query_area_element<T>(&self, area: GridArea, query: fn(&ObstacleCell) -> Option<T>) -> Vec<T> {
        area.iter_in_bounds(self.cells.bounds).filter_map(|cell| query(&self.cells[cell])).collect()
    }

    /// Stores the structure's entry and writes its variant into the area's volume.
    fn imprint<D: StructureData>(&mut self, area: GridArea, entity: Entity, data: D, structure: impl Fn(Slot<D>) -> GridStructure) {
        let level = VolumeLevel::try_from(area.origin.level).expect("a structure stands in a volume");
        let slot = D::table_mut(self).insert(StructureEntry { entity, area, data });
        let structure = structure(slot);
        for coords in area.iter_in_bounds(self.cells.bounds) {
            *self.cells[coords].structure_mut(level) = structure;
        }
        self.cells.advance_version();
    }

    /// Reads the structure from the area's first in-bounds cell at the area's volume level.
    fn structure_in(&self, area: GridArea) -> Option<GridStructure> {
        let level = VolumeLevel::try_from(area.origin.level).expect("a structure stands in a volume");
        let coords = area.iter_in_bounds(self.cells.bounds).next()?;
        Some(self.cells[coords].structure(level))
    }

    fn set_entry_area(&mut self, structure: GridStructure, area: GridArea) {
        match structure {
            GridStructure::Empty | GridStructure::Solid => {}
            GridStructure::DarkOre { slot } => self.entry_mut(slot).area = area,
            GridStructure::Tunnel { slot } => self.entry_mut(slot).area = area,
            GridStructure::Excavation { slot } => self.entry_mut(slot).area = area,
            GridStructure::Bridge { slot } => self.entry_mut(slot).area = area,
            GridStructure::Ramp { slot, .. } => self.entry_mut(slot).area = area,
            GridStructure::Wall { slot } => self.entry_mut(slot).area = area,
            GridStructure::WallTunnel { slot } => self.entry_mut(slot).area = area,
            GridStructure::Building { slot, .. } => self.entry_mut(slot).area = area,
        }
    }

    fn free_entry(&mut self, structure: GridStructure) {
        match structure {
            GridStructure::Empty | GridStructure::Solid => {}
            GridStructure::DarkOre { slot } => { self.dark_ores.free(slot); }
            GridStructure::Tunnel { slot } => { self.tunnels.free(slot); }
            GridStructure::Excavation { slot } => { self.excavations.free(slot); }
            GridStructure::Bridge { slot } => { self.bridges.free(slot); }
            GridStructure::Ramp { slot, .. } => { self.ramps.free(slot); }
            GridStructure::Wall { slot, .. } => { self.walls.free(slot); }
            GridStructure::WallTunnel { slot } => { self.wall_tunnels.free(slot); }
            GridStructure::Building { slot, .. } => { self.buildings.free(slot); }
        }
    }
}
