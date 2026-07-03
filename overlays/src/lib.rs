pub mod emissions;
pub mod energy_supply;
pub mod towers_range;

use bevy::{
    prelude::*,
    render::render_resource::ShaderType,
    sprite_render::{Material2d, MeshMaterial2d},
};

use grids::prelude::MapInfo;

pub struct OverlaysPlugin;
impl Plugin for OverlaysPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            towers_range::TowersRangeOverlayPlugin,
            energy_supply::EnergySupplyOverlayPlugin,
            emissions::EmissionsOverlayPlugin,
        ));
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, ShaderType, Default)]
struct UniformGridData {
    grid_width: u32,
    grid_height: u32,
}

/// Spawn a world-space overlay quad covering the entire map.
pub fn overlay_bundle<M: Material2d + Default>(
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<M>,
    map_info: &MapInfo,
) -> impl Bundle {
    (
        Mesh2d(meshes.add(Rectangle::new(1.0, 1.0))),
        MeshMaterial2d(materials.add(M::default())),
        Transform::from_xyz(map_info.world_width / 2., map_info.world_height / 2., 0.)
            .with_scale(Vec3::new(map_info.world_width, -map_info.world_height, 1.)),  // Flip vertically due to coordinate system
    )
}
