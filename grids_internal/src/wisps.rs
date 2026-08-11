use bevy::prelude::*;

use game_core::prelude::MapInfo;
use grids::wisps::WispsGrid;
use states::prelude::MapLoadingStage;

pub struct WispsGridPlugin;
impl Plugin for WispsGridPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(OnExit(MapLoadingStage::LoadMapInfo), |mut commands: Commands, map_info: Res<MapInfo>| { commands.insert_resource(WispsGrid::new_with_size(map_info.grid_width, map_info.grid_height)); })
            ;
    }
}
