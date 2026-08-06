use bevy::prelude::*;
use bevy::ui::UiGlobalTransform;

use widgets::prelude::{TooltipOf, TooltipOffsetAbove, Tooltips};

pub struct TooltipPlugin;
impl Plugin for TooltipPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_observer(on_add_spawn_tooltip)
            .add_systems(Update, position_tooltips);
    }
}

fn on_add_spawn_tooltip(
    trigger: On<Add, TooltipOf>,
    mut commands: Commands,
    tooltips: Query<&TooltipOf>,
) {
    let tooltip_entity = trigger.entity;
    let Ok(tooltip_of) = tooltips.get(tooltip_entity) else { return };
    let parent_entity = tooltip_of.0;

    // Hover observers go on the anchor entity, not the tooltip. The tooltip
    // is a free root node (no `ChildOf` to the anchor), so it is never clipped
    // by the anchor's overflow ancestors.
    commands.entity(parent_entity)
        .observe(on_tooltip_parent_hover_start_show_tooltips)
        .observe(on_tooltip_parent_hover_end_hide_tooltips);
}

fn on_tooltip_parent_hover_start_show_tooltips(
    trigger: On<Pointer<Over>>,
    parents: Query<&Tooltips>,
    mut tooltip_nodes: Query<&mut Node, With<TooltipOf>>,
) {
    let Ok(tooltips) = parents.get(trigger.entity) else { return };
    for tooltip_entity in tooltips.iter() {
        if let Ok(mut node) = tooltip_nodes.get_mut(tooltip_entity) {
            // Park off-screen until `position_tooltips` places it. The
            // tooltip's `ComputedNode` size is stale for one frame after
            // `Display::Flex` (layout hasn't run yet), so positioning would
            // place it incorrectly — potentially overlapping the anchor and
            // stealing the hover that just showed it.
            node.left = Val::Px(-10000.0);
            node.top = Val::Px(-10000.0);
            node.display = Display::Flex;
        }
    }
}

fn on_tooltip_parent_hover_end_hide_tooltips(
    trigger: On<Pointer<Out>>,
    parents: Query<&Tooltips>,
    mut tooltip_nodes: Query<&mut Node, With<TooltipOf>>,
) {
    let Ok(tooltips) = parents.get(trigger.entity) else { return };
    for tooltip_entity in tooltips.iter() {
        if let Ok(mut node) = tooltip_nodes.get_mut(tooltip_entity) {
            node.display = Display::None;
        }
    }
}

/// Positions each visible tooltip above its anchor.
///
/// Tooltips are free root nodes with `PositionType::Absolute`, so they have no
/// parent layout to inherit a position from — this system is the only thing
/// that places them.
///
/// `UiGlobalTransform` stores the node's center in physical pixels. `Node`
/// style values are in logical pixels, so the physical coordinates are
/// converted via `ComputedNode::inverse_scale_factor`
/// (`1 / (window_scale * ui_scale)`).
fn position_tooltips(
    mut tooltips: Query<(Entity, &TooltipOf, &TooltipOffsetAbove, &mut Node)>,
    nodes: Query<(&ComputedNode, &UiGlobalTransform)>,
) {
    for (tooltip_entity, tooltip_of, offset, mut tooltip_style) in tooltips.iter_mut() {
        if tooltip_style.display == Display::None { continue; }
        let Ok((parent_node, parent_transform)) = nodes.get(tooltip_of.0) else { continue; };
        let Ok((tooltip_node, _)) = nodes.get(tooltip_entity) else { continue; };

        let gap = offset.0;

        // Anchor's top-center in physical pixels. The transform is centered on
        // the node, so offset by -half_height to reach the top edge.
        let parent_half = parent_node.size() * 0.5;
        let parent_top_center_physical = parent_transform.transform_point2(Vec2::new(0.0, -parent_half.y));

        // Convert to logical pixels for `Node` style values.
        let scale = parent_node.inverse_scale_factor();
        let parent_top_center = parent_top_center_physical * scale;

        // Place the tooltip so its bottom edge sits `gap` pixels above the
        // anchor's top edge, centered horizontally on the anchor.
        let tooltip_size = tooltip_node.size() * scale;

        // Skip until layout has computed the tooltip's size. On the first frame
        // after `Display::Flex`, the size is still zero — placing the tooltip
        // now would put its top at the anchor's top with no height offset, and
        // once layout runs it would extend downward into the anchor.
        if tooltip_size.y == 0.0 { continue; }

        let tooltip_top_left = Vec2::new(
            parent_top_center.x - tooltip_size.x * 0.5,
            parent_top_center.y - tooltip_size.y - gap,
        );

        tooltip_style.left = Val::Px(tooltip_top_left.x);
        tooltip_style.top = Val::Px(tooltip_top_left.y);
    }
}
