//! The tooltip a building's side-menu tile shows on hover.

use bevy::prelude::*;

use almanach::prelude::*;
use buildings::prelude::BuilderBuildingSideMenuTooltip;
use game_core::prelude::MapObject;
use hud::prelude::BuilderSideMenuItemTooltip;

pub(crate) struct BuildingTooltipPlugin;
impl Plugin for BuildingTooltipPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(on_builder_add_spawn_building_tooltip);
    }
}

/// Queues tooltip construction for a building placement tile.
pub(crate) fn building_tooltip(commands: &mut Commands, anchor: Entity, map_object: MapObject) {
    let MapObject::Building(building_type) = map_object else { return };
    commands.spawn(BuilderBuildingSideMenuTooltip { anchor, building_type });
}

fn on_builder_add_spawn_building_tooltip(
    trigger: On<Add, BuilderBuildingSideMenuTooltip>,
    mut commands: Commands,
    almanach: Res<Almanach>,
    builders: Query<&BuilderBuildingSideMenuTooltip>,
) {
    let entity = trigger.entity;
    let Ok(builder) = builders.get(entity) else { return; };

    let info = almanach.get_building_info(builder.building_type);
    let mut tooltip = BuilderSideMenuItemTooltip::new(builder.anchor)
        .with_name(info.name.clone())
        .with_description(info.description.clone())
        .with_fact(info.grid_imprint.label())
        .with_cost(info.cost.clone());

    if builder.building_type.is_energy_consumer() {
        tooltip = tooltip.with_fact("Needs power");
    }

    commands.entity(entity)
        .remove::<BuilderBuildingSideMenuTooltip>()
        .insert(tooltip);
}
