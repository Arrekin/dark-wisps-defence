//! # Research Detail View
//!
//! The card showing one research in full: icon, name, description, progress bar,
//! status, remaining cost, grants and the action button. The panel's band holds two
//! cards; [`super::active`] adds the effects of the card following the running research.
//!
//! ## Which research a card shows
//!
//! Each card is spawned with a [`ResearchDetailViewSource`] naming a marker component and
//! shows the research carrying that marker: `ResearchActive` for the active card,
//! `ResearchUISelected` for the selected one. Each marker is on at most one research. When
//! the marker moves, the card follows it; when no research carries it, the card shows its
//! empty text.
//!
//! A new card needs a marker, a spawn with its title and empty text, and an
//! `add_source_observers` call for the marker.
//!
//! ## Updates
//!
//! When the shown research changes, the card's content is despawned and built anew. While
//! the panel is open, the progress percent, remaining time and cost chips update in place
//! every frame.

use std::marker::PhantomData;

use bevy::prelude::*;

use game_core::prelude::{DisplayDescription, DisplayIcon, DisplayName};
use outcomes::prelude::HasOutcomes;
use research::{
    prelude::{Research, ResearchActive, ResearchDisplayDataUpdated, ResearchRuntime, ResearchUISelected},
    research_bar::BuilderResearchBar,
};
use resources::prelude::ResourceAmount;
use states::prelude::{GameState, UiInteraction};
use widgets::{
    common::utils::set_text_if_changed,
    prelude::{
        BuilderChipStrip, BuilderCostChip, BuilderDisplayChip, BuilderVoidPanel, CostChip,
        CostChipVisualUnitAvailable, TextRole, text_font,
    },
};

use crate::{process::units_paid, ui::action_button::ResearchActionButton};

pub(crate) struct DetailViewShellPlugin;
impl Plugin for DetailViewShellPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_observer(on_builder_add_spawn_research_detail_view)
            .add_observer(on_builder_insert_rebuild_research_detail_view_content)
            .add_observer(on_show_research_in_detail_view_rebuild_content)
            .add_systems(Update, (
                    update_research_detail_view_status,
                    update_research_cost_chips,
                ).run_if(in_state(GameState::Running).and_then(in_state(UiInteraction::ResearchPanel)))
            );

        Self::add_source_observers::<ResearchActive>(app);
        Self::add_source_observers::<ResearchUISelected>(app);
    }
}

impl DetailViewShellPlugin {
    /// Registers observers that synchronize views bound to `Marker` when the marker is added,
    /// removed, or its research's display data changes.
    fn add_source_observers<Marker: Component>(app: &mut App) {
        app
            .add_observer(on_insert_research_marker_show_in_detail_view::<Marker>)
            .add_observer(on_remove_research_marker_clear_detail_view::<Marker>)
            .add_observer(on_research_display_data_updated_refresh_detail_view::<Marker>);
    }
}

// ============================================================================
// CONSTANTS
// ============================================================================

// View shell
const VIEW_PADDING: f32 = 16.0;
const VIEW_ROW_GAP: f32 = 4.0;
/// Larger than a tile's 12px — the card is a much bigger surface and the cut should read
/// as the same gesture at a larger scale.
const VIEW_CORNER_CUT: f32 = 22.0;
const VIEW_TITLE_FONT_SIZE: f32 = 14.0;

/// Shared by the title, the stall line and the empty state — the three labels
/// that frame the content rather than being it.
const MUTED_TEXT_COLOR: Color = Color::srgba(0.65, 0.65, 0.72, 1.);

// Identity row
const IDENTITY_ROW_HEIGHT: f32 = 64.0;
const IDENTITY_ROW_COLUMN_GAP: f32 = 8.0;
const ICON_SIZE: f32 = 64.0;
const NAME_FONT_SIZE: f32 = 16.0;

// Description
const DESCRIPTION_HEIGHT: f32 = 36.0;
const DESCRIPTION_FONT_SIZE: f32 = 12.0;
const DESCRIPTION_COLOR: Color = Color::srgba(0.75, 0.75, 0.8, 1.);

// Progress and status
const PROGRESS_ROW_HEIGHT: f32 = 36.0;
/// Height of the bar itself within its row — an instrument reads better trim than the
/// row, which is tall enough to give it breathing room above and below.
const PROGRESS_BAR_HEIGHT: f32 = 16.0;
const STATUS_ROW_HEIGHT: f32 = 18.0;
const STATUS_FONT_SIZE: f32 = 12.0;
const STALL_ROW_HEIGHT: f32 = 18.0;

// Bottom row
const BOTTOM_ROW_COLUMN_GAP: f32 = 12.0;
const STRIP_LABEL_FONT_SIZE: f32 = 12.0;
/// The detail view has room the tile does not, so it gives its action button a container
/// of its own rather than letting it fill a cramped row.
const ACTION_BUTTON_WIDTH: f32 = 96.0;
const ACTION_BUTTON_HEIGHT: f32 = 28.0;

// Empty state
const EMPTY_TEXT_FONT_SIZE: f32 = 14.0;

// ============================================================================
// COMPONENTS
// ============================================================================

/// Spawn contract for a detail view. Replaced by [`ResearchDetailView`] once
/// the shell is built.
///
/// The caller pairs this with a [`ResearchDetailViewSource`] to say which
/// research the view follows; the two are independent because the shell has no
/// use for the marker and the source wiring has no use for the layout.
#[derive(Component, Clone, Copy)]
pub(crate) struct BuilderResearchDetailView {
    /// Header label, above the content and outside it — it survives rebuilds.
    title: &'static str,
    /// Shown centred in place of the content while no research is bound.
    empty_text: &'static str,
}

/// Runtime component for a built view. `content` is the child the subject tree
/// hangs from, kept so a rebuild can clear it without disturbing the title.
#[derive(Component, Clone, Copy, FromTemplate)]
pub(super) struct ResearchDetailView {
    pub(super) content: Entity,
    empty_text: &'static str,
}

/// Binds a view to the research marker that feeds it. Carries no data — the
/// marker type is the whole statement.
#[derive(Component)]
pub(crate) struct ResearchDetailViewSource<Marker: Component>(PhantomData<Marker>);

/// Spawn contract for the content tree, inserted on the view's content child.
/// `research` of `None` builds the empty state, which is the startup shape and
/// the one every view falls back to when its marker moves away.
#[derive(Component, Clone, Copy)]
struct BuilderResearchDetailViewContent {
    research: Option<Entity>,
    empty_text: &'static str,
}

/// The subject a content tree was built for and the labels worth rewriting in
/// place. Present only on a populated tree, so a view showing nothing has no
/// status nodes to be wrong about.
#[derive(Component, Clone, Copy, FromTemplate)]
pub(super) struct ResearchDetailViewContent {
    pub(super) research: Entity,
    percent_text: Entity,
    remaining_time_text: Entity,
    /// The bar's container node — same rect as the bar widget itself, since it
    /// fills the container at 100% with no padding.
    pub(super) progress_bar: Entity,
}

/// Carries the original spec cost so the chip's displayed amount can be
/// recomputed each frame from how far the research has progressed.
///
/// `research` is the entity the chip reflects.
#[derive(Component, Clone)]
struct ResearchCostChip {
    research: Entity,
    cost: ResourceAmount,
}

/// Points a view at a research, or at nothing. The only way content changes.
#[derive(EntityEvent, Clone, Copy)]
struct ShowResearchInDetailView {
    #[event_target]
    view: Entity,
    research: Option<Entity>,
}

impl BuilderResearchDetailView {
    pub(crate) fn new(title: &'static str, empty_text: &'static str) -> Self {
        Self { title, empty_text }
    }
}

impl<Marker: Component> Default for ResearchDetailViewSource<Marker> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<Marker: Component> Clone for ResearchDetailViewSource<Marker> {
    fn clone(&self) -> Self {
        Self(PhantomData)
    }
}

// ============================================================================
// SOURCE WIRING — generic over the marker, so no source has its own code path
// ============================================================================

/// The research that just gained the marker is the one to show, and it is the
/// trigger target, so nothing has to be searched for.
fn on_insert_research_marker_show_in_detail_view<Marker: Component>(
    trigger: On<Insert<Marker>>,
    mut commands: Commands,
    view: Single<Entity, With<ResearchDetailViewSource<Marker>>>,
) {
    commands.trigger(ShowResearchInDetailView {
        view: *view,
        research: Some(trigger.entity),
    });
}

/// At most one research carries `Marker` and it is the one losing it, so the
/// view is left with nothing to show. Resolving that here rather than querying
/// also sidesteps `On<Remove>` firing while the component is still attached.
fn on_remove_research_marker_clear_detail_view<Marker: Component>(
    _: On<Remove<Marker>>,
    mut commands: Commands,
    view: Single<Entity, With<ResearchDetailViewSource<Marker>>>,
) {
    commands.trigger(ShowResearchInDetailView { view: *view, research: None });
}

/// Rebuilds only when the research that changed is the one this view shows.
/// Display data lands on every research as it is enabled, so an unfiltered
/// rebuild would throw away both views' content on every seed.
fn on_research_display_data_updated_refresh_detail_view<Marker: Component>(
    trigger: On<ResearchDisplayDataUpdated>,
    mut commands: Commands,
    marked: Query<(), With<Marker>>,
    view: Single<Entity, With<ResearchDetailViewSource<Marker>>>,
) {
    if !marked.contains(trigger.research) { return }

    commands.trigger(ShowResearchInDetailView {
        view: *view,
        research: Some(trigger.research),
    });
}

fn on_show_research_in_detail_view_rebuild_content(
    trigger: On<ShowResearchInDetailView>,
    mut commands: Commands,
    views: Query<&ResearchDetailView>,
) {
    let Ok(view) = views.get(trigger.view) else { return };

    commands.entity(view.content).insert(BuilderResearchDetailViewContent {
        research: trigger.research,
        empty_text: view.empty_text,
    });
}

// ============================================================================
// SHELL BUILD
// ============================================================================

/// Builds the title and the content child, then hands the content over to the
/// content builder in its empty shape. A freshly spawned view is therefore
/// already correct before any marker exists.
fn on_builder_add_spawn_research_detail_view(
    trigger: On<Add<BuilderResearchDetailView>>,
    mut commands: Commands,
    builders: Query<&BuilderResearchDetailView>,
) {
    let view_entity = trigger.entity;
    let Ok(builder) = builders.get(view_entity) else { return };
    let BuilderResearchDetailView { title, empty_text } = *builder;
    let content_builder = BuilderResearchDetailViewContent { research: None, empty_text };

    commands.entity(view_entity)
        .remove::<BuilderResearchDetailView>()
        .apply_scene(bsn! {
            ResearchDetailView { content: #Content, empty_text: empty_text }
            Node {
                height: Val::Percent(100.),
                flex_grow: 1.,
                flex_basis: Val::Px(0.),
                min_width: Val::Px(0.),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(VIEW_PADDING)),
                row_gap: Val::Px(VIEW_ROW_GAP),
            }
            BuilderVoidPanel::default().with_corner_cut(VIEW_CORNER_CUT)
            Children [
                Text(title)
                @text_font(TextRole::Heading, VIEW_TITLE_FONT_SIZE)
                TextColor(MUTED_TEXT_COLOR)
                TextLayout::no_wrap()
                --
                #Content
                Node {
                    flex_grow: 1.,
                    min_height: Val::Px(0.),
                    overflow: Overflow::clip_y(),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(VIEW_ROW_GAP),
                }
                ~{content_builder}
            ]
        });
}

// ============================================================================
// CONTENT BUILD
// ============================================================================

/// Replaces a view's content with the card for the builder's research, or with the
/// empty state when no research is bound or its display data has not arrived yet.
/// `DisplayIcon` is inserted after the research spawns, and `ResearchDisplayDataUpdated`
/// then rebuilds the card.
fn on_builder_insert_rebuild_research_detail_view_content(
    trigger: On<Insert<BuilderResearchDetailViewContent>>,
    mut commands: Commands,
    builders: Query<&BuilderResearchDetailViewContent>,
    researches: Query<(Entity, &DisplayName, &DisplayDescription, &DisplayIcon, &Research, Option<&HasOutcomes>)>,
) {
    let content_entity = trigger.entity;
    let Ok(builder) = builders.get(content_entity) else { return };
    let BuilderResearchDetailViewContent { research, empty_text } = *builder;

    let mut content = commands.entity(content_entity);
    content
        .remove::<(BuilderResearchDetailViewContent, ResearchDetailViewContent)>()
        .despawn_children();

    let Some((research, name, description, icon, research_data, has_outcomes)) =
        research.and_then(|research| researches.get(research).ok())
    else {
        content.apply_scene(bsn! { Children [ @empty_state(empty_text) ] });
        return;
    };

    let (name, description, icon) = (name.0.clone(), description.0.clone(), icon.0.clone());
    let research_bar = BuilderResearchBar::new(research);
    let bottom_row = bottom_row(research, &research_data.cost, has_outcomes);

    content.apply_scene(bsn! {
        ResearchDetailViewContent {
            research: research,
            percent_text: #Percent,
            remaining_time_text: #RemainingTime,
            progress_bar: #ProgressBar,
        }
        Children [
            @identity_row(name, icon)
            --
            @description_row(description)
            --
            Node { height: Val::Px(PROGRESS_ROW_HEIGHT), align_items: AlignItems::Center }
            Children [
                #ProgressBar
                Node { width: Val::Percent(100.), height: Val::Px(PROGRESS_BAR_HEIGHT) }
                Children [ ~{research_bar} ]
            ]
            --
            Node { height: Val::Px(STATUS_ROW_HEIGHT), flex_direction: FlexDirection::Row, justify_content: JustifyContent::SpaceBetween }
            Children [
                #Percent
                Text("--")
                @text_font(TextRole::Data, STATUS_FONT_SIZE)
                --
                #RemainingTime
                Text("--")
                @text_font(TextRole::Data, STATUS_FONT_SIZE)
            ]
            --
            @stall_row()
            --
            @bottom_spacer()
            --
            @bottom_row
        ]
    });
}

fn identity_row(name: String, icon: Handle<Image>) -> impl Scene {
    bsn! {
        Node {
            height: Val::Px(IDENTITY_ROW_HEIGHT),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(IDENTITY_ROW_COLUMN_GAP),
        }
        Children [
            ImageNode { image: icon }
            Node { width: Val::Px(ICON_SIZE), height: Val::Px(ICON_SIZE) }
            --
            Text(name)
            @text_font(TextRole::Body, NAME_FONT_SIZE)
            TextColor(Color::WHITE)
            TextLayout::no_wrap()
        ]
    }
}

fn description_row(description: String) -> impl Scene {
    bsn! {
        Node { height: Val::Px(DESCRIPTION_HEIGHT), overflow: Overflow::clip_y() }
        Children [
            Text(description)
            @text_font(TextRole::Body, DESCRIPTION_FONT_SIZE)
            TextColor(DESCRIPTION_COLOR)
        ]
    }
}

/// Reserved height so the band does not shift when the real indicator lands.
/// The static text describes what belongs here; blocker detection is a separate
/// feature.
fn stall_row() -> impl Scene {
    bsn! {
        Text("Stalled: --")
        Node { height: Val::Px(STALL_ROW_HEIGHT) }
        @text_font(TextRole::Data, STATUS_FONT_SIZE)
        TextColor(MUTED_TEXT_COLOR)
    }
}

/// Absorbs the leftover vertical space so the bottom row sits on the floor of
/// the view regardless of how tall the content above it turned out.
fn bottom_spacer() -> impl Scene {
    bsn! {
        Node { flex_grow: 1., min_height: Val::Px(0.) }
    }
}

fn bottom_row(research: Entity, costs: &[ResourceAmount], grants: Option<&HasOutcomes>) -> impl Scene {
    let remaining_chips: Vec<_> = costs.iter().copied().map(|cost| {
        let (chip, research_cost_chip) = (BuilderCostChip(cost), ResearchCostChip { research, cost });
        bsn! { ~{chip} CostChipVisualUnitAvailable ~{research_cost_chip} }
    }).collect();
    let grant_chips: Vec<_> = grants.into_iter().flat_map(|grants| grants.iter())
        .map(|grant| {
            let chip = BuilderDisplayChip(grant);
            bsn! { ~{chip} }
        })
        .collect();
    let action_button = ResearchActionButton::new(research);

    bsn! {
        Node { flex_direction: FlexDirection::Row, align_items: AlignItems::FlexEnd, column_gap: Val::Px(BOTTOM_ROW_COLUMN_GAP) }
        Children [
            Node { flex_grow: 1., flex_basis: Val::Px(0.), flex_direction: FlexDirection::Row, column_gap: Val::Px(BOTTOM_ROW_COLUMN_GAP) }
            Children [
                @labelled_strip("Remaining", remaining_chips)
                --
                @labelled_strip("Grants", grant_chips)
            ]
            --
            Node { width: Val::Px(ACTION_BUTTON_WIDTH), height: Val::Px(ACTION_BUTTON_HEIGHT) }
            Children [ ~{action_button} ]
        ]
    }
}

/// A caption over a chip strip. The two strips differ only in their label and
/// their chips, so the column itself is built once.
fn labelled_strip(label: &'static str, chips: impl SceneList) -> impl Scene {
    bsn! {
        Node { flex_grow: 1., flex_basis: Val::Px(0.), flex_direction: FlexDirection::Column }
        Children [
            Text(label)
            @text_font(TextRole::Heading, STRIP_LABEL_FONT_SIZE)
            --
            BuilderChipStrip
            Children [ {chips} ]
        ]
    }
}

fn empty_state(text: &'static str) -> impl Scene {
    bsn! {
        Node { flex_grow: 1., justify_content: JustifyContent::Center, align_items: AlignItems::Center }
        Children [
            Text(text)
            @text_font(TextRole::Body, EMPTY_TEXT_FONT_SIZE)
            TextColor(MUTED_TEXT_COLOR)
            TextLayout::no_wrap()
        ]
    }
}

// ============================================================================
// LIVE UPDATES
// ============================================================================

/// Writes each populated view's progress percent and remaining time at full
/// supply. Gated to the open panel — a label never has to be correct while
/// closed.
///
/// Both values derive from the same `ResearchRuntime.progress` and
/// `Research.duration` the tick acts on, so the display cannot drift from the
/// behaviour. The percent is floored, never rounded, so a research stalled on
/// its final unit at `0.999` reads `99%` rather than `100%`. Remaining time is
/// `duration * (1 - progress)`, ceiled so it holds at `1s` until the final
/// frame and drops to `0s` exactly on completion.
fn update_research_detail_view_status(
    researches: Query<(&Research, &ResearchRuntime)>,
    contents: Query<&ResearchDetailViewContent>,
    mut texts: Query<&mut Text>,
) {
    for content in contents.iter() {
        let Ok((research, runtime)) = researches.get(content.research) else { continue };

        let percent = (runtime.progress * 100.).floor() as u32;
        let remaining_seconds = (research.duration.as_secs_f32() * (1. - runtime.progress)).ceil() as u32;

        if let Ok(mut text) = texts.get_mut(content.percent_text) {
            set_text_if_changed(&mut text, &format!("{percent}%"));
        }
        if let Ok(mut text) = texts.get_mut(content.remaining_time_text) {
            set_text_if_changed(&mut text, &format!("{remaining_seconds}s at full supply"));
        }
    }
}

/// Writes the remaining amount into each cost chip from the progress of the
/// research it belongs to. Gated to the open panel like `sync_research_bars`
/// in `research_bar.rs` — a chip never has to be correct while closed.
///
/// Reads `ResearchRuntime` directly rather than filtering on `ResearchActive`
/// so a view showing a parked-but-started research reflects it too. Both
/// `ResearchAvailable` and `ResearchActive` require `ResearchRuntime`, so any
/// research that has ever progressed has one.
fn update_research_cost_chips(
    runtimes: Query<&ResearchRuntime>,
    mut chips: Query<(&ResearchCostChip, &mut CostChip)>,
) {
    for (research_cost_chip, mut chip) in chips.iter_mut() {
        // No runtime yet — the chip retains its spawn-time full price.
        let Ok(runtime) = runtimes.get(research_cost_chip.research) else { continue };

        let cost = &research_cost_chip.cost;
        let remaining = cost.amount - units_paid(runtime.progress, cost);
        // Assigning unconditionally would mark every chip changed every frame
        // and defeat the widget's `Changed<CostChip>` gate.
        if chip.cost.amount != remaining {
            chip.cost.amount = remaining;
        }
    }
}
