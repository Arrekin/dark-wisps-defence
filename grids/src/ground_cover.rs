//! # Ground cover
//!
//! Effects that remain on map cells, such as burning napalm and char marks. Each effect is an
//! entity. The domain that creates it handles spawning, gameplay and persistence; an optional
//! `TimeBound` controls expiry.
//!
//! `GroundCoverGrid` stores entities and their kinds per cell, allowing kind lookups without
//! querying entities. Loading ground cover entities rebuilds the grid; the grid itself is not saved.

use bevy::prelude::*;

use game_core::prelude::{GridCoords, GridImprint, MapBound};

use crate::{GridVersion, base::BaseGrid};

/// Ground cover on the cells defined by the entity's `GridCoords` and `GridImprint`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
#[component(immutable)]
#[require(GridCoords, GridImprint, MapBound)]
pub struct GroundCover(pub GroundCoverKind);

/// Kinds of ground cover, in drawing order, bottom first. Mirrored by `GROUND_COVER_*` in
/// `assets/shaders/grids/ground_cover.wesl`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroundCoverKind {
    /// The burnt mark napalm leaves behind.
    Char = 0,
    /// Burning napalm.
    Napalm = 1,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroundCoverEntry {
    pub entity: Entity,
    pub kind: GroundCoverKind,
}

pub type GroundCoverGrid = BaseGrid<Vec<GroundCoverEntry>, GridVersion>;

impl GroundCoverGrid {
    /// Adds `entry` to every in-bounds cell of the imprint.
    pub fn imprint(&mut self, coords: GridCoords, imprint: GridImprint, entry: GroundCoverEntry) {
        for cell in imprint.iter_in_bounds(coords, self.bounds) {
            self[cell].push(entry);
        }
        self.version = self.version.wrapping_add(1);
    }

    /// Removes `entity` from every in-bounds cell of the imprint without preserving entry order.
    pub fn deprint(&mut self, coords: GridCoords, imprint: GridImprint, entity: Entity) {
        for cell in imprint.iter_in_bounds(coords, self.bounds) {
            let Some(position) = self[cell].iter().position(|entry| entry.entity == entity) else { continue; };
            self[cell].swap_remove(position);
        }
        self.version = self.version.wrapping_add(1);
    }

    /// The entries on a cell. Empty outside the map.
    pub fn entries_at(&self, coords: GridCoords) -> &[GroundCoverEntry] {
        if !coords.are_in_bounds(self.bounds) { return &[]; }
        &self[coords]
    }
}
