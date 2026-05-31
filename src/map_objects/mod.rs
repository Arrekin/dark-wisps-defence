pub mod walls;
pub mod dark_ore;
pub mod quantum_field;
pub mod quantum_field_post_process;
pub mod common;

use crate::prelude::*;


pub struct MapObjectsPlugin;
impl Plugin for MapObjectsPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                dark_ore::DarkOrePlugin,
                quantum_field::QuantumFieldPlugin,
                quantum_field_post_process::QuantumFieldPostProcessPlugin,
                walls::WallPlugin,
            ));
    }
}
