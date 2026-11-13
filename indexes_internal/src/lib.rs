#![feature(adt_const_params)]

use bevy::app::{App, Plugin};

pub(crate) mod obstacles;
pub(crate) mod tower_ranges;
pub(crate) mod energy_supply;
pub(crate) mod floor_cover;
pub(crate) mod floor_cover_canvas;
pub(crate) mod wisps;
pub(crate) mod emissions;

pub struct IndexesPlugin;
impl Plugin for IndexesPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            obstacles::ObstaclesGridPlugin,
            energy_supply::EnergySupplyPlugin,
            emissions::EmissionsPlugin,
            floor_cover::FloorCoverPlugin,
            floor_cover_canvas::FloorCoverCanvasPlugin,
            wisps::WispsGridPlugin,
            tower_ranges::TowerRangesPlugin,
        ));
    }
}
