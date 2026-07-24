use bevy::prelude::*;

use game_core::prelude::{MomentKind, Moment};

// ============================================================================
// Objective moment kinds
//
// Each marker is a moment kind owned by the objective domain. The `MomentKind`
// derive infers the persistence key from the type name.
// ============================================================================

/// The parent objective has been satisfied (entered `Satisfied`).
#[derive(Component, Default, MomentKind)]
#[require(Moment, Name = Name::new("Objective Satisfied"))]
pub struct MomentObjectiveSatisfied;

/// The parent objective has failed (entered `Failed`).
#[derive(Component, Default, MomentKind)]
#[require(Moment, Name = Name::new("Objective Failed"))]
pub struct MomentObjectiveFailed;
