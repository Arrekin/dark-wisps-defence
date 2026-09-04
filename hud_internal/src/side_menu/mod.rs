//! Side-menu plugin and implementation modules.

use bevy::prelude::*;

use states::{AdminMode, MapLoadingStage};

pub(crate) mod root;
pub(crate) mod section;
pub(crate) mod strip;
pub(crate) mod tile;
pub(crate) mod tooltip;

pub struct SideMenuPlugin;
impl Plugin for SideMenuPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Startup, (
                root::SideMenu::setup,
            ))
            .add_systems(Update, (
                section::AdminSection::on_admin_mode_change_update_visibility.run_if(state_changed::<AdminMode>),
                strip::trigger_offering_changed.run_if(state_changed::<AdminMode>),
            ))
            .add_systems(OnEnter(MapLoadingStage::Ready), strip::trigger_offering_changed)
            .add_observer(section::on_insert_section_state_manage_strip)
            .add_observer(section::on_click_section_cancel_placement)
            .add_observer(section::on_start_placing_latch_owning_section)
            .add_observer(section::on_stop_placing_release_owning_section)
            .add_observer(strip::on_offering_changed_refill_strips)
            .add_observer(tile::on_add_tile_build)
            .add_observer(tile::on_add_placement_tile_watch_click)
            .add_observer(tile::on_add_tile_children_apply_presentation)
            .add_observer(tooltip::on_builder_add_spawn_side_menu_item_tooltip);
    }
}
