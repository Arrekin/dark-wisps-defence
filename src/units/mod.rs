pub mod expedition_drone;
pub mod expedition_drone2;

use crate::prelude::*;

pub struct UnitsPlugin;
impl Plugin for UnitsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            expedition_drone::ExpeditionDronePlugin,
            expedition_drone2::ExpeditionDrone2Plugin,
        ));
    }
}
