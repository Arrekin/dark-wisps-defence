//! Tooltip relationship and automatic hover behavior.
//!
//! Spawn a [`TooltipBundle`] (or a bare [`TooltipOf`] with a custom [`Node`])
//! to give any UI entity a tooltip that shows on hover and hides on leave.
//!
//! Tooltips are free UI root nodes, not children of their anchor, so they are
//! never clipped by the anchor's overflow ancestors. A positioning system in
//! `widgets_internal` places each visible tooltip above its anchor.
//!
//! # Usage
//! ```
//! // Default-styled tooltip — spawn the bundle and fill it with content.
//! let tooltip = commands.spawn(TooltipBundle::new(parent)).id();
//! commands.entity(tooltip).with_children(|tooltip| {
//!     tooltip.spawn((Text::new("Title"), TextFont::from_font_size(12.)));
//! });
//!
//! // Custom-styled tooltip — spawn TooltipOf with your own Node.
//! commands.spawn((TooltipOf(entity), Node { ..default() }));
//! ```

use bevy::prelude::*;

/// Relationship component added to a tooltip, pointing to its anchor entity.
/// Adding it automatically attaches hover observers to the anchor for
/// show/hide behavior.
///
/// The `#[require]`s give every tooltip a sensible default z-index, pickable
/// behavior, and gap — see the attribute below for specifics.
#[derive(Component)]
#[relationship(relationship_target = Tooltips)]
#[require(TooltipOffsetAbove, GlobalZIndex = GlobalZIndex(200), Pickable = Pickable::IGNORE)]
pub struct TooltipOf(pub Entity);

/// Relationship target tracking all tooltips anchored to an entity.
/// `linked_spawn` despawns the tooltips with their anchor, so a tooltip needs
/// no `ChildOf` to its anchor — and must not have one, since a child is
/// clipped by the anchor's overflow ancestors.
#[derive(Component, Default)]
#[relationship_target(relationship = TooltipOf, linked_spawn)]
pub struct Tooltips(Vec<Entity>);

/// Gap in logical pixels between the tooltip's bottom edge and its anchor's
/// top edge. Read by the positioning system in `widgets_internal`;
/// auto-inserted by [`TooltipOf`]'s `#[require]`.
#[derive(Component, Clone, Copy, Debug)]
pub struct TooltipOffsetAbove(pub f32);

impl Default for TooltipOffsetAbove {
    fn default() -> Self { Self(DEFAULT_TOOLTIP_GAP) }
}

/// Default gap between a tooltip and its anchor.
const DEFAULT_TOOLTIP_GAP: f32 = 4.0;

// Default tooltip styling, offered via [`TooltipBundle`]. A caller that wants
// a different look spawns [`TooltipOf`] with a custom [`Node`] instead.
const TOOLTIP_BACKGROUND: Color = Color::linear_rgba(0.1, 0.1, 0.2, 0.95);
const TOOLTIP_MAX_WIDTH: f32 = 220.0;
const TOOLTIP_PADDING: f32 = 6.0;
const TOOLTIP_ROW_GAP: f32 = 2.0;

/// A pre-styled tooltip bundle. Spawn it, then add children for the content.
///
/// [`TooltipOf`] auto-requires [`TooltipOffsetAbove`], [`GlobalZIndex`], and
/// [`Pickable`], so the bundle spells none of them — override any by
/// inserting the component after spawning.
#[derive(Bundle)]
pub struct TooltipBundle {
    pub tooltip_of: TooltipOf,
    pub node: Node,
    pub background_color: BackgroundColor,
}

impl TooltipBundle {
    pub fn new(parent: Entity) -> Self {
        Self {
            tooltip_of: TooltipOf(parent),
            node: Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                width: Val::Px(TOOLTIP_MAX_WIDTH),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(TOOLTIP_ROW_GAP),
                padding: UiRect::all(Val::Px(TOOLTIP_PADDING)),
                ..default()
            },
            background_color: BackgroundColor::from(TOOLTIP_BACKGROUND),
        }
    }
}
