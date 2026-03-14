pub mod obstacles;
pub mod base;
pub mod wisps;
pub mod visited;
pub mod emissions;
pub mod energy_supply;
pub mod tower_ranges;
pub mod force_fields;

use crate::lib_prelude::*;

pub struct GridsPlugin;
impl Plugin for GridsPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                emissions::EmissionsPlugin,
                energy_supply::EnergySupplyPlugin,
                obstacles::ObstaclesGridPlugin,
                wisps::WispsGridPlugin,
                tower_ranges::TowerRangesPlugin,
                force_fields::ForceFieldGridPlugin,
            ));
    }
}