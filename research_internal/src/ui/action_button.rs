use bevy::prelude::*;
use strum::AsRefStr;

use research::prelude::*;

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

#[derive(Component)]
#[require(Button, Pickable)]
pub(crate) struct ResearchActionButton {
    research: Entity,
    action: ResearchAction,
    label: Entity,
}

#[derive(Clone, Copy, Default, AsRefStr)]
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
        Self {
            research,
            action: ResearchAction::None,
            label: Entity::PLACEHOLDER,
        }
    }
}

fn on_add_research_action_button_construct(
    trigger: On<Add, ResearchActionButton>,
    mut commands: Commands,
    mut refresh_messages: MessageWriter<RefreshResearchActionButtons>,
    buttons: Query<&ResearchActionButton>,
) {
    let Ok(button) = buttons.get(trigger.entity) else { return };
    let label = commands.spawn((
        Text::default(),
        TextFont::from_font_size(12.),
        TextColor::from(Color::WHITE),
        TextLayout::no_wrap(),
    )).id();

    commands.entity(trigger.entity)
        .insert((
            BackgroundColor::from(Color::linear_rgba(0.2, 0.4, 0.8, 1.)),
            ResearchActionButton {
                research: button.research,
                action: ResearchAction::None,
                label,
            },
        ))
        .observe(on_click_research_action_button)
        .add_child(label);
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
    mut buttons: Query<(&mut ResearchActionButton, &mut Node, &mut BackgroundColor)>,
    mut texts: Query<&mut Text>,
) {
    refresh_messages.clear();

    let any_active = active.is_some();

    for (mut button, mut node, mut background) in buttons.iter_mut() {
        let action = researches.get(button.research)
            .map(|(state, runtime)| action_for(*state, runtime.map(|runtime| runtime.progress), any_active))
            .unwrap_or(ResearchAction::None);
        button.action = action;
        node.display = if matches!(action, ResearchAction::None) { Display::None } else { Display::Flex };
        background.0 = Color::linear_rgba(0.2, 0.4, 0.8, 1.);

        if let Ok(mut text) = texts.get_mut(button.label) {
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

