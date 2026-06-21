use bevy::ui::{widget::NodeImageMode, VisualBox};

use crate::{objectives::Objective, prelude::*};

pub struct ObjectivesPanelPlugin;
impl Plugin for ObjectivesPanelPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_state::<ObjectivesPanelState>()
            .add_systems(PreStartup, |mut commands: Commands| { commands.spawn(ObjectivesPanel); })
            .add_systems(Update, (
                panel_transition_to_hidden_system.run_if(in_state(ObjectivesPanelState::TransitionToHidden)),
                panel_transition_to_visible_system.run_if(in_state(ObjectivesPanelState::TransitionToVisible)),
            ))
            .add_observer(ObjectivesPanel::on_objective_added)
            .add_observer(ObjectivesPanel::on_add)
            .add_observer(ObjectivesShowHideButton::on_add);
    }
}

const SLIDING_SPEED: f32 = 800.;
const VISIBLE_TOP_POSITION: f32 = 5.;

#[derive(Component, Default, Clone)]
#[require(Button, Pickable)]
pub struct ObjectivesShowHideButton;
impl ObjectivesShowHideButton {
    fn on_add(
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
            on(Self::on_click)
        });
    }
    fn on_click(
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
#[derive(States, Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum ObjectivesPanelState {
    Hidden,
    TransitionToVisible,
    #[default]
    Visible,
    TransitionToHidden,
}

#[derive(Component)]
pub struct ObjectivesPanel;
impl ObjectivesPanel {
    fn on_add(
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
                ObjectivesShowHideButton,
            ]
        });
    }
    fn on_objective_added(
        trigger: On<Add, Objective>,
        mut commands: Commands,
        objectives_panel: Single<Entity, With<ObjectivesPanel>>,
    ) {
        let objective_entity = trigger.entity;
        commands.entity(objectives_panel.into_inner()).add_child(objective_entity);
    }
}

fn panel_transition_to_visible_system(
    time: Res<Time>,
    mut next_state: ResMut<NextState<ObjectivesPanelState>>,
    objectives_panel: Single<&mut Node, With<ObjectivesPanel>>,
) {
    let mut node = objectives_panel.into_inner();
    let current_top = match node.top {
        Val::Px(top) => top,
        _ => unreachable!(),
    };
    let new_top = current_top + time.delta_secs() * SLIDING_SPEED;
    if new_top < VISIBLE_TOP_POSITION {
        node.top = Val::Px(new_top);
    } else {
        node.top = Val::Px(VISIBLE_TOP_POSITION);
        next_state.set(ObjectivesPanelState::Visible);
    }
}

fn panel_transition_to_hidden_system(
    time: Res<Time>,
    mut next_state: ResMut<NextState<ObjectivesPanelState>>,
    objectives_panel: Single<(&ComputedNode, &mut Node), With<ObjectivesPanel>>,
) {
    let (computed_node, mut node) = objectives_panel.into_inner();
    let current_top = match node.top {
        Val::Px(top) => top,
        _ => unreachable!(),
    };
    let new_top = current_top - time.delta_secs() * SLIDING_SPEED;
    if new_top > -computed_node.size().y {
        node.top = Val::Px(new_top);
    } else {
        node.top = Val::Px(-computed_node.size().y);
        next_state.set(ObjectivesPanelState::Hidden);
    }
}
