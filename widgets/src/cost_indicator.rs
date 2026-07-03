use bevy::prelude::*;
use resources::prelude::{Cost, ResourceType};

#[derive(Component)]
#[require(Node)]
pub struct CostIndicator {
    pub cost: Cost,
    pub has_required_resources: bool,
    pub font_size: f32,
    pub font_color: Color,
}
impl Default for CostIndicator {
    fn default() -> Self {
        Self {
            cost: Cost {
                resource_type: ResourceType::DarkOre,
                amount: 0,
            },
            has_required_resources: false,
            font_size: 14.,
            font_color: Color::WHITE,
        }
    }
}
impl From<Cost> for CostIndicator {
    fn from(cost: Cost) -> Self {
        Self {
            cost,
            ..default()
        }
    }
}
