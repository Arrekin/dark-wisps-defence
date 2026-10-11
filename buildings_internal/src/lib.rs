pub(crate) mod main_base;
pub(crate) mod common;
pub(crate) mod common_systems;
pub(crate) mod energy_relay;
pub(crate) mod mining_complex;
pub(crate) mod exploration_center;
pub(crate) mod forge;
pub(crate) mod info_panel;
pub(crate) mod tooltip;
pub(crate) mod towers;

use bevy::prelude::*;

pub struct BuildingsPlugin;
impl Plugin for BuildingsPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                common_systems::CommonSystemsPlugin,
                info_panel::InfoPanelPlugin,
                tooltip::BuildingTooltipPlugin,
                energy_relay::EnergyRelayPlugin,
                exploration_center::ExplorationCenterPlugin,
                main_base::MainBasePlugin,
                mining_complex::MiningComplexPlugin,
                forge::ForgePlugin,
                towers::TowersPlugin,
            ));
    }
}
