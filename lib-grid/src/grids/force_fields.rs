use crate::lib_prelude::*;
use crate::grids::base::BaseGrid;

pub type ForceFieldGrid = BaseGrid<Option<Entity>, GridVersion>;

pub struct ForceFieldGridPlugin;
impl Plugin for ForceFieldGridPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(OnExit(MapLoadingStage::LoadMapInfo), |mut commands: Commands, map_info: Res<MapInfo>| {
                commands.insert_resource(ForceFieldGrid::new_with_size(map_info.grid_width, map_info.grid_height));
            });
    }
}
