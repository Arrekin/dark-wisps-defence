use bevy::prelude::*;
use bevy::ui::UiGlobalTransform;

use widgets::prelude::{BuilderTooltip, TooltipLeftLimit, TooltipOf, TooltipOffsetAbove, Tooltips};

pub struct TooltipPlugin;
impl Plugin for TooltipPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_observer(on_builder_add_spawn_tooltip)
            .add_observer(on_add_tooltip_of_watch_anchor_hover)
            .add_systems(Update, position_tooltips);
    }
}

/// Applies the relationship, the layout and the surface the builder carries.
fn on_builder_add_spawn_tooltip(
    trigger: On<Add, BuilderTooltip>,
    mut commands: Commands,
    builders: Query<&BuilderTooltip>,
) {
    let entity = trigger.entity;
    let Ok(builder) = builders.get(entity) else { return };

    commands.entity(entity)
        .remove::<BuilderTooltip>()
        .insert((
            TooltipOf(builder.anchor),
            builder.left_limit,
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                width: builder.width,
                max_width: builder.max_width,
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(builder.row_gap),
                padding: builder.padding,
                ..default()
            },
            builder.void_panel,
        ));
}

fn on_add_tooltip_of_watch_anchor_hover(
    trigger: On<Add, TooltipOf>,
    mut commands: Commands,
    tooltips: Query<&TooltipOf>,
) {
    let tooltip_entity = trigger.entity;
    let Ok(tooltip_of) = tooltips.get(tooltip_entity) else { return };
    let parent_entity = tooltip_of.0;

    // The tooltip remains a free root to avoid clipping by the anchor's ancestors.
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
            // Layout updates `ComputedNode` one frame after display changes. Keep the tooltip
            // off-screen until `position_tooltips` receives its new size.
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

/// Positions each visible free-root tooltip above its anchor and clamps it to
/// [`TooltipLeftLimit`]. Converts physical `UiGlobalTransform` coordinates to logical `Node`
/// coordinates with `ComputedNode::inverse_scale_factor`.
fn position_tooltips(
    mut tooltips: Query<(Entity, &TooltipOf, &TooltipOffsetAbove, &TooltipLeftLimit, &mut Node)>,
    nodes: Query<(&ComputedNode, &UiGlobalTransform)>,
) {
    for (tooltip_entity, tooltip_of, offset, left_limit, mut tooltip_style) in tooltips.iter_mut() {
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
        // anchor's top edge, centered horizontally on the anchor unless that
        // would take it past its left limit.
        let tooltip_size = tooltip_node.size() * scale;

        // Skip until layout has computed the tooltip's size. On the first frame
        // after `Display::Flex`, the size is still zero — placing the tooltip
        // now would put its top at the anchor's top with no height offset, and
        // once layout runs it would extend downward into the anchor.
        if tooltip_size.y == 0.0 { continue; }

        let tooltip_top_left = Vec2::new(
            (parent_top_center.x - tooltip_size.x * 0.5).max(left_limit.0),
            parent_top_center.y - tooltip_size.y - gap,
        );

        tooltip_style.left = Val::Px(tooltip_top_left.x);
        tooltip_style.top = Val::Px(tooltip_top_left.y);
    }
}
