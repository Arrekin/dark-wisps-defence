pub(crate) mod blaster;
pub(crate) mod cannon;
pub(crate) mod emitter;
pub(crate) mod field;
pub(crate) mod rocket_launcher;

use bevy::prelude::*;

pub(crate) struct TowersPlugin;
impl Plugin for TowersPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                blaster::tower_blaster::TowerBlasterPlugin,
                blaster::tower_flamethrower::TowerFlamethrowerPlugin,
                cannon::tower_cannon::TowerCannonPlugin,
                cannon::tower_blaze_mortar::TowerBlazeMortarPlugin,
                rocket_launcher::tower_rocket_launcher::TowerRocketLauncherPlugin,
                rocket_launcher::tower_napalm_battery::TowerNapalmBatteryPlugin,
                emitter::tower_emitter::TowerEmitterPlugin,
                emitter::tower_inferno::TowerInfernoPlugin,
                emitter::tower_inferno_fire::TowerInfernoFirePlugin,
                field::tower_field::TowerFieldPlugin,
                field::tower_crucible::TowerCruciblePlugin,
            ));
    }
}
