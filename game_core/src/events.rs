use bevy::prelude::*;

#[derive(Message, Clone, Copy, Debug)]
pub struct DamageMessage {
    pub target: Entity,
    pub amount: f32,
}

// ============================================================================
// Technical State
//
// Folded event: one of the primitive technical-state components changed.
// Observed locally by entities that register a handler, letting each entity
// own its "what does operational mean for me" logic while sharing a single
// event type.
// ============================================================================

#[derive(EntityEvent, Clone, Copy)]
pub struct TechnicalStateChanged {
    #[event_target]
    pub entity: Entity,
    pub kind: TechnicalChange,
}

#[derive(Clone, Copy)]
pub enum TechnicalChange {
    /// Fired manually by builders after all observers are attached, so the
    /// initial `IsOperational` (or custom technical state) is assessed at
    /// spawn. Has no corresponding global observer — only builders trigger it.
    JustSpawned,
    PowerGained,      // IsPowered inserted
    PowerLost,        // IsPowered removed
    PlayerDisabled,   // DisabledByPlayer inserted
    PlayerEnabled,    // DisabledByPlayer removed
}
