use bevy::color::palettes::css::YELLOW;
use bevy::camera::RenderTarget;
use bevy::render::{
    render_resource::{Extent3d, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages},
    view::Hdr,
};

use lib_grid::grids::obstacles::{GridStructureType, ObstacleGrid};

use crate::prelude::*;

pub struct DisplayInfoPanelPlugin;
impl Plugin for DisplayInfoPanelPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Startup, (
                initialize_display_info_panel_system,
            ))
            .add_systems(Update, (
                hide_system.run_if(in_state(UiInteraction::DisplayInfoPanel)),
                show_on_click_system.run_if(in_state(UiInteraction::Free).or(in_state(UiInteraction::DisplayInfoPanel))),
                on_building_destroyed_system.run_if(in_state(UiInteraction::DisplayInfoPanel).and(on_message::<BuildingDestroyedmessage>)),
            ))
            .add_systems(OnEnter(UiInteraction::DisplayInfoPanel), on_display_enter_system)
            .add_systems(OnExit(UiInteraction::DisplayInfoPanel), on_display_exit_system);
    }
}

// Marks the root of the space allowed for external content.
#[derive(Component)]
pub struct DisplayPanelMainContentRoot;

#[derive(Component)]
pub struct DisplayInfoPanel {
    pub current_focus: Entity,
}
impl Default for DisplayInfoPanel {
    fn default() -> Self {
        Self {
            current_focus: Entity::PLACEHOLDER,
        }
    }
}
#[derive(Component)]
struct DisplayInfoPanelCamera;

/// Triggers emitted when the user clicks on a map object
#[derive(EntityEvent)]
pub struct UiMapObjectFocusedTrigger{ pub entity: Entity }
#[derive(Event)]
pub struct UiMapObjectUnfocusedTrigger;

fn on_display_enter_system(
    display_info_panel: Single<&mut Visibility, With<DisplayInfoPanel>>,
    info_panel_camera: Single<&mut Camera, With<DisplayInfoPanelCamera>>,
) {
    *display_info_panel.into_inner() = Visibility::Inherited;
    info_panel_camera.into_inner().is_active = true;
}

fn on_display_exit_system(
    mut commands: Commands,
    display_info_panel: Single<&mut Visibility, With<DisplayInfoPanel>>,
    info_panel_camera: Single<&mut Camera, With<DisplayInfoPanelCamera>>,
) {
    *display_info_panel.into_inner() = Visibility::Hidden;
    info_panel_camera.into_inner().is_active = false;
    commands.trigger(UiMapObjectUnfocusedTrigger);
}

fn hide_system(
    mouse: Res<ButtonInput<MouseButton>>,
    mut ui_interaction_state: ResMut<NextState<UiInteraction>>,
) {
    if mouse.just_pressed(MouseButton::Right) {
        ui_interaction_state.set(UiInteraction::Free);
    }
}

fn show_on_click_system(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    mouse_info: Res<MouseInfo>,
    obstacle_grid: Res<ObstacleGrid>,
    mut next_ui_interaction_state: ResMut<NextState<UiInteraction>>,
    display_info_panel: Single<&mut DisplayInfoPanel>,
    info_panel_camera: Single<&mut Transform, With<DisplayInfoPanelCamera>>,
    grid_positions: Query<(&GridCoords, &GridImprint)>,
) {
    if !mouse.just_pressed(MouseButton::Left) || !mouse_info.grid_coords.is_in_bounds(obstacle_grid.bounds()) { return; }

    let field = &obstacle_grid[mouse_info.grid_coords];
    let focused_element =  match &field.structure {
        GridStructureType::Building(entity, _) => *entity,
        _ => {
            if let Some(entity) = &field.quantum_field {
                *entity
            } else {
                return;
            }
        },
    };

    // Center the camera on the focused structure
    let Ok((grid_coords, grid_imprint)) = grid_positions.get(focused_element) else { return; };
    let world_position = grid_coords.to_world_position_centered(*grid_imprint);
    let mut camera_transform = info_panel_camera.into_inner();
    camera_transform.translation.x = world_position.x;
    camera_transform.translation.y = world_position.y;

    display_info_panel.into_inner().current_focus = focused_element;
    commands.trigger(UiMapObjectFocusedTrigger { entity: focused_element });
    next_ui_interaction_state.set(UiInteraction::DisplayInfoPanel);
}

fn on_building_destroyed_system(
    mut ui_interaction_state: ResMut<NextState<UiInteraction>>,
    mut events: MessageReader<BuildingDestroyedmessage>,
    display_info_panel: Single<&DisplayInfoPanel>,
) {
    let current_display_entity = display_info_panel.into_inner().current_focus;
    for event in events.read() {
        if event.0 == current_display_entity {
            ui_interaction_state.set(UiInteraction::Free);
        }
    }
}

fn initialize_display_info_panel_system(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
) {
    let camera_image_handle = {
        let size = Extent3d {
            width: 128,
            height: 128,
            ..default()
        };
        let mut image = Image {
            texture_descriptor: TextureDescriptor {
                label: None,
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: TextureFormat::Rgba8UnormSrgb,
                usage: TextureUsages::TEXTURE_BINDING
                    | TextureUsages::COPY_DST
                    | TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            },
            ..Default::default()
        };
        image.resize(size);
        images.add(image)
    };
    commands.spawn((
        Camera2d::default(),
        Camera {
            order: 1,
            target: RenderTarget::Image(camera_image_handle.clone().into()),
            is_active: false,
            ..default()
        },
        Hdr,
        Projection::Orthographic(OrthographicProjection {
            near: -1000.,
            far: 1000.,
            scale: 2., // TODO, check new scaling_mode
            ..OrthographicProjection::default_2d()
        }),
        DisplayInfoPanelCamera,
    ));
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(5.0),
            left: Val::Percent(25.),
            width: Val::Percent(50.0),
            height: Val::Px(160.0),
            border: UiRect::all(Val::Px(4.0)),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            padding: UiRect::all(Val::Px(4.0)),
            ..default()
        },
        BackgroundColor::from(Color::linear_rgba(0.46, 0.62, 0.67, 1.)),
        BorderColor::from(Color::linear_rgba(0., 0.2, 1., 1.)),
        BorderRadius::all(Val::Px(7.)),
        Visibility::Hidden,
        DisplayInfoPanel::default(),
        children![
            // Camera image (Left side)
            (
                Node {
                    min_width: Val::Px(128.0),
                    min_height: Val::Px(128.0),
                    border: UiRect::all(Val::Px(2.0)),
                    ..default()
                },
                BorderColor::from(YELLOW),
                ImageNode::new(camera_image_handle),
            ),
            // Right panels, content is provided by external sub-panels
            (
                Node {
                    height: Val::Percent(100.),
                    width: Val::Percent(100.),
                    ..default()
                },
                DisplayPanelMainContentRoot,
            ),
        ]
    ));
}