use bevy::prelude::*;

use grids::prelude::MapInfo;
use persistence::prelude::{AppGameLoadSaveExtension, SaveableBatchCommand};
use states::prelude::MapLoadingStage;

fn save_map_info(
    mut commands: Commands,
    map_info: Res<MapInfo>,
) {
    commands.queue(SaveableBatchCommand::from_single(map_info.clone()));
}

pub struct MapInfoPlugin;
impl Plugin for MapInfoPlugin {
    fn build(&self, app: &mut App) {
        app
            .register_db_loader::<MapInfo>(MapLoadingStage::LoadMapInfo)
            .register_db_saver(save_map_info)
            ;
    }
}
