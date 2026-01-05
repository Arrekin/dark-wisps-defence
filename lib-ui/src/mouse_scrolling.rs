//! UI Mouse Scrolling System
//!
//! This module implements mouse wheel scrolling for Bevy UI nodes. Bevy's UI system
//! provides the building blocks for scrolling (`Overflow::scroll_y()`, `ScrollPosition`),
//! but does not include built-in mouse wheel handling - that must be implemented manually.
//!
//! # Architecture Overview
//!
//! The scrolling system works in two phases:
//!
//! 1. **Event Gathering** (`UiScrollEvent::gather`):
//!    - Listens to raw `MouseWheel` events from Bevy's input system
//!    - Converts scroll delta to logical pixels (handling both pixel and line-based input)
//!    - Uses Bevy's `HoverMap` to find which UI entities are under the cursor
//!    - Triggers a `UiScrollEvent` on each hovered entity
//!
//! 2. **Event Propagation & Application** (`UiScrollEvent::apply`):
//!    - The event propagates up the UI hierarchy (child → parent) via Bevy's observer system
//!    - Each node with `ScrollPosition` + `Overflow::scroll_*()` can consume part of the delta
//!    - Propagation stops when the entire scroll delta is consumed
//!
//! # How to Make a Node Scrollable
//!
//! To create a scrollable container, you need:
//!
//! ```rust,ignore
//! commands.spawn((
//!     Node {
//!         overflow: Overflow::scroll_y(),  // Enable vertical scrolling (clips + allows scroll)
//!         max_height: Val::Px(200.),       // Limit container height so content can overflow
//!         ..default()
//!     },
//!     ScrollPosition::default(),           // Required! Tracks current scroll offset
//! ));
//! ```
//!
//! # Key Bevy Concepts Used
//!
//! - **`HoverMap`**: A resource provided by `bevy_picking` that tracks which entities
//!   are currently under each pointer (mouse cursor). We use this to know where to
//!   send scroll events.
//!
//! - **`MouseWheel`**: Bevy's raw mouse wheel event. Contains X/Y delta and unit type.
//!   The unit can be `Pixel` (trackpads) or `Line` (traditional scroll wheels).
//!
//! - **`ScrollPosition`**: A Bevy UI component that stores the current scroll offset (x, y).
//!   The UI rendering system uses this to offset child content.
//!
//! - **`ComputedNode`**: Contains the actual computed size of a node after layout.
//!   We use `content_size()` (total size of children) vs `size()` (visible area) to
//!   calculate the maximum scroll offset.
//!
//! - **`EntityEvent` with `propagate` + `auto_propagate`**: Makes the event bubble up
//!   through the entity hierarchy automatically. The `propagate(false)` call stops it.
//!
//! # Event Propagation Flow
//!
//! When you scroll over a deeply nested scrollable element:
//!
//! ```text
//! MouseWheel event
//!     ↓
//! gather() finds hovered entities via HoverMap
//!     ↓
//! UiScrollEvent triggered on innermost hovered entity
//!     ↓
//! apply() runs on that entity:
//!   - If scrollable and not at limit → consumes delta, stops propagation
//!   - If not scrollable or at limit → event propagates to parent
//!     ↓
//! Parent's apply() runs (repeat until delta consumed or root reached)
//! ```
//!
//! This allows nested scrollable containers to work correctly - inner containers
//! scroll first, and only when they hit their limits does the outer container scroll.

use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::picking::hover::HoverMap;

use crate::lib_prelude::*;

pub struct MouseScrollingPlugin;
impl Plugin for MouseScrollingPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Update, UiScrollEvent::gather)
            .add_observer(UiScrollEvent::apply)
            ;
    }
}

/// Approximate height of one "line" of text in pixels.
/// Used to convert line-based scroll units (from traditional scroll wheels)
/// into pixel deltas. Most mice report scroll in lines, not pixels.
const SCROLL_LINE_HEIGHT: f32 = 21.;

/// Internal event that carries scroll intent through the UI hierarchy.
///
/// This event is triggered on UI entities under the mouse cursor when the
/// scroll wheel moves. It propagates upward through the hierarchy until
/// a scrollable node consumes the delta or the root is reached.
///
/// # Propagation Attributes
///
/// - `propagate`: Enables event bubbling through the entity hierarchy
/// - `auto_propagate`: Automatically propagates unless explicitly stopped
///
/// The `entity` field is required by Bevy's `EntityEvent` derive - it specifies
/// which entity the event is targeting/triggered on.
#[derive(EntityEvent, Debug)]
#[entity_event(propagate, auto_propagate)]
struct UiScrollEvent {
    entity: Entity,
    /// Scroll delta in logical pixels. Positive Y = scroll down (content moves up).
    /// This is mutable during propagation - handlers consume portions of it.
    delta: Vec2,
}

impl UiScrollEvent {
    /// Converts raw mouse wheel input into UI scroll events.
    ///
    /// This system runs every frame and:
    /// 1. Reads all pending `MouseWheel` events
    /// 2. Converts the scroll delta to logical pixels
    /// 3. Finds all UI entities currently under the cursor via `HoverMap`
    /// 4. Triggers a `UiScrollEvent` on each hovered entity
    ///
    /// # Why Negative Delta?
    ///
    /// Mouse wheel "up" (positive Y) conventionally means "scroll content up",
    /// but in scroll position terms that means *decreasing* the offset.
    /// We negate the delta so positive scroll delta = positive position change.
    fn gather(
        mut mouse_wheel_reader: MessageReader<MouseWheel>,
        hover_map: Res<HoverMap>,
        mut commands: Commands,
    ) {
        for mouse_wheel in mouse_wheel_reader.read() {
            // Negate: wheel up → scroll up → decrease scroll position
            let mut delta = -Vec2::new(mouse_wheel.x, mouse_wheel.y);
            
            // Convert line units to pixels (traditional scroll wheels report in lines)
            if mouse_wheel.unit == MouseScrollUnit::Line {
                delta *= SCROLL_LINE_HEIGHT;
            }
            
            // HoverMap contains: pointer_id → (entity → HitData)
            // We iterate all pointers (usually just one mouse) and all hovered entities
            for pointer_map in hover_map.values() {
                for entity in pointer_map.keys().copied() {
                    commands.trigger(UiScrollEvent { entity, delta });
                }
            }
        }
    }

    /// Handles scroll events on individual nodes, consuming delta if scrollable.
    ///
    /// This observer is called for each entity as the event propagates up the
    /// hierarchy. It checks if the current entity is scrollable and, if so,
    /// applies the scroll delta to its `ScrollPosition`.
    ///
    /// # Scroll Position Calculation
    ///
    /// - `content_size()`: Total size of all children (may exceed visible area)
    /// - `size()`: Visible size of this node (the "viewport")
    /// - `max_offset = content_size - size`: How far we can scroll before hitting the end
    /// - `inverse_scale_factor()`: Converts physical pixels to logical pixels
    ///
    /// # Propagation Control
    ///
    /// - If we consume any delta, we zero that component
    /// - If all delta is consumed (`delta == Vec2::ZERO`), we stop propagation
    /// - If we're at the scroll limit, delta passes through to parent
    fn apply(
        mut scroll: On<UiScrollEvent>,
        mut query: Query<(&mut ScrollPosition, &Node, &ComputedNode)>,
    ) {
        let Ok((mut scroll_position, node, computed)) = query.get_mut(scroll.entity) else {
            return; // Entity doesn't have scroll components - event propagates to parent
        };
        
        // Calculate maximum scroll offset (how far content extends beyond viewport)
        let max_offset = (computed.content_size() - computed.size()) * computed.inverse_scale_factor();
        let delta = &mut scroll.delta;
        
        // Handle vertical scrolling
        if node.overflow.y == OverflowAxis::Scroll && delta.y != 0. {
            // Check if we're already at the scroll limit in the direction of travel
            let at_limit = if delta.y > 0. {
                scroll_position.y >= max_offset.y  // Scrolling down, already at bottom
            } else {
                scroll_position.y <= 0.            // Scrolling up, already at top
            };
            
            if !at_limit {
                // Apply scroll and clamp to valid range
                scroll_position.y = (scroll_position.y + delta.y).clamp(0., max_offset.y.max(0.));
                // Mark Y delta as consumed
                delta.y = 0.;
            }
            // If at limit, delta.y remains non-zero and propagates to parent
        }
        
        // Handle horizontal scrolling (same logic as vertical)
        if node.overflow.x == OverflowAxis::Scroll && delta.x != 0. {
            let at_limit = if delta.x > 0. {
                scroll_position.x >= max_offset.x
            } else {
                scroll_position.x <= 0.
            };
            
            if !at_limit {
                scroll_position.x = (scroll_position.x + delta.x).clamp(0., max_offset.x.max(0.));
                delta.x = 0.;
            }
        }
        
        // Stop propagation only when ALL scroll delta has been consumed.
        // This allows unconsumed delta (e.g., horizontal scroll on vertical-only container)
        // to propagate to parent containers that might handle it.
        if *delta == Vec2::ZERO {
            scroll.propagate(false);
        }
    }
}