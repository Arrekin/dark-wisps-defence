//! # Research Panel
//!
//! The full-screen shell: header, the band of detail views, and the grid of
//! tiles. The panel owns layout and selection; it does not own what a detail
//! view shows. It spawns each view bound to a research marker
//! (`ResearchDetailViewSource`) and the view follows that marker from there, so
//! the panel has no per-view logic and no refresh plumbing.
//!
//! Selection is a component on the research rather than an entity stored here:
//! `ResearchUISelected` gives the "at most one" invariant for free, and a
//! selected research despawning removes the marker, which empties the view.

use bevy::prelude::*;

use game_core::prelude::DisplayName;
use research::prelude::{ResearchActive, ResearchUISelected};
use states::prelude::UiInteraction;
use widgets::utils::set_ui_free_on;

use super::{
    detail_view::{BuilderResearchDetailView, ResearchDetailViewSource},
    tile::{ResearchTile, ResearchTileOf, ResearchTileSelected, ResearchTilesNeedOrdering},
};

pub(crate) struct ResearchPanelPlugin;
impl Plugin for ResearchPanelPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Startup, spawn_research_panel)
            .add_systems(OnEnter(UiInteraction::ResearchPanel), show_panel)
            .add_systems(OnExit(UiInteraction::ResearchPanel), hide_panel)
            .add_observer(on_research_tile_selected_move_selection)
            .add_observer(on_research_tiles_need_ordering);
    }
}

// ============================================================================
// CONSTANTS
// ============================================================================

const PANEL_PADDING: f32 = 24.0;
const PANEL_BACKGROUND: Color = Color::linear_rgba(0.08, 0.08, 0.12, 1.);
const PANEL_Z_INDEX: i32 = 100;

const HEADER_HEIGHT: f32 = 48.0;
const HEADER_FONT_SIZE: f32 = 28.0;
const HEADER_COLOR: Color = Color::linear_rgba(0.9, 0.9, 0.9, 1.);

const CLOSE_BUTTON_SIZE: f32 = 28.0;
const CLOSE_BUTTON_FONT_SIZE: f32 = 16.0;
const CLOSE_BUTTON_BACKGROUND: Color = Color::linear_rgba(0.3, 0.3, 0.3, 1.);

const BAND_HEIGHT: f32 = 260.0;
const BAND_COLUMN_GAP: f32 = 16.0;

const TILE_GRID_GAP: f32 = 4.0;

const ACTIVE_VIEW_TITLE: &str = "Active research";
const ACTIVE_VIEW_EMPTY_TEXT: &str = "Nothing is being researched.";
const SELECTED_VIEW_TITLE: &str = "Selected";
const SELECTED_VIEW_EMPTY_TEXT: &str = "Select a research tile to inspect it.";

// ============================================================================
// COMPONENTS
// ============================================================================

/// Marker on the full-screen panel root.
#[derive(Component)]
pub(crate) struct ResearchPanelRoot;

/// Marker on the row holding the detail views.
#[derive(Component)]
pub(crate) struct ResearchBand;

/// Marker on the grid that holds research tiles.
#[derive(Component)]
pub(crate) struct ResearchTileGrid;

/// Marker on the close button.
#[derive(Component)]
#[require(Button, Pickable)]
struct ResearchPanelCloseButton;

// ============================================================================
// SPAWN
// ============================================================================

fn spawn_research_panel(mut commands: Commands) {
    let header = spawn_header(&mut commands);
    let band = spawn_band(&mut commands);
    let grid = spawn_tile_grid(&mut commands);

    commands.spawn((
        ResearchPanelRoot,
        Node {
            width: Val::Percent(100.),
            height: Val::Percent(100.),
            position_type: PositionType::Absolute,
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(Val::Px(PANEL_PADDING)),
            row_gap: Val::Px(PANEL_PADDING),
            display: Display::None,
            ..default()
        },
        BackgroundColor::from(PANEL_BACKGROUND),
        GlobalZIndex(PANEL_Z_INDEX),
    )).add_children(&[header, band, grid]);
}

fn spawn_header(commands: &mut Commands) -> Entity {
    let title = commands.spawn((
        Text::new("Research"),
        TextFont::from_font_size(HEADER_FONT_SIZE),
        TextColor::from(HEADER_COLOR),
    )).id();

    let close_button = commands.spawn((
        ResearchPanelCloseButton,
        Node {
            width: Val::Px(CLOSE_BUTTON_SIZE),
            height: Val::Px(CLOSE_BUTTON_SIZE),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            ..default()
        },
        BackgroundColor::from(CLOSE_BUTTON_BACKGROUND),
        children![(
            Text::new("X"),
            TextFont::from_font_size(CLOSE_BUTTON_FONT_SIZE),
            TextColor::from(Color::WHITE),
            TextLayout::no_wrap(),
        )],
    )).observe(set_ui_free_on::<Pointer<Click>>).id();

    commands.spawn(Node {
        width: Val::Percent(100.),
        height: Val::Px(HEADER_HEIGHT),
        flex_direction: FlexDirection::Row,
        justify_content: JustifyContent::SpaceBetween,
        align_items: AlignItems::Center,
        ..default()
    }).add_children(&[title, close_button]).id()
}

/// The two views differ only in their marker binding and their labels; nothing
/// else about the band knows which is which.
fn spawn_band(commands: &mut Commands) -> Entity {
    let active_view = commands.spawn((
        BuilderResearchDetailView::new(ACTIVE_VIEW_TITLE, ACTIVE_VIEW_EMPTY_TEXT),
        ResearchDetailViewSource::<ResearchActive>::default(),
    )).id();
    let selected_view = commands.spawn((
        BuilderResearchDetailView::new(SELECTED_VIEW_TITLE, SELECTED_VIEW_EMPTY_TEXT),
        ResearchDetailViewSource::<ResearchUISelected>::default(),
    )).id();

    commands.spawn((
        ResearchBand,
        Node {
            width: Val::Percent(100.),
            height: Val::Px(BAND_HEIGHT),
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(BAND_COLUMN_GAP),
            ..default()
        },
    )).add_children(&[active_view, selected_view]).id()
}

fn spawn_tile_grid(commands: &mut Commands) -> Entity {
    commands.spawn((
        ResearchTileGrid,
        Node {
            width: Val::Percent(100.),
            flex_grow: 1.,
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            align_content: AlignContent::FlexStart,
            row_gap: Val::Px(TILE_GRID_GAP),
            column_gap: Val::Px(TILE_GRID_GAP),
            ..default()
        },
    )).id()
}

// ============================================================================
// SELECTION
// ============================================================================

/// Moves `ResearchUISelected` to the clicked tile's research. The removal is
/// queued before the insert, so the view bound to the marker clears and then
/// repopulates in that order.
///
/// Re-clicking the selected tile returns early: moving the marker onto the
/// research already holding it would still fire both lifecycle events and cost
/// a full content rebuild for no visible change.
fn on_research_tile_selected_move_selection(
    trigger: On<ResearchTileSelected>,
    mut commands: Commands,
    tiles: Query<&ResearchTile>,
    current: Option<Single<Entity, With<ResearchUISelected>>>,
) {
    let Ok(tile) = tiles.get(trigger.tile) else { return };

    if let Some(current) = current {
        let current = current.into_inner();
        if current == tile.research { return }
        commands.entity(current).remove::<ResearchUISelected>();
    }
    commands.entity(tile.research).insert(ResearchUISelected);
}

// ============================================================================
// SHOW / HIDE
// ============================================================================

fn show_panel(root: Single<&mut Node, With<ResearchPanelRoot>>) {
    root.into_inner().display = Display::Flex;
}

fn hide_panel(root: Single<&mut Node, With<ResearchPanelRoot>>) {
    root.into_inner().display = Display::None;
}

// ============================================================================
// TILE ORDERING — grid positions tiles by DisplayName
// ============================================================================

fn on_research_tiles_need_ordering(
    _: On<ResearchTilesNeedOrdering>,
    mut commands: Commands,
    grid: Single<(Entity, &Children), With<ResearchTileGrid>>,
    tile_ofs: Query<&ResearchTileOf>,
    research_names: Query<&DisplayName>,
) {
    let (grid_entity, grid_children) = grid.into_inner();

    let mut sorted: Vec<(String, Entity)> = grid_children.iter()
        .filter_map(|child| {
            let tile_of = tile_ofs.get(child).ok()?;
            let name = research_names.get(tile_of.0).map(|name| name.0.clone()).unwrap_or_default();
            Some((name, child))
        })
        .collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));

    let sorted_entities: Vec<Entity> = sorted.into_iter().map(|(_, entity)| entity).collect();
    commands.entity(grid_entity).replace_children(&sorted_entities);
}
