use bevy::prelude::*;

use game_core::prelude::{DisplayIcon, DisplayName};
use outcomes::prelude::HasOutcomes;
use research::prelude::*;
use research::research_bar::BuilderResearchBar;
use widgets::prelude::{BuilderChipStrip, BuilderDisplayChip};

use super::{
    action_button::ResearchActionButton,
    panel::ResearchTileGrid,
};

pub(crate) struct ResearchTilePlugin;
impl Plugin for ResearchTilePlugin {
    fn build(&self, app: &mut App) {
        app
            .add_observer(on_add_research_tile_of_build_tile)
            .add_observer(on_insert_research_state_refresh_tile);
    }
}

#[derive(EntityEvent, Clone, Copy)]
pub(crate) struct ResearchTileSelected {
    #[event_target]
    pub(crate) tile: Entity,
}

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

/// Marker on a tile entity. Stores the research entity and the child widget
/// entities for direct lookup.
#[derive(Component)]
#[allow(dead_code)]
pub(crate) struct ResearchTile {
    pub(crate) research: Entity,
    icon: Entity,
    name_text: Entity,
    progress_bar: Entity,
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
        TextFont::from_font_size(15.),
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
        BuilderResearchBar::new(research).with_fill_fraction(progress),
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

    let action_row = commands.spawn(Node {
        width: Val::Percent(100.),
        height: Val::Px(20.),
        justify_content: JustifyContent::Center,
        ..default()
    }).with_child((
        ResearchActionButton::new(research),
        Node {
            padding: UiRect::horizontal(Val::Px(6.)),
            ..default()
        },
    )).id();

    commands.entity(tile_entity)
        .insert((
            Node {
                width: Val::Px(168.),
                height: Val::Auto,
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(4.),
                padding: UiRect::all(Val::Px(8.)),
                border: UiRect::all(Val::Px(1.)),
                border_radius: BorderRadius::all(Val::Px(4.)),
                ..default()
            },
            BackgroundColor::from(Color::linear_rgba(0.15, 0.15, 0.2, 1.)),
            BorderColor::all(Color::linear_rgba(0.3, 0.3, 0.35, 1.)),
            ResearchTile {
                research,
                icon: icon_node,
                name_text,
                progress_bar,
            },
        ))
        .observe(move |_: On<Pointer<Click>>, mut commands: Commands| {
            commands.trigger(ResearchTileSelected { tile: tile_entity });
        })
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
// TILE REFRESH — On<Insert, ResearchState>
// ============================================================================

fn on_insert_research_state_refresh_tile(
    trigger: On<Insert, ResearchState>,
    researches: Query<(&ResearchState, &ResearchTileLink)>,
    mut tile_bgs: Query<(&mut BackgroundColor, &mut BorderColor), With<ResearchTile>>,
) {
    let Ok((state, link)) = researches.get(trigger.entity) else { return };
    let Ok((mut bg, mut border)) = tile_bgs.get_mut(link.0) else { return };

    match state {
        ResearchState::Completed => {
            bg.0 = Color::linear_rgba(0.1, 0.3, 0.15, 1.);
            *border = BorderColor::all(Color::linear_rgba(0.2, 0.6, 0.3, 1.));
        }
        _ => {
            bg.0 = Color::linear_rgba(0.15, 0.15, 0.2, 1.);
            *border = BorderColor::all(Color::linear_rgba(0.3, 0.3, 0.35, 1.));
        }
    }
}
