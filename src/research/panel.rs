//! The research panel: a toggled overlay listing one card per actionable research.
//!
//! Fully reactive. The card list is rebuilt by observers when research lifecycle state changes
//! (active toggled, completed, made obsolete, or a new research instantiated) and when the panel
//! opens — never polled. The only per-frame work is animating the active research's progress bar.
//! The panel renders presentation projections (`ResearchCardDisplay`, `OutcomeDisplay`) and reads
//! lifecycle via query filters; it never touches outcome/possession domain types.

use crate::prelude::*;

/// Trigger to rebuild the panel's card list.
#[derive(Event)]
pub struct RebuildResearchPanel;

/// Full-screen overlay root; toggled visible while in `UiInteraction::ResearchPanel`.
#[derive(Component)]
pub struct ResearchPanelRoot;
impl ResearchPanelRoot {
    fn on_add(trigger: On<Add, ResearchPanelRoot>, mut commands: Commands) {
        commands.entity(trigger.entity)
            .insert((
                Node {
                    position_type: PositionType::Absolute,
                    top: Val::Px(0.),
                    left: Val::Px(0.),
                    width: Val::Percent(100.),
                    height: Val::Percent(100.),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor::from(Color::linear_rgba(0., 0., 0., 0.6)),
                Visibility::Hidden,
            ))
            .with_children(|parent| {
                parent.spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        row_gap: Val::Px(10.),
                        padding: UiRect::all(Val::Px(16.)),
                        min_width: Val::Px(380.),
                        border_radius: BorderRadius::all(Val::Px(6.)),
                        ..default()
                    },
                    BackgroundColor::from(Color::linear_rgba(0.1, 0.1, 0.15, 0.98)),
                    children![
                        (
                            Text::new("Research"),
                            TextFont::from_font_size(18.),
                        ),
                        (
                            Node {
                                flex_direction: FlexDirection::Column,
                                align_items: AlignItems::Stretch,
                                row_gap: Val::Px(8.),
                                ..default()
                            },
                            ResearchPanelContent,
                        ),
                    ],
                ));
            });
    }
}

/// Container the cards are (re)built into.
#[derive(Component)]
pub struct ResearchPanelContent;

/// Marks the progress-bar fill of a card so it can be animated live for `research`.
#[derive(Component)]
struct ResearchCardProgressFill {
    research: Entity,
}

/// Start/Stop button on a card, bound to `research`.
#[derive(Component)]
#[require(Button)]
struct ResearchStartStopButton {
    research: Entity,
}
impl ResearchStartStopButton {
    fn on_click(
        trigger: On<Pointer<Click>>,
        mut commands: Commands,
        buttons: Query<&ResearchStartStopButton>,
        active: Query<(), With<ActiveResearch>>,
        researches: Query<&Research>,
    ) {
        let Ok(button) = buttons.get(trigger.entity) else { return };
        let research = button.research;
        if active.get(research).is_ok() {
            commands.trigger(StopResearch);
        } else if let Ok(research_id) = researches.get(research) {
            commands.trigger(SetActiveResearch(research_id.0));
        }
    }
}

pub fn setup(mut commands: Commands) {
    commands.spawn(ResearchPanelRoot);
}

pub fn register(app: &mut App) {
    app
        .add_systems(Startup, setup)
        .add_systems(OnEnter(UiInteraction::ResearchPanel), on_enter_panel)
        .add_systems(OnExit(UiInteraction::ResearchPanel), on_exit_panel)
        .add_systems(Update, update_card_progress.run_if(in_state(UiInteraction::ResearchPanel)))
        .add_observer(ResearchPanelRoot::on_add)
        .add_observer(rebuild_panel)
        .add_observer(on_active_inserted)
        .add_observer(on_active_removed)
        .add_observer(on_research_completed)
        .add_observer(on_obsolete_inserted)
        .add_observer(on_obsolete_removed)
        .add_observer(on_research_instantiated)
        ;
}

fn on_enter_panel(
    mut commands: Commands,
    root: Single<&mut Visibility, With<ResearchPanelRoot>>,
) {
    *root.into_inner() = Visibility::Inherited;
    commands.trigger(RebuildResearchPanel);
}

fn on_exit_panel(root: Single<&mut Visibility, With<ResearchPanelRoot>>) {
    *root.into_inner() = Visibility::Hidden;
}

// Reactive rebuild triggers: any lifecycle transition that changes which cards are shown, or their
// Start/Stop label, requests a rebuild. No per-frame polling.
fn on_active_inserted(_trigger: On<Insert, ActiveResearch>, mut commands: Commands) {
    commands.trigger(RebuildResearchPanel);
}
fn on_active_removed(_trigger: On<Remove, ActiveResearch>, mut commands: Commands) {
    commands.trigger(RebuildResearchPanel);
}
fn on_research_completed(_trigger: On<ResearchCompleted>, mut commands: Commands) {
    commands.trigger(RebuildResearchPanel);
}
fn on_obsolete_inserted(_trigger: On<Insert, Obsolete>, mut commands: Commands) {
    commands.trigger(RebuildResearchPanel);
}
// Future-proofs revocation: a research becoming non-obsolete again should reappear.
fn on_obsolete_removed(_trigger: On<Remove, Obsolete>, mut commands: Commands) {
    commands.trigger(RebuildResearchPanel);
}
fn on_research_instantiated(_trigger: On<ResearchInstantiated>, mut commands: Commands) {
    commands.trigger(RebuildResearchPanel);
}

fn update_card_progress(
    progresses: Query<&ResearchProgress>,
    mut fills: Query<(&ResearchCardProgressFill, &mut Node)>,
) {
    for (fill, mut node) in fills.iter_mut() {
        let fraction = progresses.get(fill.research).map(|progress| progress.fraction).unwrap_or(0.0);
        node.width = Val::Percent(fraction * 100.);
    }
}

fn rebuild_panel(
    _trigger: On<RebuildResearchPanel>,
    mut commands: Commands,
    content: Single<Entity, With<ResearchPanelContent>>,
    researches: Query<(Entity, &ResearchCardDisplay, &ResearchOutcomes, Has<ActiveResearch>, Option<&ResearchProgress>), (With<Research>, Without<Completed>, Without<Obsolete>)>,
    outcome_displays: Query<&OutcomeDisplay>,
) {
    let content_entity = *content;
    commands.entity(content_entity).despawn_related::<Children>();

    for (research_entity, display, outcomes, is_active, progress) in researches.iter() {
        let grants: Vec<(Handle<Image>, String)> = outcomes.iter()
            .filter_map(|outcome| outcome_displays.get(outcome).ok())
            .map(|outcome_display| (outcome_display.icon.clone(), outcome_display.title.clone()))
            .collect();
        let fraction = progress.map(|p| p.fraction).unwrap_or(0.0);
        let card = spawn_card(&mut commands, research_entity, display, is_active, fraction, &grants);
        commands.entity(content_entity).add_child(card);
    }
}

fn spawn_card(
    commands: &mut Commands,
    research: Entity,
    display: &ResearchCardDisplay,
    is_active: bool,
    fraction: f32,
    grants: &[(Handle<Image>, String)],
) -> Entity {
    let button_label = if is_active { "Stop" } else { "Start" };

    let card = commands.spawn((
        Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(10.),
            padding: UiRect::all(Val::Px(8.)),
            border_radius: BorderRadius::all(Val::Px(4.)),
            ..default()
        },
        BackgroundColor::from(Color::linear_rgba(0.15, 0.15, 0.2, 0.9)),
    )).id();

    let icon = commands.spawn((
        Node { width: Val::Px(48.), height: Val::Px(48.), ..default() },
        ImageNode::new(display.icon.clone()),
    )).id();
    commands.entity(card).add_child(icon);

    let column = commands.spawn(Node {
        flex_direction: FlexDirection::Column,
        align_items: AlignItems::Start,
        row_gap: Val::Px(4.),
        min_width: Val::Px(220.),
        ..default()
    }).id();
    commands.entity(card).add_child(column);

    let title = commands.spawn((
        Text::new(display.title.clone()),
        TextFont::from_font_size(14.),
        TextLayout::new_with_linebreak(LineBreak::NoWrap),
    )).id();
    commands.entity(column).add_child(title);

    let grants_row = commands.spawn(Node {
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        column_gap: Val::Px(4.),
        ..default()
    }).id();
    commands.entity(column).add_child(grants_row);

    let grants_label = commands.spawn((
        Text::new("Grants:"),
        TextFont::from_font_size(10.),
        TextColor::from(Color::linear_rgba(0.7, 0.7, 0.7, 1.)),
        TextLayout::new_with_linebreak(LineBreak::NoWrap),
    )).id();
    commands.entity(grants_row).add_child(grants_label);
    for (grant_icon, grant_title) in grants {
        let grant_icon_node = commands.spawn((
            Node { width: Val::Px(20.), height: Val::Px(20.), ..default() },
            ImageNode::new(grant_icon.clone()),
        )).id();
        commands.entity(grants_row).add_child(grant_icon_node);
        let grant_title_node = commands.spawn((
            Text::new(grant_title.clone()),
            TextFont::from_font_size(10.),
            TextColor::from(Color::linear_rgba(0.85, 0.85, 0.85, 1.)),
            TextLayout::new_with_linebreak(LineBreak::NoWrap),
        )).id();
        commands.entity(grants_row).add_child(grant_title_node);
    }

    let bar_bg = commands.spawn((
        Node {
            width: Val::Px(220.),
            height: Val::Px(10.),
            border: UiRect::all(Val::Px(1.)),
            border_radius: BorderRadius::all(Val::Px(2.)),
            ..default()
        },
        BackgroundColor::from(Color::linear_rgba(0.1, 0.1, 0.1, 0.8)),
        BorderColor::from(Color::linear_rgba(0.4, 0.4, 0.3, 1.)),
    )).id();
    commands.entity(column).add_child(bar_bg);

    let fill = commands.spawn((
        Node {
            width: Val::Percent(fraction * 100.),
            height: Val::Percent(100.),
            ..default()
        },
        BackgroundColor::from(Color::linear_rgba(0.3, 0.6, 0.9, 1.)),
        ResearchCardProgressFill { research },
    )).id();
    commands.entity(bar_bg).add_child(fill);

    let button = commands.spawn((
        ResearchStartStopButton { research },
        Node {
            padding: UiRect::axes(Val::Px(10.), Val::Px(5.)),
            border_radius: BorderRadius::all(Val::Px(3.)),
            align_self: AlignSelf::Center,
            ..default()
        },
        BackgroundColor::from(Color::linear_rgba(0.2, 0.3, 0.5, 0.9)),
        children![(
            Text::new(button_label),
            TextFont::from_font_size(12.),
        )],
    )).observe(ResearchStartStopButton::on_click).id();
    commands.entity(card).add_child(button);

    card
}
