//! Root layout for the left-edge construction and panel menu.

use bevy::prelude::*;
use strum::IntoEnumIterator;

use game_core::prelude::*;

use super::{
    section::{AdminSection, SectionOffering, SideMenuSection, on_click_open_forging_panel, on_click_open_research_panel},
    tile::PlacementTile,
};

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
                @SideMenuSection::scene("ui/side_menu_towers.png", bsn_list!{}, bsn!{ SectionOffering::Towers })
                --
                @SideMenuSection::scene("ui/side_menu_buildings.png", bsn_list!{}, bsn!{ SectionOffering::Buildings })
                --
                @SideMenuSection::scene("ui/side_menu_research.png", bsn_list!{}, bsn!{ on(on_click_open_research_panel) })
                --
                @SideMenuSection::scene("ui/side_menu_forging.png", bsn_list!{}, bsn!{ on(on_click_open_forging_panel) })
                --
                @SideMenuSection::scene("ui/side_menu_consumables.png", bsn_list!{}, bsn!{})
                --
                @SideMenuSection::scene("ui/side_menu_admin_objects.png", bsn_list!{
                    ~{PlacementTile(MapObject::DarkOre)}
                    --
                    ~{PlacementTile(MapObject::Wall)}
                    --
                    ~{PlacementTile(MapObject::QuantumField)}
                }, bsn!{ AdminSection })
                --
                @SideMenuSection::scene("ui/side_menu_admin_wisps.png", WispType::iter()
                    .map(|wisp_type| bsn!{ ~{PlacementTile(MapObject::Wisp(wisp_type))} })
                    .collect::<Vec<_>>(), bsn!{ AdminSection })
            ]
        });
    }
}
