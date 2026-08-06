use bevy::prelude::*;

// ============================================================================
// Display vocabulary
//
// Generic, domain-agnostic components for entities that the editor and UI
// present to the player. Any entity may carry any subset; no `require` between
// them.
// ============================================================================

#[derive(Component, Clone, Debug, Default)]
pub struct DisplayName(pub String);

#[derive(Component, Clone, Debug, Default)]
pub struct DisplayDescription(pub String);

/// Authored icon path. A global observer loads the handle and inserts
/// `DisplayIcon`, which is the derived form the UI reads.
#[derive(Component, Clone, Debug, Default)]
pub struct DisplayIconSwitcher(pub String);

#[derive(Component, Clone, Debug, Default)]
pub struct DisplayIcon(pub Handle<Image>);

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct DisplayOrder(pub u32);
