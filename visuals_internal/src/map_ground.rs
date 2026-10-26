//! Draws a floor beneath the map.

use bevy::prelude::*;

use game_core::prelude::{MapInfo, ZDepth};
use states::prelude::MapLoadingStage;

const MAP_GROUND_COLOR: Color = Color::srgb_u8(19, 20, 22); // near black

pub(crate) struct MapGroundPlugin;
impl Plugin for MapGroundPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(MapLoadingStage::LoadResources), spawn_map_ground);
    }
}

#[derive(Component)]
#[require(ZDepth::MapGround)]
struct MapGround;

fn spawn_map_ground(mut commands: Commands, map_info: Res<MapInfo>) {
    commands.despawn_all::<With<MapGround>>();

    let world_size = map_info.world_size();
    commands.spawn((
        Sprite::from_color(MAP_GROUND_COLOR, world_size),
        Transform::from_translation((world_size / 2.).extend(0.)),
        MapGround,
    ));
}
