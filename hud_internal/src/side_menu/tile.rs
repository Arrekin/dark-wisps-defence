//! Side-menu tile frames, placement interaction, and registered object presentation.

use bevy::prelude::*;
use bevy::ui::FocusPolicy;

use almanach::prelude::*;
use game_core::prelude::*;
use grids::placement::GridObjectPlacerRequest;
use widgets::prelude::{BuilderVoidPanel, VoidPanelBorderSurge};

// Tile frame
const FACE_SIZE: f32 = 46.;
/// Keeps the face clear of the panel contour and its animated surge.
const TILE_FACE_INSET: f32 = 5.;
const TILE_SIZE: f32 = FACE_SIZE + 2. * TILE_FACE_INSET;

/// Border-surge tuning for a tile-sized perimeter; `span` and `width` are pixels.
const TILE_BORDER_SURGE: VoidPanelBorderSurge = VoidPanelBorderSurge {
    rate: 1. / 4.,
    span: 14.,
    width: 2.,
    intensity: 4.,
};

// ============================================================================
// TILE CORE
// ============================================================================


#[derive(Component, Clone, Copy, Default)]
#[require(Button, FocusPolicy)]
pub(crate) struct Tile;

#[derive(Component, Clone, Copy)]
pub(crate) struct TileChildren {
    pub face: Entity,
}

pub(crate) fn on_add_tile_build(
    trigger: On<Add, Tile>,
    mut commands: Commands,
) {
    let tile_entity = trigger.entity;

    let face = commands.spawn(Node {
        width: Val::Px(FACE_SIZE),
        height: Val::Px(FACE_SIZE),
        ..default()
    }).id();

    commands.entity(tile_entity)
        .insert((
            Node {
                width: Val::Px(TILE_SIZE),
                height: Val::Px(TILE_SIZE),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BuilderVoidPanel::default()
                .with_corner_cut(0.)
                .with_hairline_strength(0.)
                .with_border_surge(TILE_BORDER_SURGE),
            TileChildren { face },
        ))
        .add_child(face);
}

// ============================================================================
// PLACEMENT SPECIALIZATION — the tile that starts a placement session
// ============================================================================

/// A tile that starts a placement session for its object.
#[derive(Component, Clone, Copy)]
#[require(Tile)]
pub(crate) struct PlacementTile(pub MapObject);

pub(crate) fn on_add_placement_tile_watch_click(
    trigger: On<Add, PlacementTile>,
    mut commands: Commands,
) {
    commands.entity(trigger.entity).observe(on_click_placement_tile_request_placement);
}

/// Applies the object's registered face and optional tooltip to a placement tile.
pub(crate) fn on_add_tile_children_apply_presentation(
    trigger: On<Add, TileChildren>,
    mut commands: Commands,
    almanach: Res<Almanach>,
    tiles: Query<(&PlacementTile, &TileChildren)>,
) {
    let tile_entity = trigger.entity;
    let Ok((placement_tile, children)) = tiles.get(tile_entity) else { return };
    let map_object = placement_tile.0;

    let presentation = almanach.presentation_for(map_object);
    presentation.face.apply(&mut commands.entity(children.face), map_object);
    if let Some(build_tooltip) = presentation.tooltip {
        build_tooltip(&mut commands, tile_entity, map_object);
    }
}

/// Stops propagation because a click on the parent section cancels its active placement.
fn on_click_placement_tile_request_placement(
    mut trigger: On<Pointer<Click>>,
    mut grid_object_placer_request: ResMut<GridObjectPlacerRequest>,
    tiles: Query<&PlacementTile>,
) {
    trigger.propagate(false);
    let Ok(placement_tile) = tiles.get(trigger.entity) else { return };

    grid_object_placer_request.set(placement_tile.0);
}
