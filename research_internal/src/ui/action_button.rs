use bevy::prelude::*;
use strum::AsRefStr;

use research::prelude::*;
use widgets::prelude::{BuilderTextCommandButton, TextCommandButtonChildren, TextRole};

pub(crate) struct ResearchActionButtonPlugin;
impl Plugin for ResearchActionButtonPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_message::<RefreshResearchActionButtons>()
            .add_observer(on_add_research_action_button_construct)
            .add_observer(on_insert_research_state_request_refresh_action_buttons)
            .add_systems(
                Update,
                refresh_research_action_buttons.run_if(on_message::<RefreshResearchActionButtons>),
            );
    }
}

/// Action label — Start, Resume, Switch, Stop.
const LABEL_FONT_SIZE: f32 = 12.0;

#[derive(Component)]
pub(crate) struct ResearchActionButton {
    research: Entity,
    action: ResearchAction,
}

#[derive(Clone, Copy, Default, AsRefStr)]
#[strum(serialize_all = "UPPERCASE")]
enum ResearchAction {
    #[default]
    None,
    Start,
    Resume,
    Switch,
    Stop,
}

#[derive(Message, Clone, Copy)]
struct RefreshResearchActionButtons;

impl ResearchActionButton {
    pub(crate) fn new(research: Entity) -> Self {
        Self { research, action: ResearchAction::None }
    }
}

fn on_add_research_action_button_construct(
    trigger: On<Add, ResearchActionButton>,
    mut commands: Commands,
    mut refresh_messages: MessageWriter<RefreshResearchActionButtons>,
) {
    // The queued refresh determines the action label.
    commands.entity(trigger.entity)
        .insert(
            BuilderTextCommandButton::default()
                .with_font_size(LABEL_FONT_SIZE)
                .with_text_role(TextRole::Data),
        )
        .observe(on_click_research_action_button);
    refresh_messages.write(RefreshResearchActionButtons);
}

fn on_insert_research_state_request_refresh_action_buttons(
    _: On<Insert, ResearchState>,
    mut refresh_messages: MessageWriter<RefreshResearchActionButtons>,
) {
    refresh_messages.write(RefreshResearchActionButtons);
}

fn refresh_research_action_buttons(
    mut refresh_messages: MessageReader<RefreshResearchActionButtons>,
    researches: Query<(&ResearchState, Option<&ResearchRuntime>)>,
    active: Option<Single<(), With<ResearchActive>>>,
    mut buttons: Query<(&mut ResearchActionButton, &mut Node, &TextCommandButtonChildren)>,
    mut texts: Query<&mut Text>,
) {
    refresh_messages.clear();

    let any_active = active.is_some();

    for (mut button, mut node, children) in buttons.iter_mut() {
        let action = researches.get(button.research)
            .map(|(state, runtime)| action_for(*state, runtime.map(|runtime| runtime.progress), any_active))
            .unwrap_or(ResearchAction::None);
        button.action = action;
        node.display = if matches!(action, ResearchAction::None) { Display::None } else { Display::Flex };

        if let Ok(mut text) = texts.get_mut(children.label) {
            text.0 = action.as_ref().to_string();
        }
    }
}

fn on_click_research_action_button(
    mut trigger: On<Pointer<Click>>,
    mut commands: Commands,
    buttons: Query<&ResearchActionButton>,
) {
    trigger.propagate(false);
    let Ok(button) = buttons.get(trigger.entity) else { return };

    match button.action {
        ResearchAction::Start | ResearchAction::Resume | ResearchAction::Switch => {
            commands.trigger(SetActiveResearch { research: button.research });
        }
        ResearchAction::Stop => commands.trigger(StopResearch { research: button.research }),
        ResearchAction::None => {}
    }
}

fn action_for(
    state: ResearchState,
    progress: Option<f32>,
    any_active: bool,
) -> ResearchAction {
    match state {
        ResearchState::Active => ResearchAction::Stop,
        ResearchState::Completed => ResearchAction::None,
        ResearchState::Available => {
            if any_active { ResearchAction::Switch }
            else if progress.unwrap_or(0.) > 0. { ResearchAction::Resume }
            else { ResearchAction::Start }
        }
    }
}

