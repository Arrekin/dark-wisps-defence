use bevy::{
    prelude::*,
    ui::{widget::NodeImageMode, VisualBox},
};

use game_core::prelude::DisplayName;
use narrative::prelude::*;

pub(crate) struct ObjectivesPanelPlugin;
impl Plugin for ObjectivesPanelPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_state::<ObjectivesPanelState>()
            .add_systems(PreStartup, |mut commands: Commands| {
                commands.spawn(ObjectivesPanel);
            })
            .add_systems(Update, (
                panel_transition_to_visible.run_if(in_state(ObjectivesPanelState::TransitionToVisible)),
                panel_transition_to_hidden.run_if(in_state(ObjectivesPanelState::TransitionToHidden)),
                update_display_lines,
                update_titles,
            ))
            .add_observer(on_add_objectives_panel_construct)
            .add_observer(on_add_objectives_panel_content_construct)
            .add_observer(ObjectivesShowHideButton::on_add_objectives_show_hide_button_construct)
            .add_observer(on_add_objective_details_spawn_row)
            .add_observer(on_remove_objective_details_despawn_row)
            .add_observer(on_insert_objective_state_request_rebuild)
            .add_observer(on_rebuild_objectives_panel_do_so);
    }
}

// ============================================================================
// COMPONENTS
// ============================================================================

/// Marker on the panel root entity.
#[derive(Component)]
pub(crate) struct ObjectivesPanel;

/// Marker on the content container (child of panel root, holds objective rows).
#[derive(Component, Default, Clone)]
pub(crate) struct ObjectivesPanelContent;

/// Marker on the show/hide toggle button (child of panel root).
#[derive(Component, Default, Clone)]
#[require(Button, Pickable)]
pub(crate) struct ObjectivesShowHideButton;

/// Component on a row entity (child of content container). Stores the objective
/// this row mirrors, plus the row's widget entities for direct indexing.
#[derive(Component)]
pub(crate) struct ObjectiveRow {
    pub(crate) objective: Entity,
    pub(crate) checkmark: Entity,
    pub(crate) title: Entity,
    pub(crate) content: Entity,
}

/// Marker on the content container within a row (holds display-line texts).
#[derive(Component)]
pub(crate) struct ObjectiveRowContent;

/// Marker on the checkmark image child of a row.
#[derive(Component)]
pub(crate) struct ObjectiveCheckmark;

/// Marker on the title text child of a row.
#[derive(Component)]
pub(crate) struct ObjectiveTitle;

/// Marker on a display-line text child of a row. Stores the goal entity
/// this text mirrors.
#[derive(Component)]
pub(crate) struct ObjectiveDisplayLineText(pub Entity);

/// Panel show/hide state machine (self-contained, not UiInteraction).
#[derive(States, Debug, Clone, PartialEq, Eq, Hash, Default)]
pub(crate) enum ObjectivesPanelState {
    Hidden,
    TransitionToVisible,
    #[default]
    Visible,
    TransitionToHidden,
}

// ============================================================================
// CONSTANTS
// ============================================================================

const SLIDING_SPEED: f32 = 800.;
const VISIBLE_TOP_POSITION: f32 = 5.;

// ============================================================================
// PANEL ROOT CONSTRUCTION
// ============================================================================

fn on_add_objectives_panel_construct(
    trigger: On<Add, ObjectivesPanel>,
    mut commands: Commands,
) {
    commands.entity(trigger.entity).apply_scene(bsn! {
        Node {
            width: Val::Px(300.0),
            position_type: PositionType::Absolute,
            flex_direction: FlexDirection::Column,
            top: Val::Px(VISIBLE_TOP_POSITION),
            right: Val::Px(5.0),
            padding: UiRect::all(Val::Px(8.0)),
            row_gap: Val::Px(2.0),
        }
        ImageNode {
            image: "ui/objectives_panel.png",
            image_mode: NodeImageMode::Sliced(TextureSlicer {
                border: BorderRect::all(20.0),
                center_scale_mode: SliceScaleMode::Stretch,
                sides_scale_mode: SliceScaleMode::Stretch,
                max_corner_scale: 1.0,
            }),
            visual_box: VisualBox::BorderBox,
        }
        Children [
            ObjectivesPanelContent,
            ObjectivesShowHideButton,
        ]
    });
}

fn on_add_objectives_panel_content_construct(
    trigger: On<Add, ObjectivesPanelContent>,
    mut commands: Commands,
) {
    commands.entity(trigger.entity).apply_scene(bsn! {
        Node {
            width: Val::Percent(100.),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(2.),
        }
    });
}

// ============================================================================
// SHOW/HIDE BUTTON
// ============================================================================

impl ObjectivesShowHideButton {
    fn on_add_objectives_show_hide_button_construct(
        trigger: On<Add, ObjectivesShowHideButton>,
        mut commands: Commands,
    ) {
        commands.entity(trigger.entity).apply_scene(bsn! {
            Node {
                width: Val::Px(32.0),
                height: Val::Px(32.0),
                position_type: PositionType::Absolute,
                bottom: Val::Px(-34.0),
                right: Val::Px(5.0),
            }
            ImageNode { image: "ui/objectives_panel.png" }
            on(Self::on_click_toggle_panel_visibility)
        });
    }

    fn on_click_toggle_panel_visibility(
        _trigger: On<Pointer<Click>>,
        current_state: Res<State<ObjectivesPanelState>>,
        mut next_state: ResMut<NextState<ObjectivesPanelState>>,
    ) {
        match current_state.get() {
            ObjectivesPanelState::Hidden => next_state.set(ObjectivesPanelState::TransitionToVisible),
            ObjectivesPanelState::Visible => next_state.set(ObjectivesPanelState::TransitionToHidden),
            _ => {}
        }
    }
}

// ============================================================================
// SLIDE ANIMATION
// ============================================================================

fn panel_transition_to_visible(
    time: Res<Time>,
    mut next_state: ResMut<NextState<ObjectivesPanelState>>,
    panel: Single<&mut Node, With<ObjectivesPanel>>,
) {
    let mut node = panel.into_inner();
    let current_top = match node.top {
        Val::Px(top) => top,
        _ => return,
    };
    let new_top = current_top + time.delta_secs() * SLIDING_SPEED;
    if new_top < VISIBLE_TOP_POSITION {
        node.top = Val::Px(new_top);
    } else {
        node.top = Val::Px(VISIBLE_TOP_POSITION);
        next_state.set(ObjectivesPanelState::Visible);
    }
}

fn panel_transition_to_hidden(
    time: Res<Time>,
    mut next_state: ResMut<NextState<ObjectivesPanelState>>,
    panel: Single<(&ComputedNode, &mut Node), With<ObjectivesPanel>>,
) {
    let (computed_node, mut node) = panel.into_inner();
    let current_top = match node.top {
        Val::Px(top) => top,
        _ => return,
    };
    let new_top = current_top - time.delta_secs() * SLIDING_SPEED;
    if new_top > -computed_node.size().y {
        node.top = Val::Px(new_top);
    } else {
        node.top = Val::Px(-computed_node.size().y);
        next_state.set(ObjectivesPanelState::Hidden);
    }
}

// ============================================================================
// OBJECTIVE ROW CONSTRUCTION
// ============================================================================

fn on_add_objective_details_spawn_row(
    trigger: On<Add, ObjectiveDetails>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    details: Query<&ObjectiveDetails>,
    content: Single<Entity, With<ObjectivesPanelContent>>,
) {
    let objective_entity = trigger.entity;
    let Ok(details) = details.get(objective_entity) else { return };

    let checkmark = commands.spawn((
        Node {
            width: Val::Px(16.),
            height: Val::Px(16.),
            ..default()
        },
        ImageNode::new(asset_server.load("ui/objectives_check_active.png")),
        ObjectiveCheckmark,
    )).id();

    let title = commands.spawn((
        Text::new(details.id_name.clone()),
        TextFont::from_font_size(12.),
        ObjectiveTitle,
    )).id();

    let header = commands.spawn((
        Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(5.),
            ..default()
        },
    ))
    .add_children(&[checkmark, title])
    .id();

    let row_content = commands.spawn((
        ObjectiveRowContent,
        Node {
            width: Val::Percent(100.),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(1.),
            padding: UiRect::left(Val::Px(21.)),
            ..default()
        },
    )).id();

    let row = commands.spawn((
        ObjectiveRow {
            objective: objective_entity,
            checkmark,
            title,
            content: row_content,
        },
        Node {
            width: Val::Percent(100.),
            border: UiRect::all(Val::Px(2.)),
            flex_direction: FlexDirection::Column,
            border_radius: BorderRadius::all(Val::Px(7.)),
            padding: UiRect::all(Val::Px(4.)),
            row_gap: Val::Px(2.),
            ..default()
        },
        BackgroundColor::from(Color::srgba(0.3, 0.3, 0.3, 0.7)),
        BorderColor::from(Color::srgba(0.2, 0.2, 0.2, 0.9)),
    ))
    .add_children(&[header, row_content])
    .id();

    commands.entity(*content).add_child(row);
}

// ============================================================================
// REBUILD EVENT + TRIGGER OBSERVERS
// ============================================================================

#[derive(Event)]
pub(crate) struct RebuildObjectivesPanel;

fn on_insert_objective_state_request_rebuild(
    _trigger: On<Insert, ObjectiveState>,
    mut commands: Commands,
) {
    commands.trigger(RebuildObjectivesPanel);
}

// ============================================================================
// REBUILD (structural changes: state, goal-set)
// ============================================================================

fn on_rebuild_objectives_panel_do_so(
    _trigger: On<RebuildObjectivesPanel>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    rows: Query<(Entity, &ObjectiveRow)>,
    objectives: Query<(&ObjectiveState, Option<&ObjectiveGoals>), With<ObjectiveDetails>>,
    display_lines: Query<&DisplayName>,
    mut checkmarks: Query<&mut ImageNode, With<ObjectiveCheckmark>>,
    mut row_colors: Query<(&mut BackgroundColor, &mut BorderColor), With<ObjectiveRow>>,
    row_content_children: Query<&Children, With<ObjectiveRowContent>>,
    display_texts: Query<&ObjectiveDisplayLineText>,
) {
    for (row_entity, row) in rows.iter() {
        let Ok((state, goals)) = objectives.get(row.objective) else { continue };

        // 1. Update colors + checkmark based on state
        let (checkmark_image, background, border) = match state {
            ObjectiveState::Inactive => (
                "ui/objectives_check_active.png",
                Color::srgba(0.3, 0.3, 0.3, 0.7),
                Color::srgba(0.2, 0.2, 0.2, 0.9),
            ),
            ObjectiveState::InProgress => (
                "ui/objectives_check_active.png",
                Color::srgba(0.1, 0.3, 0.8, 0.7),
                Color::srgba(0., 0.2, 0.8, 0.9),
            ),
            ObjectiveState::Satisfied => (
                "ui/objectives_check_completed.png",
                Color::srgba(0.1, 0.8, 0.3, 0.7),
                Color::srgba(0., 0.8, 0.2, 0.9),
            ),
            ObjectiveState::Failed => (
                "ui/objectives_check_failed.png",
                Color::srgba(0.8, 0.1, 0.3, 0.7),
                Color::srgba(0.8, 0., 0.2, 0.9),
            ),
        };

        // Update row colors
        if let Ok((mut background_color, mut border_color)) = row_colors.get_mut(row_entity) {
            *background_color = background.into();
            *border_color = border.into();
        }

        // Update the checkmark directly via stored entity.
        if let Ok(mut checkmark) = checkmarks.get_mut(row.checkmark) {
            checkmark.image = asset_server.load(checkmark_image);
        }

        // 2. Sync display-line texts
        // Despawn old display-line texts
        if let Ok(content_children) = row_content_children.get(row.content) {
            for child in content_children.iter() {
                if display_texts.contains(child) {
                    commands.entity(child).despawn();
                }
            }
        }

        // Spawn new display-line texts from goals
        if let Some(goals) = goals {
            for goal_entity in goals.iter() {
                if let Ok(display_line) = display_lines.get(goal_entity) {
                    let text = commands.spawn((
                        Text::new(display_line.0.clone()),
                        TextFont::from_font_size(10.),
                        ObjectiveDisplayLineText(goal_entity),
                    )).id();
                    commands.entity(row.content).add_child(text);
                }
            }
        }
    }
}

// ============================================================================
// DISPLAY-LINE UPDATE (progress ticks — Changed<DisplayName>)
// ============================================================================

fn update_display_lines(
    changed: Query<&DisplayName, Changed<DisplayName>>,
    mut texts: Query<(&ObjectiveDisplayLineText, &mut Text)>,
) {
    if changed.is_empty() { return; }
    for (marker, mut text) in texts.iter_mut() {
        if let Ok(line) = changed.get(marker.0) {
            text.0 = line.0.clone();
        }
    }
}

// ============================================================================
// TITLE UPDATE (id_name rename — Changed<ObjectiveDetails>)
// ============================================================================

fn update_titles(
    changed: Query<&ObjectiveDetails, Changed<ObjectiveDetails>>,
    rows: Query<&ObjectiveRow>,
    mut titles: Query<&mut Text, With<ObjectiveTitle>>,
) {
    if changed.is_empty() { return; }
    for row in rows.iter() {
        let Ok(details) = changed.get(row.objective) else { continue };
        if let Ok(mut title_text) = titles.get_mut(row.title) {
            title_text.0 = details.id_name.clone();
        }
    }
}

// ============================================================================
// ROW CLEANUP
// ============================================================================

fn on_remove_objective_details_despawn_row(
    trigger: On<Remove, ObjectiveDetails>,
    mut commands: Commands,
    rows: Query<(Entity, &ObjectiveRow)>,
) {
    let objective_entity = trigger.entity;
    for (row_entity, row) in rows.iter() {
        if row.objective == objective_entity {
            commands.entity(row_entity).despawn();
        }
    }
}
