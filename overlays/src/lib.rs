pub mod emissions;
pub mod energy_supply;
pub mod towers_range;

use bevy::{prelude::*, render::render_resource::ShaderType};

use game_core::prelude::Bounds;

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
impl From<Bounds> for UniformGridData {
    fn from(bounds: Bounds) -> Self {
        let (grid_width, grid_height) = bounds.as_u32();
        Self { grid_width, grid_height }
    }
}
