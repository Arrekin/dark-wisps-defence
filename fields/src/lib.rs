//! # Fields
//!
//! A field is an area a tower controls.
//!
//! Fields don't overlap. If two fields reach the same spot, only one of them owns it. Near its
//! centre a field almost always wins. Further out, a bigger or stronger field wins.
//!
//! ## The field entity
//!
//! A field is a separate entity from its tower. `FieldGeneratedBy` links it to the tower. The
//! field has its own `Transform`, so it doesn't have to sit on the tower and it can move.
//!
//! Every field has a [`Field`](field::Field) component: its radius, target radius and strength.
//! It also has a [`FieldType`](field::FieldType) component. Each field type has its own marker
//! component, builder, look and effects.
//!
//! The tower only sets the target radius. It sets its range while it works and zero when it
//! doesn't. The field's radius grows or shrinks toward the target at a fixed speed. When the tower
//! is destroyed, the field shrinks to zero and then despawns.
//!
//! Fields are not saved. Each tower creates its field again when the game loads.
//!
//! ## Finding the owner of a point
//!
//! [`FieldIndex`](index::FieldIndex) tells which field owns a point. It keeps a list of fields
//! per grid cell, so a lookup only checks the few fields near the point.
//!
//! The shaders get a copy of the index (`assets/shaders/fields/fields.wesl`). A field's look uses
//! it to draw only inside its own area and to find where its edges are.
//!
//! ## Wisps in fields
//!
//! Each wisp remembers which field it is in, judged by its position. When that changes, the old
//! field gets `FieldExited` and the new one gets `FieldEntered`. Each field type decides what those
//! events do. For example, a slow starts on `FieldEntered` and ends on `FieldExited`.
//!
//! When a field shrinks away, every wisp in it gets `FieldExited`. So its effects end without any
//! extra cleanup.

pub mod field;
pub mod field_types;
pub mod index;

pub mod prelude {
    pub use super::field::{Field, FieldAffectable, FieldType};
    pub use super::field_types::{BuilderCrucibleField, BuilderForceField, CrucibleField, ForceField};
    pub use super::index::{FieldIndex, FieldSlot};
}
