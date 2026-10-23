//! The tooltip a building's side-menu tile shows on hover.

use bevy::prelude::*;

use almanach::prelude::*;
use game_core::prelude::{MapObject, MapObjectTooltipRequest};
use hud::prelude::BuilderSideMenuItemTooltip;

pub(crate) struct BuildingTooltipPlugin;
impl Plugin for BuildingTooltipPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(on_map_object_tooltip_request_spawn_building_tooltip);
    }
}

fn on_map_object_tooltip_request_spawn_building_tooltip(
    trigger: On<MapObjectTooltipRequest>,
    mut commands: Commands,
    almanach: Res<Almanach>,
) {
    let MapObjectTooltipRequest { entity, object: MapObject::Building(building_type) } = *trigger else { return };

    let info = almanach.get_building_info(building_type);
    let mut tooltip = BuilderSideMenuItemTooltip::new(entity)
        .with_name(info.name.clone())
        .with_description(info.description.clone())
        .with_fact(info.grid_imprint.label())
        .with_cost(info.cost.clone());

    if building_type.is_energy_consumer() {
        tooltip = tooltip.with_fact("Needs power");
    }

    commands.spawn(tooltip);
}
