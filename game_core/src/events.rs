use bevy::prelude::*;

// Event that carries string-identified or constant game events
#[derive(Event)]
pub struct DynamicGameEvent(pub String);
impl DynamicGameEvent {
    pub fn game_started() -> Self { DynamicGameEvent("game-started".to_string()) }
}

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

// ============================================================================
// Triggers
//
// Activation trigger primitives for the objectives system. A trigger source
// entity carries `TriggerSource` (marker) and fires `TriggerFired` on itself
// when it activates. The core activation observer catches `TriggerFired` and
// activates all `Inactive` objectives with `ObjectiveActivatedBy(source)`.
// ============================================================================

/// Marker component on entities that are activation trigger sources.
/// Carried by StartGame trigger, objective roots (for chaining), and future
/// trigger types (timers, region sensors). `ObjectiveDetails` requires this
/// via `#[require(TriggerSource)]`, so objective roots always carry it;
/// other source types insert their own.
#[derive(Component, Default)]
pub struct TriggerSource;

/// Fired on a trigger source entity when it activates. The core activation
/// observer catches this and activates all `Inactive` objectives with
/// `ObjectiveActivatedBy(source)`. Chaining: the core fires this on an
/// objective root when it resolves `Satisfied`.
#[derive(EntityEvent, Clone, Copy)]
pub struct TriggerFired {
    #[event_target]
    pub entity: Entity,
}
