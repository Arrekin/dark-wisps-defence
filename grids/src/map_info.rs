use bevy::prelude::*;

use game_core::prelude::SSS;

#[derive(Resource, Default, Clone, SSS)]
pub struct MapInfo {
    pub grid_width: i32,
    pub grid_height: i32,
    pub world_width: f32,
    pub world_height: f32,
    pub name: String,
}
