//! # Field Index
//!
//! Finds which field owns a point by checking only the candidates in that point's grid cell.
//! Each field keeps a stable slot until removal. Cells list these slots; [`IndexedField::claim`]
//! decides ownership among the candidates.
//!
//! A field's area is the grid rectangle enclosing its radius plus [`FIELD_INDEX_MARGIN`].
//! Cell lists change when the area changes, including when the radius reaches zero.
//!
//! ## Example lookup
//!
//! Two fields: A at (400, 320) with radius 224, and B at (700, 320) with radius 180. A third slot
//! was freed when its field despawned.
//!
//! ```text
//! slots   [0] A: centre (400, 320), radius 224, strength 1
//!         [1] B: centre (700, 320), radius 180, strength 1
//!         [2] None                                  (free, reused by the next field)
//!
//! cells   (12, 10) -> [0]                           only A reaches this cell
//!         (18, 10) -> [0, 1]                        A and B both reach it
//!         (40, 40) -> []                            no field nearby
//!
//! owner_at((590, 330))
//!   cell (18, 10) -> candidates [0, 1]
//!   A: inside, claim 0.72
//!   B: inside, claim 0.37  -> B owns the point
//! ```

use bevy::prelude::*;

use game_core::prelude::{Bounds, CELL_SIZE, GridArea, GridCoords};
use grids::{base::BaseGrid, slot_index::SlotIndex, GridVersion};

use crate::field::{Field, FieldType};

/// Extra radius in world pixels used to build a field's area.
/// This lets shaders find neighbouring fields when drawing seams.
pub const FIELD_INDEX_MARGIN: f32 = CELL_SIZE;

/// The field's slot in the index, assigned when it joins.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldSlot {
    pub index: usize,
}

/// Field data and grid coverage stored in one index slot.
#[derive(Clone, Copy)]
pub struct IndexedField {
    pub entity: Entity,
    pub field_type: FieldType,
    pub center: Vec2,
    pub current_radius: f32,
    pub strength: f32,
    /// Grid coverage. `None` when the radius is zero or negative.
    area: Option<GridArea>,
}
impl IndexedField {
    /// Copies the field data and computes its grid coverage, including [`FIELD_INDEX_MARGIN`].
    fn new(entity: Entity, field_type: FieldType, center: Vec2, field: &Field) -> Self {
        let area = (field.current_radius > 0.).then(|| GridArea::covering_circle(center, field.current_radius + FIELD_INDEX_MARGIN));
        Self { entity, field_type, center, current_radius: field.current_radius, strength: field.strength, area }
    }

    /// Returns `(distance / (radius * strength))²` for a point inside the field.
    /// The lowest claim wins ownership. Returns `None` outside the radius or when the radius
    /// is zero or negative.
    pub fn claim(&self, point: Vec2) -> Option<f32> {
        if self.current_radius <= 0. { return None; }
        let distance_squared = point.distance_squared(self.center);
        if distance_squared > self.current_radius * self.current_radius { return None; }
        let weight = self.current_radius * self.strength;
        Some(distance_squared / (weight * weight))
    }
}

#[derive(Resource, Default)]
pub struct FieldIndex {
    index: SlotIndex<IndexedField>,
}
impl FieldIndex {
    pub fn new_with_size(bounds: impl Into<Bounds>) -> Self {
        Self { index: SlotIndex::new_with_size(bounds) }
    }

    /// Every slot in slot order. A free slot is `None`.
    pub fn slots(&self) -> &[Option<IndexedField>] {
        self.index.slots()
    }

    /// Candidate slots per grid cell, with a version counter for changes to the lists.
    pub fn cells(&self) -> &BaseGrid<Vec<usize>, GridVersion> {
        self.index.cells()
    }

    /// Gives `entity` a slot and lists it in the cells its area covers.
    pub fn insert(&mut self, entity: Entity, field_type: FieldType, center: Vec2, field: &Field) -> FieldSlot {
        let slot = FieldSlot { index: self.index.allocate() };
        self.write(slot, IndexedField::new(entity, field_type, center, field));
        slot
    }

    /// Updates the field data. Updates its cell memberships only if the area changed.
    pub fn update(&mut self, slot: FieldSlot, field_type: FieldType, center: Vec2, field: &Field) {
        let Some(indexed_field) = self.index.get(slot.index) else { return; };
        self.write(slot, IndexedField::new(indexed_field.entity, field_type, center, field));
    }

    /// Frees the slot if `entity` still holds it.
    pub fn remove(&mut self, slot: FieldSlot, entity: Entity) {
        let Some(indexed_field) = self.index.get(slot.index) else { return; };
        if indexed_field.entity != entity { return; }
        self.relist(slot, indexed_field.area, None);
        self.index.free(slot.index);
    }

    /// Returns the containing field with the lowest claim, or `None` outside all fields or the map.
    /// Only candidates in the point's cell are checked. Equal claims favour the first candidate.
    pub fn owner_at(&self, world_position: Vec2) -> Option<Entity> {
        self.index.listed_at(GridCoords::from_world_position(world_position))
            .filter_map(|candidate| Some((candidate.entity, candidate.claim(world_position)?)))
            .min_by(|(_, claim), (_, other_claim)| claim.total_cmp(other_claim))
            .map(|(entity, _)| entity)
    }

    /// Replaces the slot's data and updates its cell memberships if the area changed.
    fn write(&mut self, slot: FieldSlot, new: IndexedField) {
        let old_area = self.index.get(slot.index).and_then(|indexed_field| indexed_field.area);
        self.relist(slot, old_area, new.area);
        self.index.set(slot.index, new);
    }

    /// Moves the slot's cell memberships from `old` to `new` and advances the grid version when the
    /// areas differ. Cells both areas cover keep their entry.
    fn relist(&mut self, slot: FieldSlot, old: Option<GridArea>, new: Option<GridArea>) {
        if old == new { return; }
        match (old, new) {
            (Some(old), Some(new)) => {
                let (left, entered) = GridArea::diff(old, new);
                self.unlist(slot, left);
                self.enlist(slot, entered);
            }
            (Some(old), None) => self.unlist(slot, old.iter()),
            (None, Some(new)) => self.enlist(slot, new.iter()),
            (None, None) => {}
        }
        self.index.advance_version();
    }

    /// Removes the slot from every in-bounds cell of `cells`.
    fn unlist(&mut self, slot: FieldSlot, cells: impl Iterator<Item = GridCoords>) {
        let bounds = self.index.bounds();
        for coords in cells.filter(|coords| bounds.contains(*coords)) {
            self.index.unlist(slot.index, coords);
        }
    }

    /// Adds the slot to every in-bounds cell of `cells`.
    fn enlist(&mut self, slot: FieldSlot, cells: impl Iterator<Item = GridCoords>) {
        let bounds = self.index.bounds();
        for coords in cells.filter(|coords| bounds.contains(*coords)) {
            self.index.list(slot.index, coords);
        }
    }
}
