use bevy::color::palettes::css::YELLOW;
use bevy::ui::widget::ViewportNode;

use lib_core::camera::{BuilderPreviewCamera, CameraAutoFollowEntity};
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
                show_on_click_system.run_if(in_state(UiInteraction::Free).or_else(in_state(UiInteraction::DisplayInfoPanel))),
            ))
            .add_systems(OnEnter(UiInteraction::DisplayInfoPanel), on_display_enter_system)
            .add_systems(OnExit(UiInteraction::DisplayInfoPanel), on_display_exit_system)
            .add_observer(on_focused_entity_despawned)
            ;
    }
}

// Marks the root of the space allowed for external content.
#[derive(Component)]
pub struct DisplayPanelMainContentRoot;

/// Marker component on the info panel UI entity.
#[derive(Component)]
pub struct DisplayInfoPanel;

/// Marker placed on the currently selected map object.
/// Insert to select, remove to deselect.
/// Observers on `On<Insert, FocusedMapObject>` and `On<Remove, FocusedMapObject>` drive
/// overlay highlights, info panel content, and other selection-dependent UI.
#[derive(Component)]
pub struct FocusedMapObject;

#[derive(Component)]
struct DisplayInfoPanelCamera;

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
    currently_focused: Option<Single<Entity, With<FocusedMapObject>>>,
) {
    *display_info_panel.into_inner() = Visibility::Hidden;
    info_panel_camera.into_inner().is_active = false;
    if let Some(focused) = currently_focused {
        commands.entity(focused.into_inner()).remove::<FocusedMapObject>();
    }
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
    currently_focused: Option<Single<Entity, With<FocusedMapObject>>>,
    info_panel_camera: Single<Entity, With<DisplayInfoPanelCamera>>,
) {
    if mouse_info.is_over_ui || !mouse.just_pressed(MouseButton::Left) || !mouse_info.grid_coords.is_in_bounds(obstacle_grid.bounds()) { return; }

    let field = &obstacle_grid[mouse_info.grid_coords];
    let focused_element = match &field.structure {
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
    commands.entity(info_panel_camera.into_inner()).insert(CameraAutoFollowEntity(focused_element));

    // Defocus the previous selection before focusing the new one
    if let Some(old_focused) = currently_focused {
        commands.entity(old_focused.into_inner()).remove::<FocusedMapObject>();
    }
    commands.entity(focused_element).insert(FocusedMapObject);
    (*next_ui_interaction_state).set_if_neq(UiInteraction::DisplayInfoPanel);
}

fn on_focused_entity_despawned(
    _trigger: On<Despawn, FocusedMapObject>,
    mut ui_interaction_state: ResMut<NextState<UiInteraction>>,
) {
    ui_interaction_state.set(UiInteraction::Free);
}

fn initialize_display_info_panel_system(
    mut commands: Commands,
) {
    let info_panel_entity = commands.spawn_empty().id();

    // Spawn camera that renders to the image
    let camera = commands.spawn((
        BuilderPreviewCamera::new(info_panel_entity, Vec2::ZERO, 2.),
        DisplayInfoPanelCamera,
    )).id();

    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(5.0),
            left: Val::Percent(25.),
            width: Val::Percent(50.0),
            height: Val::Px(160.0),
            border: UiRect::all(Val::Px(4.0)),
            border_radius: BorderRadius::all(Val::Px(7.)),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            padding: UiRect::all(Val::Px(4.0)),
            ..default()
        },
        BackgroundColor::from(Color::linear_rgba(0.46, 0.62, 0.67, 1.)),
        BorderColor::from(Color::linear_rgba(0., 0.2, 1., 1.)),
        Visibility::Hidden,
        DisplayInfoPanel,
        children![
            // Camera viewport (Left side) using ViewportNode
            (
                Node {
                    min_width: Val::Px(128.0),
                    min_height: Val::Px(128.0),
                    border: UiRect::all(Val::Px(2.0)),
                    ..default()
                },
                BorderColor::from(YELLOW),
                ViewportNode::new(camera),
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
