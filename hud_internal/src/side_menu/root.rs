//! Root layout for the left-edge construction and panel menu.

use bevy::ecs::template::TemplateContext;
use bevy::prelude::*;
use strum::IntoEnumIterator;

use game_core::prelude::*;

use super::section::{AdminSection, SectionOffering, on_click_open_research_panel, side_menu_section};
use super::tile::PlacementTile;

/// Distance from the window's left edge to the menu column.
pub(crate) const SIDE_MENU_LEFT: f32 = 5.0;

#[derive(Component, Default, Clone)]
pub(crate) struct SideMenu;
impl SideMenu {
    pub(crate) fn setup(mut commands: Commands) {
        commands.spawn_scene(bsn! {
            SideMenu
            Node {
                position_type: PositionType::Absolute,
                top: Val::Percent(30.),
                left: Val::Px(SIDE_MENU_LEFT),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
            }
            Children [
                side_menu_section("ui/side_menu_towers.png", bsn_list![], template_value(SectionOffering::Towers)),
                side_menu_section("ui/side_menu_buildings.png", bsn_list![], template_value(SectionOffering::Buildings)),
                side_menu_section("ui/side_menu_research.png", bsn_list![], bsn!{ on(on_click_open_research_panel) }),
                side_menu_section("ui/side_menu_upgrades.png", bsn_list![], bsn!{}),
                side_menu_section("ui/side_menu_consumables.png", bsn_list![], bsn!{}),
                side_menu_section("ui/side_menu_admin_objects.png", bsn_list![
                    template(|_: &mut TemplateContext| Ok(PlacementTile(MapObject::DarkOre))),
                    template(|_: &mut TemplateContext| Ok(PlacementTile(MapObject::Wall))),
                    template(|_: &mut TemplateContext| Ok(PlacementTile(MapObject::QuantumField))),
                ], bsn!{ AdminSection }),
                side_menu_section("ui/side_menu_admin_wisps.png", WispType::iter()
                    .map(|wisp_type| template(move |_: &mut TemplateContext| Ok(PlacementTile(MapObject::Wisp(wisp_type)))))
                    .collect::<Vec<_>>(), bsn!{ AdminSection }),
            ]
        });
    }
}
