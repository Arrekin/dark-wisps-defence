use bevy::prelude::*;

use game_core::prelude::{GridCoords, GridImprint};

fn on_grid_coords_insert_sync_transform(
    trigger: On<Insert, GridCoords>,
    mut transforms: Query<(&mut Transform, &GridCoords, &GridImprint)>,
) {
    let entity = trigger.entity;
    let Ok((mut transform, grid_coords, grid_imprint)) = transforms.get_mut(entity) else { return; };
    let world_centered = grid_coords.to_world_position_centered(grid_imprint);
    transform.translation.x = world_centered.x;
    transform.translation.y = world_centered.y;
}

pub struct GridTransformSyncPlugin;
impl Plugin for GridTransformSyncPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_observer(on_grid_coords_insert_sync_transform);
    }
}
