pub(crate) mod expedition_drone;

use bevy::prelude::*;

pub struct UnitsPlugin;
impl Plugin for UnitsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            expedition_drone::ExpeditionDronePlugin,
        ));
    }
}
