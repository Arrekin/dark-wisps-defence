use bevy::color::palettes::css::YELLOW;
use bevy::prelude::*;
use bevy::ui::widget::ViewportNode;

use grids::obstacles::{GridStructureType, ObstacleGrid};
use hud::prelude::{DisplayPanelMainContentRoot, FocusedMapObject};
use states::prelude::*;
use viewport::{BuilderPreviewCamera, CameraAutoFollowEntity, MouseInfo};

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
            .add_systems(OnEnter(UiInteraction::DisplayInfoPanel), show_display_info_panel)
            .add_systems(OnExit(UiInteraction::DisplayInfoPanel), hide_display_info_panel)
            .add_observer(on_focused_entity_despawned_return_to_free_interaction)
            ;
    }
}

/// Marker component on the info panel UI entity.
#[derive(Component)]
pub(crate) struct DisplayInfoPanel;

#[derive(Component)]
struct DisplayInfoPanelCamera;

fn show_display_info_panel(
    display_info_panel: Single<&mut Visibility, With<DisplayInfoPanel>>,
    info_panel_camera: Single<&mut Camera, With<DisplayInfoPanelCamera>>,
) {
    *display_info_panel.into_inner() = Visibility::Inherited;
    info_panel_camera.into_inner().is_active = true;
}

fn hide_display_info_panel(
    mut commands: Commands,
    currently_focused: Option<Single<Entity, With<FocusedMapObject>>>,
    display_info_panel: Single<&mut Visibility, With<DisplayInfoPanel>>,
    info_panel_camera: Single<&mut Camera, With<DisplayInfoPanelCamera>>,
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
    mut next_ui_interaction_state: ResMut<NextState<UiInteraction>>,
    obstacle_grid: Res<ObstacleGrid>,
    currently_focused: Option<Single<Entity, With<FocusedMapObject>>>,
    info_panel_camera: Single<Entity, With<DisplayInfoPanelCamera>>,
) {
    if mouse_info.is_over_ui || !mouse.just_pressed(MouseButton::Left) || !mouse_info.grid_coords.are_in_bounds(obstacle_grid.bounds) { return; }

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

fn on_focused_entity_despawned_return_to_free_interaction(
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
        BackgroundColor::from(Color::srgba(0.46, 0.62, 0.67, 1.)),
        BorderColor::from(Color::srgba(0., 0.2, 1., 1.)),
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
