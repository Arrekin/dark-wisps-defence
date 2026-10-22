//! # Field Index
//!
//! Finds which field owns a point by checking only the candidates in that point's grid cell.
//! Each field keeps a stable slot until removal. Cells list these slots; [`IndexedField::claim`]
//! decides ownership among the candidates.
//!
//! A field's imprint is the grid rectangle enclosing its radius plus [`FIELD_INDEX_MARGIN`].
//! Cell lists change when the imprint changes, including when the radius reaches zero.
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

use game_core::prelude::{Bounds, CELL_SIZE, GridCoords, GridImprint};
use grids::{base::BaseGrid, GridVersion};

use crate::field::{Field, FieldType};

/// Extra radius in world pixels used to build a field's imprint.
/// This lets shaders find neighbouring fields when drawing seams.
pub const FIELD_INDEX_MARGIN: f32 = CELL_SIZE;

/// The field's slot in the index, assigned when it joins.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldSlot(pub usize);

/// Field data and grid coverage stored in one index slot.
#[derive(Clone, Copy)]
pub struct IndexedField {
    pub entity: Entity,
    pub field_type: FieldType,
    pub center: Vec2,
    pub current_radius: f32,
    pub strength: f32,
    /// Origin cell and grid coverage. `None` when the radius is zero or negative.
    imprint: Option<(GridCoords, GridImprint)>,
}
impl IndexedField {
    /// Copies the field data and computes its grid coverage, including [`FIELD_INDEX_MARGIN`].
    fn new(entity: Entity, field_type: FieldType, center: Vec2, field: &Field) -> Self {
        let imprint = (field.current_radius > 0.).then(|| {
            let reach = field.current_radius + FIELD_INDEX_MARGIN;
            let origin = GridCoords::from_world_vec2(center - Vec2::splat(reach));
            let end = GridCoords::from_world_vec2(center + Vec2::splat(reach));
            (origin, GridImprint::Rectangle { width: end.x - origin.x + 1, height: end.y - origin.y + 1 })
        });
        Self { entity, field_type, center, current_radius: field.current_radius, strength: field.strength, imprint }
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
    /// Indexed by slot. A free slot is `None`.
    slots: Vec<Option<IndexedField>>,
    free_slots: Vec<usize>,
    /// Candidate slots per cell. A slot appears at most once in a cell.
    cells: BaseGrid<Vec<usize>, GridVersion>,
}
impl FieldIndex {
    pub fn new_with_size(bounds: impl Into<Bounds>) -> Self {
        Self { cells: BaseGrid::new_with_size(bounds), ..default() }
    }

    /// Every slot in slot order. A free slot is `None`.
    pub fn slots(&self) -> &[Option<IndexedField>] {
        &self.slots
    }

    /// Candidate slots per grid cell, with a version counter for changes to the lists.
    pub fn cells(&self) -> &BaseGrid<Vec<usize>, GridVersion> {
        &self.cells
    }

    /// Gives `entity` a slot and lists it in the cells its imprint covers.
    pub fn insert(&mut self, entity: Entity, field_type: FieldType, center: Vec2, field: &Field) -> FieldSlot {
        let slot = FieldSlot(self.free_slots.pop().unwrap_or_else(|| { self.slots.push(None); self.slots.len() - 1 }));
        self.write(slot, Some(IndexedField::new(entity, field_type, center, field)));
        slot
    }

    /// Updates the field data. Rebuilds its cell memberships only if the imprint changed.
    pub fn update(&mut self, slot: FieldSlot, field_type: FieldType, center: Vec2, field: &Field) {
        let Some(indexed_field) = self.slots[slot.0] else { return; };
        self.write(slot, Some(IndexedField::new(indexed_field.entity, field_type, center, field)));
    }

    /// Frees the slot if `entity` still holds it.
    pub fn remove(&mut self, slot: FieldSlot, entity: Entity) {
        let Some(Some(indexed_field)) = self.slots.get(slot.0) else { return; };
        if indexed_field.entity != entity { return; }
        self.write(slot, None);
        self.free_slots.push(slot.0);
    }

    /// Returns the containing field with the lowest claim, or `None` outside all fields or the map.
    /// Only candidates in the point's cell are checked. Equal claims favour the first candidate.
    pub fn owner_at(&self, world_position: Vec2) -> Option<Entity> {
        let coords = GridCoords::from_world_vec2(world_position);
        if !coords.are_in_bounds(self.cells.bounds) { return None; }
        self.cells[coords].iter()
            .filter_map(|&slot| {
                let candidate = self.slots[slot].as_ref()?;
                Some((candidate.entity, candidate.claim(world_position)?))
            })
            .min_by(|(_, claim), (_, other_claim)| claim.total_cmp(other_claim))
            .map(|(entity, _)| entity)
    }

    /// Replaces the slot's data and updates its cell memberships if the imprint changed.
    fn write(&mut self, slot: FieldSlot, new: Option<IndexedField>) {
        let old_imprint = self.slots[slot.0].and_then(|indexed_field| indexed_field.imprint);
        self.relist(slot, old_imprint, new.and_then(|indexed_field| indexed_field.imprint));
        self.slots[slot.0] = new;
    }

    /// Rebuilds the slot's cell memberships and advances the grid version when the imprints differ.
    fn relist(&mut self, slot: FieldSlot, old: Option<(GridCoords, GridImprint)>, new: Option<(GridCoords, GridImprint)>) {
        if old == new { return; }
        if let Some((origin, imprint)) = old {
            for coords in imprint.iter_in_bounds(origin, self.cells.bounds) {
                let cell = &mut self.cells[coords];
                if let Some(position) = cell.iter().position(|&listed| listed == slot.0) {
                    cell.swap_remove(position);
                }
            }
        }
        if let Some((origin, imprint)) = new {
            for coords in imprint.iter_in_bounds(origin, self.cells.bounds) {
                self.cells[coords].push(slot.0);
            }
        }
        self.cells.version = self.cells.version.wrapping_add(1);
    }
}
