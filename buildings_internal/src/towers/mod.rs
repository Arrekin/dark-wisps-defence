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
                emitter::tower_emitter::TowerEmitterPlugin,
                field::tower_field::TowerFieldPlugin,
            ));
    }
}
