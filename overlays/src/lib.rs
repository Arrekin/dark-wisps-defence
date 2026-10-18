pub mod emissions;
pub mod energy_supply;
pub mod towers_range;

use bevy::prelude::*;

pub struct OverlaysPlugin;
impl Plugin for OverlaysPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            towers_range::TowersRangeOverlayPlugin,
            energy_supply::EnergySupplyOverlayPlugin,
            emissions::EmissionsOverlayPlugin,
        ));
    }
}
