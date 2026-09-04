//! The tooltip a side-menu tile shows on hover.

use bevy::prelude::*;

use resources::prelude::Cost;

/// Spawn request for a side-menu tile tooltip.
///
/// Optional and empty fields omit their corresponding rows.
#[derive(Component, Clone, Debug)]
pub struct BuilderSideMenuItemTooltip {
    /// The tile this tooltip describes and anchors to.
    pub anchor: Entity,
    pub name: Option<String>,
    pub description: Option<String>,
    /// Short statements, shown as one row.
    pub facts: Vec<String>,
    pub cost: Vec<Cost>,
}

impl BuilderSideMenuItemTooltip {
    pub fn new(anchor: Entity) -> Self {
        Self {
            anchor,
            name: None,
            description: None,
            facts: Vec::new(),
            cost: Vec::new(),
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Adds one statement to the facts row.
    pub fn with_fact(mut self, fact: impl Into<String>) -> Self {
        self.facts.push(fact.into());
        self
    }

    pub fn with_cost(mut self, cost: Vec<Cost>) -> Self {
        self.cost = cost;
        self
    }
}
