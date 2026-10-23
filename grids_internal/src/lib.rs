#![feature(adt_const_params)]

use bevy::app::{App, Plugin};

pub(crate) mod obstacles;
pub(crate) mod tower_ranges;
pub(crate) mod energy_supply;
pub(crate) mod ground_cover;
pub(crate) mod ground_cover_canvas;
pub(crate) mod wisps;
pub(crate) mod emissions;
pub(crate) mod placement;

pub struct GridsPlugin;
impl Plugin for GridsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            obstacles::ObstaclesGridPlugin,
            energy_supply::EnergySupplyPlugin,
            emissions::EmissionsPlugin,
            ground_cover::GroundCoverPlugin,
            ground_cover_canvas::GroundCoverCanvasPlugin,
            wisps::WispsGridPlugin,
            tower_ranges::TowerRangesPlugin,
            placement::GridObjectPlacerPlugin,
        ));
    }
}
