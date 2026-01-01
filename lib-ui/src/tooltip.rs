//! Tooltip relationship and automatic hover behavior
//!
//! Use `TooltipOf` to create a tooltip for any UI entity. The tooltip will automatically
//! show on hover and hide when the mouse leaves.
//!
//! # Usage
//! ```
//! commands.entity(parent).with_related::<TooltipOf>(my_tooltip_bundle);
//! ```

use crate::lib_prelude::*;

pub struct TooltipPlugin;
impl Plugin for TooltipPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(TooltipOf::on_add);
    }
}

/// Relationship component added to a tooltip, pointing to its parent entity.
/// When added, automatically attaches hover observers to the parent for show/hide behavior.
#[derive(Component)]
#[relationship(relationship_target = Tooltips)]
pub struct TooltipOf(pub Entity);

/// Relationship target tracking all tooltips for an entity.
#[derive(Component, Default)]
#[relationship_target(relationship = TooltipOf)]
pub struct Tooltips(Vec<Entity>);

impl TooltipOf {
    fn on_add(
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
            .observe(Self::on_parent_hover_start)
            .observe(Self::on_parent_hover_end);
    }
    
    fn on_parent_hover_start(
        trigger: On<Pointer<Over>>,
        parents: Query<&Tooltips>,
        mut tooltip_nodes: Query<&mut Node, With<TooltipOf>>,
    ) {
        let Ok(tooltips) = parents.get(trigger.entity) else { return };
        for &tooltip_entity in tooltips.0.iter() {
            if let Ok(mut node) = tooltip_nodes.get_mut(tooltip_entity) {
                node.display = Display::Flex;
            }
        }
    }
    
    fn on_parent_hover_end(
        trigger: On<Pointer<Out>>,
        parents: Query<&Tooltips>,
        mut tooltip_nodes: Query<&mut Node, With<TooltipOf>>,
    ) {
        let Ok(tooltips) = parents.get(trigger.entity) else { return };
        for &tooltip_entity in tooltips.0.iter() {
            if let Ok(mut node) = tooltip_nodes.get_mut(tooltip_entity) {
                node.display = Display::None;
            }
        }
    }
}
