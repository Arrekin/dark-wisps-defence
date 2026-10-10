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
use widgets::{
    common::utils::set_ui_free_on,
    palette::ABYSS_BACKGROUND,
    prelude::{BuilderCloseButton, TextRole, text_font},
};

use super::{
    detail_view::{BuilderResearchDetailView, ResearchDetailViewSource},
    tile::{ResearchTileOf, ResearchTilesNeedOrdering},
};

pub(crate) struct ResearchPanelPlugin;
impl Plugin for ResearchPanelPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Startup, spawn_research_panel)
            .add_systems(OnEnter(UiInteraction::ResearchPanel), show_panel)
            .add_systems(OnExit(UiInteraction::ResearchPanel), hide_panel)
            .add_observer(on_research_tiles_need_ordering_do_so);
    }
}

// ============================================================================
// CONSTANTS
// ============================================================================

const PANEL_PADDING: f32 = 24.0;
const PANEL_Z_INDEX: i32 = 100;

const HEADER_HEIGHT: f32 = 48.0;
const HEADER_FONT_SIZE: f32 = 28.0;
const HEADER_COLOR: Color = Color::srgba(0.9, 0.9, 0.9, 1.);

const CLOSE_BUTTON_SIZE: f32 = 28.0;

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
#[derive(Component, Default, Clone)]
pub(crate) struct ResearchPanelRoot;

/// Marker on the row holding the detail views.
#[derive(Component, Default, Clone)]
pub(crate) struct ResearchBand;

/// Marker on the grid that holds research tiles.
#[derive(Component, Default, Clone)]
pub(crate) struct ResearchTileGrid;

/// Marker on the close button.
#[derive(Component, Default, Clone)]
struct ResearchPanelCloseButton;

// ============================================================================
// SPAWN
// ============================================================================

fn spawn_research_panel(mut commands: Commands) {
    let active_view = BuilderResearchDetailView::new(ACTIVE_VIEW_TITLE, ACTIVE_VIEW_EMPTY_TEXT);
    let selected_view = BuilderResearchDetailView::new(SELECTED_VIEW_TITLE, SELECTED_VIEW_EMPTY_TEXT);

    commands.spawn_scene(bsn! {
        ResearchPanelRoot
        Node {
            width: Val::Percent(100.),
            height: Val::Percent(100.),
            position_type: PositionType::Absolute,
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(Val::Px(PANEL_PADDING)),
            row_gap: Val::Px(PANEL_PADDING),
            display: Display::None,
        }
        // A darker background makes the cards and tiles appear raised.
        BackgroundColor(ABYSS_BACKGROUND)
        GlobalZIndex(PANEL_Z_INDEX)
        Children [
            Node {
                width: Val::Percent(100.),
                height: Val::Px(HEADER_HEIGHT),
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Center,
            }
            Children [
                Text("Research")
                @text_font(TextRole::Heading, HEADER_FONT_SIZE)
                TextColor(HEADER_COLOR)
                --
                ResearchPanelCloseButton
                BuilderCloseButton::default()
                Node { width: Val::Px(CLOSE_BUTTON_SIZE), height: Val::Px(CLOSE_BUTTON_SIZE) }
                on(set_ui_free_on::<PointerClick>)
            ]
            --
            // The two views differ only in their marker binding and their labels; nothing
            // else about the band knows which is which.
            ResearchBand
            Node {
                width: Val::Percent(100.),
                height: Val::Px(BAND_HEIGHT),
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(BAND_COLUMN_GAP),
            }
            Children [
                ~{active_view}
                ResearchDetailViewSource::<ResearchActive>
                --
                ~{selected_view}
                ResearchDetailViewSource::<ResearchUISelected>
            ]
            --
            ResearchTileGrid
            Node {
                width: Val::Percent(100.),
                flex_grow: 1.,
                flex_direction: FlexDirection::Row,
                flex_wrap: FlexWrap::Wrap,
                align_content: AlignContent::FlexStart,
                row_gap: Val::Px(TILE_GRID_GAP),
                column_gap: Val::Px(TILE_GRID_GAP),
            }
        ]
    });
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

fn on_research_tiles_need_ordering_do_so(
    _: On<ResearchTilesNeedOrdering>,
    mut commands: Commands,
    grid: Single<(Entity, &Children), With<ResearchTileGrid>>,
    tiles: Query<&ResearchTileOf>,
    research_names: Query<&DisplayName>,
) {
    let (grid_entity, grid_children) = grid.into_inner();

    let mut named_tiles: Vec<(&str, Entity)> = grid_children.iter()
        .filter_map(|child| {
            let tile_of = tiles.get(child).ok()?;
            let name = research_names.get(tile_of.0).map(|name| name.0.as_str()).unwrap_or_default();
            Some((name, child))
        })
        .collect();
    named_tiles.sort_by_key(|(name, _)| *name);

    let sorted_entities: Vec<Entity> = named_tiles.into_iter().map(|(_, entity)| entity).collect();
    commands.entity(grid_entity).replace_children(&sorted_entities);
}
