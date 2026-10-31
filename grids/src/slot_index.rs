//! # Slot Index
//!
//! [`Slots`] keeps values at stable indices and reuses freed slots. [`Slot`] identifies an index
//! with a key type, so APIs can distinguish slots for different kinds of data. Slots can also
//! index slices and vectors arranged in the same order as the store.
//!
//! [`SlotIndex`] adds cell membership lists for spatial lookups. Callers maintain these lists
//! and advance the version when consumers need to refresh.

use std::{fmt, hash::{Hash, Hasher}, marker::PhantomData, ops::{Index, IndexMut}};

use bevy::prelude::*;

use game_core::prelude::{Bounds, GridCoords};

use crate::{GridVersion, base::BaseGrid};

/// An index into a [`Slots`] store, distinguished by key type `K`.
///
/// Valid until freed. Slots have no generation or store identity; callers must use them with
/// the correct store and discard them when freed. As a component, a slot identifies an entity's
/// entry in that store.
#[derive(Component)]
pub struct Slot<K> {
    /// Position of the slot in the store.
    pub index: u32,
    key: PhantomData<fn() -> K>,
}
impl<K> Slot<K> {
    pub const fn new(index: u32) -> Self {
        Self { index, key: PhantomData }
    }
}
impl<K> Clone for Slot<K> {
    fn clone(&self) -> Self { *self }
}
impl<K> Copy for Slot<K> {}
impl<K> PartialEq for Slot<K> {
    fn eq(&self, other: &Self) -> bool { self.index == other.index }
}
impl<K> Eq for Slot<K> {}
impl<K> Hash for Slot<K> {
    fn hash<H: Hasher>(&self, state: &mut H) { self.index.hash(state); }
}
impl<K> fmt::Debug for Slot<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Slot({})", self.index)
    }
}

impl<T, K> Index<Slot<K>> for [T] {
    type Output = T;
    fn index(&self, slot: Slot<K>) -> &T { &self[slot.index as usize] }
}
impl<T, K> IndexMut<Slot<K>> for [T] {
    fn index_mut(&mut self, slot: Slot<K>) -> &mut T { &mut self[slot.index as usize] }
}
impl<T, K> Index<Slot<K>> for Vec<T> {
    type Output = T;
    fn index(&self, slot: Slot<K>) -> &T { &self[slot.index as usize] }
}
impl<T, K> IndexMut<Slot<K>> for Vec<T> {
    fn index_mut(&mut self, slot: Slot<K>) -> &mut T { &mut self[slot.index as usize] }
}

/// Values stored at stable indices, with freed slots available for reuse.
///
/// `K` is the slot's key type and defaults to the stored value type `T`.
pub struct Slots<T, K = T> {
    /// Values in slot order; `None` marks an empty slot.
    values: Vec<Option<T>>,
    free_slots: Vec<Slot<K>>,
}
impl<T, K> Default for Slots<T, K> {
    fn default() -> Self {
        Self { values: Vec::new(), free_slots: Vec::new() }
    }
}
impl<T, K> Slots<T, K> {
    /// All slots, including empty slots represented by `None`.
    pub fn as_slice(&self) -> &[Option<T>] {
        &self.values
    }

    /// Returns the value in `slot`, or `None` if the slot is empty.
    pub fn get(&self, slot: Slot<K>) -> Option<&T> {
        self.values[slot].as_ref()
    }
    pub fn get_mut(&mut self, slot: Slot<K>) -> Option<&mut T> {
        self.values[slot].as_mut()
    }

    /// Reserves a reusable slot or appends a new one. The slot remains empty until [`Self::set`].
    pub fn allocate(&mut self) -> Slot<K> {
        self.free_slots.pop().unwrap_or_else(|| { self.values.push(None); Slot::new(self.values.len() as u32 - 1) })
    }
    /// Stores `value` in a new slot.
    pub fn insert(&mut self, value: T) -> Slot<K> {
        let slot = self.allocate();
        self.set(slot, value);
        slot
    }
    /// Writes to an allocated slot.
    pub fn set(&mut self, slot: Slot<K>, value: T) {
        self.values[slot] = Some(value);
    }
    /// Clears an allocated slot, makes it available for reuse and returns its value.
    ///
    /// The caller must not free a slot twice.
    pub fn free(&mut self, slot: Slot<K>) -> Option<T> {
        self.free_slots.push(slot);
        self.values[slot].take()
    }
}

/// A slot store with a list of slot references for each grid cell.
///
/// Callers must keep each cell's list unique and remove its references before freeing a slot.
pub struct SlotIndex<T> {
    slots: Slots<T>,
    cells: BaseGrid<Vec<Slot<T>>, GridVersion>,
}
impl<T> Default for SlotIndex<T> {
    fn default() -> Self {
        Self { slots: Slots::default(), cells: BaseGrid::default() }
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
        self.slots.as_slice()
    }

    /// Slots per grid cell and the caller-managed version counter.
    pub fn cells(&self) -> &BaseGrid<Vec<Slot<T>>, GridVersion> {
        &self.cells
    }

    /// Returns the value in `slot`, or `None` if the slot is empty.
    pub fn get(&self, slot: Slot<T>) -> Option<&T> {
        self.slots.get(slot)
    }

    /// Iterates over values listed in `coords`, skipping empty slots. Empty outside the grid.
    pub fn listed_at(&self, coords: GridCoords) -> impl Iterator<Item = &T> {
        self.cells.get(coords).into_iter().flatten().filter_map(|&slot| self.slots.get(slot))
    }

    /// Reserves a reusable slot or appends a new one. The slot remains empty until [`Self::set`].
    pub fn allocate(&mut self) -> Slot<T> {
        self.slots.allocate()
    }

    /// Writes to an allocated slot without changing its cell memberships.
    pub fn set(&mut self, slot: Slot<T>, value: T) {
        self.slots.set(slot, value);
    }

    /// Clears an allocated slot and makes it available for reuse.
    ///
    /// The caller must remove all cell memberships first and must not free a slot twice.
    pub fn free(&mut self, slot: Slot<T>) {
        self.slots.free(slot);
    }

    /// Adds `slot` to an in-bounds cell. The caller must ensure it is not already listed there.
    pub fn list(&mut self, slot: Slot<T>, coords: GridCoords) {
        self.cells[coords].push(slot);
    }

    /// Removes `slot` from an in-bounds cell if present.
    pub fn unlist(&mut self, slot: Slot<T>, coords: GridCoords) {
        let cell = &mut self.cells[coords];
        if let Some(position) = cell.iter().position(|&listed| listed == slot) {
            cell.swap_remove(position);
        }
    }

    /// Increments the version counter, wrapping on overflow. Mutations do not advance it automatically.
    pub fn advance_version(&mut self) {
        self.cells.advance_version();
    }
}
