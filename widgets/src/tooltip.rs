//! Tooltip relationship and automatic hover behavior.
//!
//! Spawn a [`BuilderTooltip`] to give any UI entity a tooltip that shows on hover and hides on leave.
//!
//! Tooltips are free UI root nodes, not children of their anchor, so they are
//! never clipped by the anchor's overflow ancestors. A positioning system in
//! `widgets_internal` places each visible tooltip above its anchor.
//!
//! # Usage
//! ```
//! commands.spawn((
//!     BuilderTooltip::new(anchor),
//!     children![(Text::new("Title"), TextFont::from_font_size(12.))],
//! ));
//! ```

use bevy::prelude::*;

use crate::void_panel::BuilderVoidPanel;

/// Links a tooltip to its anchor and installs the anchor's show/hide observers.
///
/// Required components keep tooltips above ordinary UI, outside pointer hit-testing, and positioned
/// with default gap and boundary values.
#[derive(Component)]
#[relationship(relationship_target = Tooltips)]
#[require(TooltipOffsetAbove, TooltipLeftLimit, GlobalZIndex = GlobalZIndex(200), Pickable = Pickable::IGNORE)]
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

/// Minimum tooltip x-coordinate in logical pixels. Anchor centering is clamped to this value.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct TooltipLeftLimit(pub f32);

// Layout of a default-styled tooltip.
const TOOLTIP_WIDTH: f32 = 220.0;
const TOOLTIP_PADDING: f32 = 6.0;
const TOOLTIP_ROW_GAP: f32 = 2.0;

const TOOLTIP_CORNER_CUT: f32 = 0.0;
/// Keeps the darkest contour facet distinct from the tooltip field.
const TOOLTIP_EDGE_BRIGHTNESS: f32 = 1.7;
/// Disables directional shading that would split the bright contour into visibly different colors.
const TOOLTIP_CONTOUR_LIGHT_RANGE: f32 = 0.0;
/// Tooltip field center: #142149.
const TOOLTIP_FIELD_CENTER: Color = Color::srgb_u8(0x14, 0x21, 0x49);
/// Tooltip field edge: #080E1C.
const TOOLTIP_FIELD_EDGE: Color = Color::srgb_u8(0x08, 0x0E, 0x1C);

/// Spawn contract for a styled tooltip. Content is supplied as children; the observer replaces
/// this builder with the relationship, layout, and panel surface.
///
/// The default fixed width gives wrapping text a stable measurement. Use
/// [`BuilderTooltip::sized_to_content`] only for single-line content.
#[derive(Component, Clone, Debug)]
pub struct BuilderTooltip {
    /// The entity the tooltip describes and shows on hover of.
    pub anchor: Entity,
    pub width: Val,
    pub max_width: Val,
    pub left_limit: TooltipLeftLimit,
    pub padding: UiRect,
    pub row_gap: f32,
    pub void_panel: BuilderVoidPanel,
}

impl BuilderTooltip {
    pub fn new(anchor: Entity) -> Self {
        Self {
            anchor,
            width: Val::Px(TOOLTIP_WIDTH),
            max_width: Val::Px(TOOLTIP_WIDTH),
            left_limit: TooltipLeftLimit::default(),
            padding: UiRect::all(Val::Px(TOOLTIP_PADDING)),
            row_gap: TOOLTIP_ROW_GAP,
            void_panel: BuilderVoidPanel::default()
                .with_background_center(TOOLTIP_FIELD_CENTER)
                .with_background_edge(TOOLTIP_FIELD_EDGE)
                .with_corner_cut(TOOLTIP_CORNER_CUT)
                .with_edge_brightness(TOOLTIP_EDGE_BRIGHTNESS)
                .with_contour_light_range(TOOLTIP_CONTOUR_LIGHT_RANGE),
        }
    }

    /// Shrinks single-line content up to [`TOOLTIP_WIDTH`]. Auto-width text is measured as one
    /// line, so content that wraps at the maximum width receives an incorrect height.
    pub fn sized_to_content(mut self) -> Self {
        self.width = Val::Auto;
        self
    }

    /// Chamfer depth on the surface.
    pub fn with_corner_cut(mut self, cut: f32) -> Self {
        self.void_panel = self.void_panel.with_corner_cut(cut);
        self
    }

    /// Leftmost window x the tooltip may occupy. See [`TooltipLeftLimit`].
    pub fn with_left_limit(mut self, left_limit: f32) -> Self {
        self.left_limit = TooltipLeftLimit(left_limit);
        self
    }

    /// Space between the tooltip's edge and its content. A tooltip whose children carry their own
    /// edges passes [`UiRect::ZERO`], so a child can span the full width.
    pub fn with_padding(mut self, padding: UiRect) -> Self {
        self.padding = padding;
        self
    }

    /// Space between one child and the next.
    pub fn with_row_gap(mut self, row_gap: f32) -> Self {
        self.row_gap = row_gap;
        self
    }
}
