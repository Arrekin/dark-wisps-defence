use bevy::prelude::*;

use game_core::prelude::MapInfo;
use grids::force_fields::ForceFieldGrid;
use states::prelude::MapLoadingStage;

pub struct ForceFieldGridPlugin;
impl Plugin for ForceFieldGridPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(OnExit(MapLoadingStage::LoadMapInfo), |mut commands: Commands, map_info: Res<MapInfo>| {
                commands.insert_resource(ForceFieldGrid::new_with_size(map_info.grid_bounds));
            });
    }
}
