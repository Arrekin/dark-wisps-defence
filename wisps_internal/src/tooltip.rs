//! The tooltip a wisp's side-menu tile shows on hover.

use bevy::prelude::*;

use almanach::prelude::*;
use game_core::prelude::MapObject;
use hud::prelude::BuilderSideMenuItemTooltip;
use wisps::prelude::BuilderWispSideMenuTooltip;

pub(crate) struct WispTooltipPlugin;
impl Plugin for WispTooltipPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(on_builder_add_spawn_wisp_tooltip);
    }
}

/// Queues tooltip construction for a wisp placement tile.
pub(crate) fn wisp_tooltip(commands: &mut Commands, anchor: Entity, map_object: MapObject) {
    let MapObject::Wisp(wisp_type) = map_object else { return };
    commands.spawn(BuilderWispSideMenuTooltip { anchor, wisp_type });
}

fn on_builder_add_spawn_wisp_tooltip(
    trigger: On<Add, BuilderWispSideMenuTooltip>,
    mut commands: Commands,
    almanach: Res<Almanach>,
    builders: Query<&BuilderWispSideMenuTooltip>,
) {
    let entity = trigger.entity;
    let Ok(builder) = builders.get(entity) else { return; };

    let info = &almanach.wisps;
    commands.entity(entity)
        .remove::<BuilderWispSideMenuTooltip>()
        .insert(
            BuilderSideMenuItemTooltip::new(builder.anchor)
                .with_name(format!("{} Wisp", builder.wisp_type.as_ref()))
                .with_description(info.description.clone())
                .with_fact(info.grid_imprint.label()),
        );
}
