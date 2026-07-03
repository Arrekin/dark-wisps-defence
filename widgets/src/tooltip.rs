//! Tooltip relationship and automatic hover behavior
//!
//! Use `TooltipOf` to create a tooltip for any UI entity. The tooltip will automatically
//! show on hover and hide when the mouse leaves.
//!
//! # Usage
//! ```
//! commands.entity(parent).with_related::<TooltipOf>(my_tooltip_bundle);
//! ```

use bevy::prelude::*;

/// Relationship component added to a tooltip, pointing to its parent entity.
/// When added, automatically attaches hover observers to the parent for show/hide behavior.
#[derive(Component)]
#[relationship(relationship_target = Tooltips)]
pub struct TooltipOf(pub Entity);

/// Relationship target tracking all tooltips for an entity.
#[derive(Component, Default)]
#[relationship_target(relationship = TooltipOf)]
pub struct Tooltips(Vec<Entity>);
