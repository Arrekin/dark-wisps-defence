//! # Floor cover
//!
//! Effects such as burning napalm and char marks persist on map cells. Each effect is an entity
//! whose domain handles spawning, gameplay, and persistence. An optional `TimeBound` controls expiry.
//!
//! [`FloorCoverGrid`] records each effect's entity, kind, and floor per cell, allowing lookups
//! without an ECS query. Loading the entities rebuilds the index; the grid itself is not saved.

use std::ops::{Deref, DerefMut};

use bevy::prelude::*;

use game_core::prelude::{Bounds, FloorLevel, GridArea, GridCoords, MapBound};
use grids::{GridVersion, base::{BaseGrid, GridSingle}};

/// A floor cover effect occupying the entity's `GridArea`, which must use a floor level.
///
/// The index tracks insertion and removal of this component. To change the area, remove
/// `FloorCover` before replacing `GridArea`, then insert `FloorCover` again.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
#[component(immutable)]
#[require(MapBound)]
pub struct FloorCover(pub FloorCoverKind);

/// Kinds of floor cover, ordered from bottom to top when drawing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FloorCoverKind {
    /// The burnt mark napalm leaves behind.
    Char = 0,
    /// Burning napalm.
    Napalm = 1,
}
impl FloorCoverKind {
    /// GPU code matching `FLOOR_COVER_*` in `assets/shaders/grids/floor_cover.wesl`.
    pub fn gpu_index(self) -> u32 {
        self as u32
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FloorCoverEntry {
    pub entity: Entity,
    pub kind: FloorCoverKind,
    pub level: FloorLevel,
}

#[derive(Resource, Default)]
pub struct FloorCoverGrid(BaseGrid<GridSingle<Vec<FloorCoverEntry>>, GridVersion>);
impl Deref for FloorCoverGrid {
    type Target = BaseGrid<GridSingle<Vec<FloorCoverEntry>>, GridVersion>;
    fn deref(&self) -> &Self::Target { &self.0 }
}
impl DerefMut for FloorCoverGrid {
    fn deref_mut(&mut self) -> &mut Self::Target { &mut self.0 }
}

impl FloorCoverGrid {
    pub fn new_with_size(bounds: impl Into<Bounds>) -> Self {
        Self(BaseGrid::new_with_size(bounds))
    }
    /// Adds `entity` with `kind` to every in-bounds cell of the area, on the area's floor.
    pub fn imprint(&mut self, area: GridArea, entity: Entity, kind: FloorCoverKind) {
        let level = area.origin.level.floor();
        let entry = FloorCoverEntry { entity, kind, level };
        for coords in area.iter_in_bounds(self.bounds) {
            self[coords].push(entry);
        }
        self.advance_version();
    }

    /// Removes `entity` from every in-bounds cell of the area without preserving entry order.
    pub fn deprint(&mut self, area: GridArea, entity: Entity) {
        for coords in area.iter_in_bounds(self.bounds) {
            let entries = &mut self[coords];
            let Some(position) = entries.iter().position(|entry| entry.entity == entity) else { continue; };
            entries.swap_remove(position);
        }
        self.advance_version();
    }

    /// Returns entries for all floors at `coords`, or an empty slice outside the map.
    pub fn entries_at(&self, coords: GridCoords) -> &[FloorCoverEntry] {
        self.get(coords).map_or(&[], |cell| cell.as_slice())
    }
}
