use bevy::prelude::*;

use game_core::prelude::{DisplayIcon, DisplayName};
use outcomes::prelude::HasOutcomes;
use research::prelude::*;
use research::research_bar::BuilderResearchBar;
use widgets::prelude::{
    BuilderChipStrip, BuilderDisplayChip, BuilderVoidPanel, ChipsFaded, TextRole, VoidPanel,
    VoidPanelBorderSurge, VoidPanelStyle,
};

use super::{
    action_button::ResearchActionButton,
    panel::ResearchTileGrid,
};

pub(crate) struct ResearchTilePlugin;
impl Plugin for ResearchTilePlugin {
    fn build(&self, app: &mut App) {
        app
            .add_observer(on_add_research_tile_of_build_tile)
            .add_observer(on_insert_research_state_refresh_tile)
            .add_observer(on_add_research_ui_selected_light_tile)
            .add_observer(on_remove_research_ui_selected_dim_tile)
            .add_observer(on_add_research_active_surge_tile)
            .add_observer(on_remove_research_active_still_tile);
    }
}

/// Look of a completed research tile. Teal (`#42F5C8`) is the palette's rare-positive
/// accent. The signal that reads across a grid is `corner_mark`: a teal wedge in the
/// tile's bottom-right corner. `contour_scale` and `tint` pull the surface toward the
/// same colour so the mark isn't sitting on an otherwise-untouched panel.
const COMPLETED_STYLE: VoidPanelStyle = VoidPanelStyle {
    color: Color::Srgba(Srgba::new(0.259, 0.961, 0.784, 1.0)),
    field_scale: 1.0,
    contour_scale: 0.8,
    tint: 0.8,
    corner_mark: 10.0,
};

/// Border surges on the running research's tile. A tile's border is about a third the
/// length of a detail card's, so the span is scaled to cover the same share of it, and the
/// intensity is lower: the grid is where the eye rests while reading names, and motion
/// there competes with that.
const TILE_BORDER_SURGE: VoidPanelBorderSurge = VoidPanelBorderSurge {
    rate: 1.0 / 5.0,
    span: 21.0,
    width: 3.0,
    intensity: 2.5,
};

/// Research name on the tile face.
const NAME_FONT_SIZE: f32 = 15.0;

/// Icon and name-text brightness once a research is completed.
const COMPLETED_CONTENT_BRIGHTNESS: f32 = 0.55;

/// Shown in the action row once the button is gone, so a completed tile does not end in
/// an empty strip. Wears the same teal as the panel's corner mark: the mark is what you
/// catch scanning the grid, this is what you read on one tile.
const COMPLETED_LABEL: &str = "COMPLETE";
const COMPLETED_LABEL_FONT_SIZE: f32 = 11.0;

// ============================================================================
// RELATIONSHIP — tile lifetime follows research
// ============================================================================

/// Source side (on the tile): "this tile belongs to research N."
/// Linked despawn — when the research despawns, its tile despawns too.
#[derive(Component)]
#[relationship(relationship_target = ResearchTileLink)]
pub(crate) struct ResearchTileOf(pub Entity);

/// Target side (on the research): the tile that represents it.
/// `linked_spawn` despawns the tile when the research goes.
#[derive(Component)]
#[relationship_target(relationship = ResearchTileOf, linked_spawn)]
pub(crate) struct ResearchTileLink(Entity);

// ============================================================================
// TILE COMPONENTS
// ============================================================================

/// Marker on a tile entity. Stores the research entity and child widget entities so updates
/// can target them without traversing the UI subtree.
#[derive(Component)]
pub(crate) struct ResearchTile {
    pub(crate) research: Entity,
    icon: Entity,
    name_text: Entity,
    progress_bar: Entity,
    grants: Entity,
    /// Sits in the action row, shown only once the research is completed.
    completed_label: Entity,
}

// ============================================================================
// TILE BUILD — On<Add, ResearchTileOf> builds UI + registers data observer
// ============================================================================

fn on_add_research_tile_of_build_tile(
    trigger: On<Add, ResearchTileOf>,
    mut commands: Commands,
    tiles: Query<&ResearchTileOf>,
    grid: Single<Entity, With<ResearchTileGrid>>,
    runtimes: Query<&ResearchRuntime>,
    outcomes: Query<&HasOutcomes>,
) {
    let tile_entity = trigger.entity;
    let Ok(tile_of) = tiles.get(tile_entity) else { return };
    let research = tile_of.0;

    let progress = runtimes.get(research).map(|r| r.progress).unwrap_or(0.);

    // Build tile structure — content is populated by ResearchDataUpdated.
    let icon_node = commands.spawn((
        ImageNode::default(),
        Node {
            width: Val::Px(32.),
            height: Val::Px(32.),
            ..default()
        },
    )).id();

    let name_text = commands.spawn((
        Text::default(),
        Node {
            width: Val::Percent(100.),
            height: Val::Px(36.),
            overflow: Overflow::clip_y(),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            ..default()
        },
        TextRole::Body.font(NAME_FONT_SIZE),
        TextLayout {
            justify: Justify::Center,
            ..default()
        },
    )).id();

    let progress_bar = commands.spawn((
        Node {
            width: Val::Percent(100.),
            height: Val::Px(8.),
            ..default()
        },
    )).with_child(
        BuilderResearchBar::new(research).with_fraction(progress),
    ).id();

    let grants = commands.spawn((
        BuilderChipStrip,
        Node {
            width: Val::Percent(100.),
            height: Val::Px(20.),
            justify_content: JustifyContent::Center,
            ..default()
        },
    )).with_children(|strip| {
        if let Ok(has_outcomes) = outcomes.get(research) {
            for outcome in has_outcomes.iter() {
                strip.spawn(BuilderDisplayChip(outcome));
            }
        }
    }).id();

    // Collapsed rather than hidden while the research is unfinished: a hidden node still
    // takes space in the row and would push the action button off centre.
    let completed_label = commands.spawn((
        Text::new(COMPLETED_LABEL),
        TextRole::Data.font(COMPLETED_LABEL_FONT_SIZE),
        TextColor::from(COMPLETED_STYLE.color),
        TextLayout::no_wrap(),
        Node {
            display: Display::None,
            ..default()
        },
    )).id();

    let action_row = commands.spawn(Node {
        width: Val::Percent(100.),
        height: Val::Px(20.),
        justify_content: JustifyContent::Center,
        ..default()
    }).with_child(
        ResearchActionButton::new(research),
    ).add_child(completed_label).id();

    commands.entity(tile_entity)
        .insert((
            Node {
                width: Val::Px(168.),
                height: Val::Auto,
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(4.),
                padding: UiRect::all(Val::Px(8.)),
                ..default()
            },
            BuilderVoidPanel::default().with_border_surge(TILE_BORDER_SURGE),
            ResearchTile {
                research,
                icon: icon_node,
                name_text,
                progress_bar,
                grants,
                completed_label,
            },
        ))
        .observe(on_click_research_tile_select)
        .add_children(&[icon_node, name_text, progress_bar, grants, action_row]);

    // Register the data-updated observer on the research entity.
    commands.entity(research).observe(on_research_display_data_updated);

    commands.entity(*grid).add_child(tile_entity);
}

// ============================================================================
// DATA POPULATION — ResearchDataUpdated pushes display data to tile children
// ============================================================================

/// Fired when the grid's tiles may need reordering.
#[derive(Event, Clone, Copy)]
pub(crate) struct ResearchTilesNeedOrdering;

fn on_research_display_data_updated(
    trigger: On<ResearchDisplayDataUpdated>,
    researches: Query<(&DisplayName, &DisplayIcon, &ResearchTileLink)>,
    tiles: Query<&ResearchTile>,
    mut image_nodes: Query<&mut ImageNode>,
    mut texts: Query<&mut Text>,
    mut commands: Commands,
) {
    let research = trigger.research;
    let Ok((name, icon, link)) = researches.get(research) else { return };
    let Ok(tile) = tiles.get(link.0) else { return };

    if let Ok(mut text) = texts.get_mut(tile.name_text) {
        text.0 = name.0.clone();
    }
    if let Ok(mut image_node) = image_nodes.get_mut(tile.icon) {
        image_node.image = icon.0.clone();
    }

    commands.trigger(ResearchTilesNeedOrdering);
}

// ============================================================================
// TILE REFRESH — ResearchState drives the tile panel's style state
// ============================================================================

fn on_insert_research_state_refresh_tile(
    trigger: On<Insert, ResearchState>,
    mut commands: Commands,
    researches: Query<(&ResearchState, &ResearchTileLink)>,
    mut tiles: Query<(&ResearchTile, &mut VoidPanel)>,
    mut visibilities: Query<&mut Visibility>,
    mut nodes: Query<&mut Node>,
    mut icons: Query<&mut ImageNode>,
) {
    let Ok((research_state, tile_link)) = researches.get(trigger.entity) else { return };
    let Ok((tile, mut panel)) = tiles.get_mut(tile_link.0) else { return };

    if research_state.is_completed() {
        panel.set_style(COMPLETED_STYLE);
    } else {
        panel.clear_style();
    }

    // Hide the progress bar but keep it in the layout.
    if let Ok(mut visibility) = visibilities.get_mut(tile.progress_bar) {
        *visibility = if research_state.is_completed() { Visibility::Hidden } else { Visibility::Inherited };
    }

    // Takes the action button's place in the row. Collapsed rather than hidden so it does
    // not sit beside the button and shift it off centre while a research is unfinished.
    if let Ok(mut label_node) = nodes.get_mut(tile.completed_label) {
        label_node.display = if research_state.is_completed() { Display::Flex } else { Display::None };
    }

    let content_color = if research_state.is_completed() {
        Color::srgb(
            COMPLETED_CONTENT_BRIGHTNESS,
            COMPLETED_CONTENT_BRIGHTNESS,
            COMPLETED_CONTENT_BRIGHTNESS,
        )
    } else {
        Color::WHITE
    };
    if let Ok(mut icon) = icons.get_mut(tile.icon) {
        icon.color = content_color;
    }
    commands.entity(tile.name_text).insert(TextColor::from(content_color));

    // The outcome chips carry their own white icon and label, so they stay bright unless
    // the strip is told otherwise, and a completed tile ends up muted everywhere except
    // its grants.
    let mut grants = commands.entity(tile.grants);
    if research_state.is_completed() {
        grants.insert(ChipsFaded(COMPLETED_CONTENT_BRIGHTNESS));
    } else {
        grants.remove::<ChipsFaded>();
    }
}

// ============================================================================
// SELECTION — the ResearchUISelected marker is the single source of truth.
//
// Clicking a tile only moves the marker. The highlight follows from observing it, so
// selection made anywhere else — the detail view, a restored save — lights the right tile
// with no further wiring.
// ============================================================================

fn on_click_research_tile_select(
    trigger: On<Pointer<Click>>,
    mut commands: Commands,
    tiles: Query<&ResearchTile>,
    selected_research: Option<Single<Entity, With<ResearchUISelected>>>,
) {
    let Ok(clicked_tile) = tiles.get(trigger.entity) else { return };
    let clicked_research = clicked_tile.research;

    // Deselect whoever currently holds the marker.
    if let Some(selected_research) = selected_research {
        let selected_research = selected_research.into_inner();
        commands.entity(selected_research).remove::<ResearchUISelected>();
        // Clicking the already-selected research is a toggle — deselect and stop.
        if selected_research == clicked_research { return }
    }

    commands.entity(clicked_research).insert(ResearchUISelected);
}

fn on_add_research_ui_selected_light_tile(
    trigger: On<Add, ResearchUISelected>,
    links: Query<&ResearchTileLink>,
    mut panels: Query<&mut VoidPanel, With<ResearchTile>>,
) {
    let Ok(link) = links.get(trigger.entity) else { return };
    let Ok(mut panel) = panels.get_mut(link.0) else { return };
    panel.set_selected(true);
}

fn on_remove_research_ui_selected_dim_tile(
    trigger: On<Remove, ResearchUISelected>,
    links: Query<&ResearchTileLink>,
    mut panels: Query<&mut VoidPanel, With<ResearchTile>>,
) {
    // Also fires when the research despawns, which takes its tile with it. Both lookups
    // then miss and this does nothing.
    let Ok(link) = links.get(trigger.entity) else { return };
    let Ok(mut panel) = panels.get_mut(link.0) else { return };
    panel.set_selected(false);
}

// ============================================================================
// ACTIVITY — the running research's tile carries the same surges as its card
// ============================================================================

fn on_add_research_active_surge_tile(
    trigger: On<Add, ResearchActive>,
    links: Query<&ResearchTileLink>,
    mut panels: Query<&mut VoidPanel, With<ResearchTile>>,
) {
    let Ok(link) = links.get(trigger.entity) else { return };
    let Ok(mut panel) = panels.get_mut(link.0) else { return };
    panel.set_border_surge(true);
}

fn on_remove_research_active_still_tile(
    trigger: On<Remove, ResearchActive>,
    links: Query<&ResearchTileLink>,
    mut panels: Query<&mut VoidPanel, With<ResearchTile>>,
) {
    let Ok(link) = links.get(trigger.entity) else { return };
    let Ok(mut panel) = panels.get_mut(link.0) else { return };
    panel.set_border_surge(false);
}
