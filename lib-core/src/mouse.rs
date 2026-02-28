use bevy::picking::hover::PickingInteraction;
use bevy::window::PrimaryWindow;

use crate::lib_prelude::*;
use crate::camera::MainCamera;

pub mod mouse_prelude {
    pub use super::MouseInfo;
}

pub struct MousePlugin;
impl Plugin for MousePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(MouseInfo::default());
        app.add_systems(PreUpdate, update_mouse_info_system.after(bevy::picking::PickingSystems::Hover));

    }
}

#[derive(Resource, Default)]
pub struct MouseInfo {
    pub screen_position: Vec2,
    pub world_position: Vec2,
    pub grid_coords: GridCoords, // Not guaranteed to be in bounds
    pub is_over_ui: bool,
}

pub fn update_mouse_info_system(
    mut mouse_info: ResMut<MouseInfo>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
    ui_nodes: Query<&PickingInteraction, With<ComputedNode>>,
) {
    let (camera, camera_transform) = camera.into_inner();

    if let Some(screen_position) = window.into_inner().cursor_position() {
        let world_position = camera.viewport_to_world_2d(camera_transform, screen_position).unwrap();
        let grid_coords = GridCoords::from_world_vec2(world_position);
        // Update mouse info
        mouse_info.screen_position = screen_position;
        mouse_info.world_position = world_position;
        mouse_info.grid_coords = grid_coords;
    }
    if !ui_nodes.is_empty() {
        mouse_info.is_over_ui = ui_nodes.iter().any(|interaction| !matches!(interaction, PickingInteraction::None));
    }
}