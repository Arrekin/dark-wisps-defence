//! # Slot Index
//!
//! Stores values in reusable slots and lists their slot indexes in grid cells for local lookups.
//! Slot indexes remain stable until freed. Callers manage cell membership and advance the version
//! when their consumers need to refresh.

use game_core::prelude::{Bounds, GridCoords};

use crate::{GridVersion, base::BaseGrid};

pub struct SlotIndex<T> {
    /// Values in slot order; `None` marks an empty slot.
    slots: Vec<Option<T>>,
    free_slots: Vec<usize>,
    /// Slot indexes per cell. Callers must keep each cell's entries unique.
    cells: BaseGrid<Vec<usize>, GridVersion>,
}
impl<T> Default for SlotIndex<T> {
    fn default() -> Self {
        Self { slots: Vec::new(), free_slots: Vec::new(), cells: BaseGrid::default() }
    }
}
impl<T> SlotIndex<T> {
    pub fn new_with_size(bounds: impl Into<Bounds>) -> Self {
        Self { cells: BaseGrid::new_with_size(bounds), ..Self::default() }
    }

    pub fn bounds(&self) -> Bounds {
        self.cells.bounds
    }

    /// Caller-managed version counter, incremented by [`Self::advance_version`].
    pub fn version(&self) -> GridVersion {
        self.cells.version
    }

    /// All slots, including empty slots represented by `None`.
    pub fn slots(&self) -> &[Option<T>] {
        &self.slots
    }

    /// Slot indexes per cell and the caller-managed version counter.
    pub fn cells(&self) -> &BaseGrid<Vec<usize>, GridVersion> {
        &self.cells
    }

    /// Returns the value in `slot`, or `None` if the slot is empty or out of bounds.
    pub fn get(&self, slot: usize) -> Option<&T> {
        self.slots.get(slot)?.as_ref()
    }

    /// Iterates over values listed in `coords`, skipping empty slots. Empty outside the grid.
    pub fn listed_at(&self, coords: GridCoords) -> impl Iterator<Item = &T> {
        self.cells.get(coords).into_iter().flatten().filter_map(|&slot| self.slots[slot].as_ref())
    }

    /// Reserves a reusable slot or appends a new one. The slot remains empty until [`Self::set`].
    pub fn allocate(&mut self) -> usize {
        self.free_slots.pop().unwrap_or_else(|| { self.slots.push(None); self.slots.len() - 1 })
    }

    /// Writes to an allocated slot without changing its cell memberships.
    pub fn set(&mut self, slot: usize, value: T) {
        self.slots[slot] = Some(value);
    }

    /// Clears an allocated slot and makes it available for reuse.
    ///
    /// The caller must remove all cell memberships first and must not free a slot twice.
    pub fn free(&mut self, slot: usize) {
        self.slots[slot] = None;
        self.free_slots.push(slot);
    }

    /// Adds `slot` to an in-bounds cell. The caller must ensure it is not already listed there.
    pub fn list(&mut self, slot: usize, coords: GridCoords) {
        self.cells[coords].push(slot);
    }

    /// Removes `slot` from an in-bounds cell if present.
    pub fn unlist(&mut self, slot: usize, coords: GridCoords) {
        let cell = &mut self.cells[coords];
        if let Some(position) = cell.iter().position(|&listed| listed == slot) {
            cell.swap_remove(position);
        }
    }

    /// Increments the version counter, wrapping on overflow. Mutations do not advance it automatically.
    pub fn advance_version(&mut self) {
        self.cells.version = self.cells.version.wrapping_add(1);
    }
}
