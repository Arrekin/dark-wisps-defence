use bevy::{prelude::*, sprite_render::{Material2d, MeshMaterial2d}};

use game_core::prelude::MapInfo;

/// Full-map canvas quad: a 1x1 unit mesh scaled to the map and centered at its midpoint.
///
/// The Y scale is negative to flip the texture vertically: the grid origin (0,0) is at the
/// bottom-left in world space, but texture sampling starts at the top-left, so without the flip
/// the shader output would appear upside-down.
#[derive(Bundle)]
pub struct MapCanvasBundle<M: Material2d> {
    mesh: Mesh2d,
    material: MeshMaterial2d<M>,
    transform: Transform,
}
impl<M: Material2d> MapCanvasBundle<M> {
    pub fn new(meshes: &mut Assets<Mesh>, material: Handle<M>, map_info: &MapInfo) -> Self {
        let world_size = map_info.world_size();
        Self {
            mesh: Mesh2d(meshes.add(Rectangle::new(1.0, 1.0))),
            material: MeshMaterial2d(material),
            transform: Transform::from_translation((world_size / 2.).extend(0.))
                .with_scale(Vec3::new(world_size.x, -world_size.y, 1.)),
        }
    }
}
