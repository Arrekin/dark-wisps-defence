//! The tooltip a wisp's side-menu tile shows on hover.

use bevy::prelude::*;

use almanach::prelude::*;
use game_core::prelude::{MapObject, MapObjectTooltipRequest};
use hud::prelude::BuilderSideMenuItemTooltip;

pub(crate) struct WispTooltipPlugin;
impl Plugin for WispTooltipPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(on_map_object_tooltip_request_spawn_wisp_tooltip);
    }
}

fn on_map_object_tooltip_request_spawn_wisp_tooltip(
    trigger: On<MapObjectTooltipRequest>,
    mut commands: Commands,
    almanach: Res<Almanach>,
) {
    let MapObjectTooltipRequest { entity, object: MapObject::Wisp(wisp_type) } = *trigger else { return };

    let info = &almanach.wisps;
    commands.spawn(
        BuilderSideMenuItemTooltip::new(entity)
            .with_name(format!("{} Wisp", wisp_type.as_ref()))
            .with_description(info.description.clone())
            .with_fact(info.grid_imprint.label()),
    );
}
