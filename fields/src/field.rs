//! # Field
//!
//! Fields divide space between towers. Each point belongs to at most one field, regardless of
//! type. Every field entity carries [`Field`] for its radius and strength, alongside a component
//! that identifies its type.

use bevy::prelude::*;

use game_core::prelude::MapBound;

// ── Relationships ──────────────────────────────────────────────────────────────────────

/// The entity that generates this field. Every field has exactly one.
#[derive(Component)]
#[relationship(relationship_target = GeneratedField)]
pub struct FieldGeneratedBy(pub Entity);

/// The field this entity generates. An entity generates at most one.
#[derive(Component)]
#[relationship_target(relationship = FieldGeneratedBy)]
pub struct GeneratedField(Entity);
impl GeneratedField {
    pub fn field(&self) -> Entity {
        self.0
    }
}

// ── Field Events ───────────────────────────────────────────────────────────────────────

/// Triggered on a field when a `FieldAffectable` entity enters the space it owns.
/// `exited_field` identifies the previous field, or is `None` if the entity was outside all fields.
/// Ownership can change when the entity moves or when fields resize, move, or change strength.
#[derive(EntityEvent, Clone, Copy)]
pub struct FieldEntered {
    #[event_target]
    pub field: Entity,
    pub target: Entity,
    pub exited_field: Option<Entity>,
}

/// Triggered on a field when a `FieldAffectable` entity leaves the space it owns.
#[derive(EntityEvent, Clone, Copy)]
pub struct FieldExited {
    #[event_target]
    pub field: Entity,
    pub target: Entity,
}

// ── FieldAffectable ────────────────────────────────────────────────────────────────────

/// Marks an entity as affected by fields and tracks which field owns its position.
#[derive(Component, Default)]
pub struct FieldAffectable {
    pub current_field: Option<Entity>,
}

// ── Field ──────────────────────────────────────────────────────────────────────────────

/// Field types. Discriminants mirror the `FIELD_*` constants in fields.wesl.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum FieldType {
    Force,
}

/// Radius, strength, and type shared by all fields. The entity's `Transform` sets its centre.
#[derive(Component)]
#[require(MapBound)]
pub struct Field {
    pub field_type: FieldType,
    /// Current radius in world pixels.
    pub current_radius: f32,
    /// Desired radius in world pixels, set by the generator.
    pub target_radius: f32,
    /// Weight in ownership comparisons. Higher values claim more overlap; must be positive.
    pub strength: f32,
}
impl Field {
    /// Creates a field with both radii at zero and strength 1.
    pub fn new(field_type: FieldType) -> Self {
        Self { field_type, current_radius: 0., target_radius: 0., strength: 1. }
    }

    /// Moves the current radius toward the target radius by at most `max_step`, up or down.
    pub fn step_toward_target(&mut self, max_step: f32) {
        self.current_radius += (self.target_radius - self.current_radius).clamp(-max_step, max_step);
    }
}
