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
            .apply_scene(bsn! {
                Node {
                    position_type: PositionType::Absolute,
                    top: Val::Px(0.),
                    left: Val::Px(0.),
                    width: Val::Percent(100.),
                    height: Val::Percent(100.),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                }
                BackgroundColor(Color::linear_rgba(0., 0., 0., 0.6))
                Visibility::Hidden
                Children [
                    (
                        Node {
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Center,
                            row_gap: Val::Px(10.),
                            padding: UiRect::all(Val::Px(16.)),
                            min_width: Val::Px(380.),
                            border_radius: BorderRadius::all(Val::Px(6.)),
                        }
                        BackgroundColor(Color::linear_rgba(0.1, 0.1, 0.15, 0.98))
                        Children [
                            (
                                Text("Research")
                                TextFont { font_size: {18.0} }
                            ),
                            (
                                Node {
                                    flex_direction: FlexDirection::Column,
                                    align_items: AlignItems::Stretch,
                                    row_gap: Val::Px(8.),
                                }
                                ResearchPanelContent
                            ),
                        ]
                    )
                ]
            });
    }
}

/// Container the cards are (re)built into.
#[derive(Component, Default, Clone)]
pub struct ResearchPanelContent;

/// Marks the progress-bar fill of a card so it can be animated live for `research`.
#[derive(Component, FromTemplate, Clone)]
struct ResearchCardProgressFill {
    research: Entity,
}

/// Start/Stop button on a card, bound to `research`.
#[derive(Component, FromTemplate, Clone)]
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

    let mut entries: Vec<_> = researches.iter().collect();
    entries.sort_by(|a, b| a.1.title.cmp(&b.1.title));

    for (research_entity, display, outcomes, is_active, progress) in entries {
        let grants: Vec<(Handle<Image>, String)> = outcomes.iter()
            .filter_map(|outcome| outcome_displays.get(outcome).ok())
            .map(|outcome_display| (outcome_display.icon.clone(), outcome_display.title.clone()))
            .collect();
        let fraction = progress.map(|p| p.fraction).unwrap_or(0.0);
        let card = commands.spawn_scene(research_card(research_entity, display, is_active, fraction, grants)).id();
        commands.entity(content_entity).add_child(card);
    }
}

fn research_card(
    research: Entity,
    display: &ResearchCardDisplay,
    is_active: bool,
    fraction: f32,
    grants: Vec<(Handle<Image>, String)>,
) -> impl Scene {
    let button_label = if is_active { "Stop" } else { "Start" };
    let grant_scenes: Vec<_> = grants.into_iter()
        .map(|(icon, title)| bsn! {
            Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(2.),
            }
            Children [
                (
                    Node { width: Val::Px(20.), height: Val::Px(20.) }
                    ImageNode { image: {icon} }
                ),
                (
                    Text({title})
                    TextFont { font_size: {10.0} }
                    TextColor(Color::linear_rgba(0.85, 0.85, 0.85, 1.))
                    template_value(TextLayout::no_wrap())
                )
            ]
        })
        .collect();

    bsn! {
        Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(10.),
            padding: UiRect::all(Val::Px(8.)),
            border_radius: BorderRadius::all(Val::Px(4.)),
        }
        BackgroundColor(Color::linear_rgba(0.15, 0.15, 0.2, 0.9))
        Children [
            (
                Node { width: Val::Px(48.), height: Val::Px(48.) }
                ImageNode { image: {display.icon.clone()} }
            ),
            (
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Start,
                    row_gap: Val::Px(4.),
                    min_width: Val::Px(220.),
                }
                Children [
                    (
                        Text({display.title.clone()})
                        TextFont { font_size: {14.0} }
                        template_value(TextLayout::no_wrap())
                    ),
                    (
                        Node {
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            column_gap: Val::Px(4.),
                        }
                        Children [
                            (
                                Text("Grants:")
                                TextFont { font_size: {10.0} }
                                TextColor(Color::linear_rgba(0.7, 0.7, 0.7, 1.))
                                template_value(TextLayout::no_wrap())
                            ),
                            {grant_scenes}
                        ]
                    ),
                    (
                        Node {
                            width: Val::Px(220.),
                            height: Val::Px(10.),
                            border: UiRect::all(Val::Px(1.)),
                            border_radius: BorderRadius::all(Val::Px(2.)),
                        }
                        BackgroundColor(Color::linear_rgba(0.1, 0.1, 0.1, 0.8))
                        template_value(BorderColor::from(Color::linear_rgba(0.4, 0.4, 0.3, 1.)))
                        Children [
                            (
                                Node {
                                    width: Val::Percent({fraction * 100.}),
                                    height: Val::Percent(100.),
                                }
                                BackgroundColor(Color::linear_rgba(0.3, 0.6, 0.9, 1.))
                                ResearchCardProgressFill { research: {research} }
                            )
                        ]
                    ),
                ]
            ),
            (
                ResearchStartStopButton { research: {research} }
                Node {
                    padding: UiRect::axes(Val::Px(10.), Val::Px(5.)),
                    border_radius: BorderRadius::all(Val::Px(3.)),
                    align_self: AlignSelf::Center,
                }
                BackgroundColor(Color::linear_rgba(0.2, 0.3, 0.5, 0.9))
                on(ResearchStartStopButton::on_click)
                Children [
                    ( Text({button_label}) TextFont { font_size: {12.0} } )
                ]
            )
        ]
    }
}
