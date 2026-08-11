#![feature(adt_const_params)]

use bevy::app::{App, Plugin};

pub(crate) mod obstacles;
pub(crate) mod tower_ranges;
pub(crate) mod energy_supply;
pub(crate) mod wisps;
pub(crate) mod force_fields;
pub(crate) mod emissions;
pub(crate) mod transform_sync;
pub(crate) mod placement;

pub struct GridsPlugin;
impl Plugin for GridsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            obstacles::ObstaclesGridPlugin,
            energy_supply::EnergySupplyPlugin,
            emissions::EmissionsPlugin,
            wisps::WispsGridPlugin,
            tower_ranges::TowerRangesPlugin,
            force_fields::ForceFieldGridPlugin,
            transform_sync::GridTransformSyncPlugin,
            placement::GridObjectPlacerPlugin,
        ));
    }
}
