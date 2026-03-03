pub mod main_base;
pub mod common;
pub mod tower_blaster;
pub mod tower_cannon;
pub mod tower_emitter;
pub mod common_systems;
pub mod energy_relay;
pub mod tower_rocket_launcher;
pub mod mining_complex;
pub mod exploration_center;
pub mod info_panel;

use crate::prelude::*;

pub mod prelude {
    pub use super::common::*;
}

pub struct BuildingsPlugin;
impl Plugin for BuildingsPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                common_systems::CommonSystemsPlugin,
                info_panel::InfoPanelPlugin,
                energy_relay::EnergyRelayPlugin,
                exploration_center::ExplorationCenterPlugin,
                main_base::MainBasePlugin,
                mining_complex::MiningComplexPlugin,
                tower_blaster::TowerBlasterPlugin,
                tower_cannon::TowerCannonPlugin,
                tower_rocket_launcher::TowerRocketLauncherPlugin,
                tower_emitter::TowerEmitterPlugin,
            ));
    }
}
