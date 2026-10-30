//! # Tower Range Index
//!
//! Uses cell membership to narrow tower range queries to nearby candidates.
//! Each cell lists the ranges that intersect it, as determined by [`AreaRange::reaches_cell`].
//! Cell ranges cover whole cells; circular ranges can cover only part of a cell, so position queries
//! also test the position against each candidate circle.

use bevy::{platform::collections::HashMap, prelude::*};

use game_core::prelude::{AreaRange, Bounds, GridArea, GridCoords, RangeType};

use crate::{GridVersion, slot_index::SlotIndex};

/// Index slot that stays stable across range updates. Removal frees it for reuse by another tower.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TowerRangeSlot {
    /// Zero-based slot index, less than [`TowerRangeIndex::slot_count`] while the tower is indexed.
    pub index: usize,
}

/// One tower's indexed range.
#[derive(Clone, Copy, Debug)]
pub struct IndexedTowerRange {
    pub entity: Entity,
    pub range: AreaRange,
    pub slot: TowerRangeSlot,
    /// Cached range bounds used to limit cell membership updates.
    bounding_box: GridArea,
}
impl IndexedTowerRange {
    fn new(entity: Entity, range: AreaRange, slot: usize) -> Self {
        Self { entity, range, slot: TowerRangeSlot { index: slot }, bounding_box: range.bounding_box() }
    }
}

#[derive(Resource, Default)]
pub struct TowerRangeIndex {
    index: SlotIndex<IndexedTowerRange>,
    tower_slots: HashMap<Entity, usize>,
}
impl TowerRangeIndex {
    pub fn new_with_size(bounds: impl Into<Bounds>) -> Self {
        Self { index: SlotIndex::new_with_size(bounds), ..default() }
    }

    pub fn bounds(&self) -> Bounds {
        self.index.bounds()
    }

    /// Increments when a range is added, changed, or removed.
    pub fn version(&self) -> GridVersion {
        self.index.version()
    }

    /// Number of slots, including free ones. Use this to size tables keyed by [`TowerRangeSlot::index`].
    pub fn slot_count(&self) -> usize {
        self.index.slots().len()
    }

    /// Returns the tower's indexed range, or `None` if the tower is not indexed.
    pub fn get(&self, tower: Entity) -> Option<&IndexedTowerRange> {
        self.index.get(*self.tower_slots.get(&tower)?)
    }

    /// Iterates over occupied slots in slot order.
    pub fn ranges(&self) -> impl Iterator<Item = &IndexedTowerRange> {
        self.index.slots().iter().flatten()
    }

    /// Ranges that reach any part of the cell at `coords`. Empty outside the map.
    pub fn ranges_reaching(&self, coords: GridCoords) -> impl Iterator<Item = &IndexedTowerRange> {
        self.index.listed_at(coords)
    }

    /// Ranges that contain `world_position`. Empty outside the map.
    pub fn ranges_containing(&self, world_position: Vec2) -> impl Iterator<Item = &IndexedTowerRange> {
        self.ranges_reaching(GridCoords::from_world_position(world_position))
            .filter(move |indexed| indexed.range.range_type == RangeType::Cells || indexed.range.contains(world_position))
    }

    /// Indexes `range` for `tower`, replacing its previous range. Does nothing if the range is unchanged.
    pub fn upsert(&mut self, tower: Entity, range: AreaRange) {
        let slot = *self.tower_slots.entry(tower).or_insert_with(|| self.index.allocate());
        let old = self.index.get(slot).copied();
        if old.is_some_and(|old| old.range == range) { return; }
        let new = IndexedTowerRange::new(tower, range, slot);
        self.relist(slot, old, Some(new));
        self.index.set(slot, new);
    }

    /// Removes the tower's range from all cells and frees its slot, if present.
    pub fn remove(&mut self, tower: Entity) {
        let Some(slot) = self.tower_slots.remove(&tower) else { return; };
        let old = self.index.get(slot).copied();
        self.relist(slot, old, None);
        self.index.free(slot);
    }

    /// Updates cell memberships that differ between `old` and `new`, then advances the version.
    fn relist(&mut self, slot: usize, old: Option<IndexedTowerRange>, new: Option<IndexedTowerRange>) {
        // Changing only the minimum affects cells within the larger of the old and new inner boundaries.
        let (unlist_region, enlist_region) = match (old, new) {
            (Some(old), Some(new)) if AreaRange { min: new.range.min, ..old.range } == new.range => {
                let hole = AreaRange { max: old.range.min.max(new.range.min), ..new.range }.bounding_box();
                (Some(hole), Some(hole))
            }
            _ => (old.map(|indexed| indexed.bounding_box), new.map(|indexed| indexed.bounding_box)),
        };
        let bounds = self.index.bounds();
        let reached = |indexed: Option<IndexedTowerRange>, coords| indexed.is_some_and(|indexed| indexed.range.reaches_cell(coords));
        for coords in unlist_region.into_iter().flat_map(|area| area.iter_in_bounds(bounds)) {
            if reached(old, coords) && !reached(new, coords) { self.index.unlist(slot, coords); }
        }
        for coords in enlist_region.into_iter().flat_map(|area| area.iter_in_bounds(bounds)) {
            if !reached(old, coords) && reached(new, coords) { self.index.list(slot, coords); }
        }
        self.index.advance_version();
    }
}
