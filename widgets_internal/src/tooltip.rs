use bevy::prelude::*;
use widgets::prelude::{TooltipOf, Tooltips};

pub struct TooltipPlugin;
impl Plugin for TooltipPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(on_add_spawn_tooltip);
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

    // Add UI parent-child relationship for proper layout
    commands.entity(tooltip_entity).insert(ChildOf(parent_entity));

    // Attach hover observers to the parent entity
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
