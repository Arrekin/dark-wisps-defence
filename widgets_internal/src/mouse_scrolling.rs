//! Mouse-wheel scrolling for UI nodes.
//!
//! A node scrolls when it has `ScrollPosition` and `Overflow::scroll_*()` on the axis. Each wheel
//! event is triggered as a `UiScrollEvent` on every hovered entity and bubbles up the hierarchy;
//! a scrollable node consumes the delta on its axes unless it is already at its limit. Nested
//! containers therefore scroll inner-first, and the outer one takes over at the inner one's end.
//! Horizontal-only containers also accept vertical wheel input.

use bevy::{
    input::mouse::{MouseScrollUnit, MouseWheel},
    picking::hover::HoverMap,
    prelude::*,
};

pub(crate) struct MouseScrollingPlugin;
impl Plugin for MouseScrollingPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Update, UiScrollEvent::gather)
            .add_observer(UiScrollEvent::apply);
    }
}

/// Approximate height of one text line, converting line-based wheel input to pixels.
const SCROLL_LINE_HEIGHT: f32 = 21.;

/// Scroll intent bubbling up the UI hierarchy until a scrollable node consumes it.
#[derive(EntityEvent, Debug)]
#[entity_event(propagate, auto_propagate)]
struct UiScrollEvent {
    entity: Entity,
    /// Remaining delta in logical pixels; positive Y scrolls down. Nodes zero the axes they consume.
    delta: Vec2,
}

impl UiScrollEvent {
    /// Triggers a `UiScrollEvent` on every hovered entity for each wheel event. The delta is
    /// negated so that wheel-up decreases the scroll offset.
    fn gather(
        mut commands: Commands,
        hover_map: Res<HoverMap>,
        mut mouse_wheel_reader: MessageReader<MouseWheel>,
    ) {
        for mouse_wheel in mouse_wheel_reader.read() {
            let mut delta = -Vec2::new(mouse_wheel.x, mouse_wheel.y);
            if mouse_wheel.unit == MouseScrollUnit::Line {
                delta *= SCROLL_LINE_HEIGHT;
            }

            for pointer_map in hover_map.values() {
                for entity in pointer_map.keys().copied() {
                    commands.trigger(UiScrollEvent { entity, delta });
                }
            }
        }
    }

    /// Consumes the delta on each axis this node scrolls, unless it is already at its limit in
    /// that direction. Propagation stops once all delta is consumed, so a delta this node
    /// cannot use (at its limit, or on an axis it does not scroll) reaches its ancestors.
    fn apply(
        mut scroll: On<UiScrollEvent>,
        mut scrollables: Query<(&mut ScrollPosition, &Node, &ComputedNode)>,
    ) {
        let Ok((mut scroll_position, node, computed)) = scrollables.get_mut(scroll.entity) else { return };

        // How far the content extends beyond the visible area, in logical pixels.
        let max_offset = (computed.content_size() - computed.size()) * computed.inverse_scale_factor();
        let delta = &mut scroll.delta;

        if node.overflow.y == OverflowAxis::Scroll && delta.y != 0. {
            let at_limit = if delta.y > 0. {
                scroll_position.y >= max_offset.y
            } else {
                scroll_position.y <= 0.
            };
            if !at_limit {
                scroll_position.y = (scroll_position.y + delta.y).clamp(0., max_offset.y.max(0.));
                delta.y = 0.;
            }
        }

        // Use the dominant axis so both vertical mouse wheels and horizontal gestures scroll this row.
        let horizontal_only = node.overflow.x == OverflowAxis::Scroll && node.overflow.y != OverflowAxis::Scroll;
        let delta_x = if horizontal_only && delta.y.abs() > delta.x.abs() { delta.y } else { delta.x };
        if node.overflow.x == OverflowAxis::Scroll && delta_x != 0. {
            let at_limit = if delta_x > 0. {
                scroll_position.x >= max_offset.x
            } else {
                scroll_position.x <= 0.
            };
            if !at_limit {
                scroll_position.x = (scroll_position.x + delta_x).clamp(0., max_offset.x.max(0.));
                delta.x = 0.;
                if horizontal_only {
                    delta.y = 0.;
                }
            }
        }

        if *delta == Vec2::ZERO {
            scroll.propagate(false);
        }
    }
}
