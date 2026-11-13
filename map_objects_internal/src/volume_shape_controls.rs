//! # Volume shape controls
//!
//! Shared controls for wall and excavation placement: tunnel openings, ramp orientation,
//! and replacement of existing walls or excavations.
//! The shape controls replace a [`SelectableShape`] component on the session, then send
//! `GridPlacerGhostRebuildRequest` to update both the preview and the opening buttons. The replace
//! switch toggles `ReplaceExisting` and updates only its own border.
//!
//! Each placement tool registers [`cycle_shape`] and
//! [`on_ghost_rebuild_request_refresh_opening_selector`] for its shape type.

use bevy::{prelude::*, ui_widgets::Button};
use strum::IntoEnumIterator;

use game_core::prelude::{CELL_SIZE, EdgeSide, EdgeSides};
use grids::placement::{GridPlacerGhostRebuildRequest, PlacementSession, ReplaceExisting};
use map_objects::prelude::VolumeShape;

pub(crate) struct VolumeShapeControlsPlugin;
impl Plugin for VolumeShapeControlsPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(ReplaceToggleButton::on_add_construct_replace_toggle);
    }
}

pub(crate) const SHAPE_KEY: KeyCode = KeyCode::KeyR;

// Control frame colors
pub(crate) const CONTROL_FRAME_SELECTED: Color = Color::srgb(0.16, 0.78, 1.00); // cyan
pub(crate) const CONTROL_FRAME_IDLE: Color = Color::srgb(0.15, 0.15, 0.18); // dark grey

// Tunnel opening buttons
const OPENING_EDGE_OPEN: Color = CONTROL_FRAME_SELECTED;
const OPENING_EDGE_CLOSED: Color = Color::srgb(0.30, 0.30, 0.34); // grey
const OPENING_EDGE_THICKNESS: f32 = 6.0;

/// Adapts a placement session's shape component to the shared controls.
pub(crate) trait SelectableShape: Component + Copy {
    /// Selection after the last ramp direction or after closing the last tunnel opening.
    const BASE: Self;

    /// Returns the selected volume shape, or `None` for an open pit.
    fn volume_shape(self) -> Option<VolumeShape>;

    fn from_volume_shape(shape: VolumeShape) -> Self;

    fn openings(self) -> EdgeSides {
        self.volume_shape().map_or(EdgeSides::NONE, VolumeShape::openings)
    }

    /// Cycles uphill direction through top, right, bottom and left, then returns to [`Self::BASE`].
    /// Other shapes start at the top ramp, discarding any tunnel openings.
    fn next_in_cycle(self) -> Self {
        let next_high_side = match self.volume_shape() {
            Some(VolumeShape::Ramp { high_side: EdgeSide::Top }) => EdgeSide::Right,
            Some(VolumeShape::Ramp { high_side: EdgeSide::Right }) => EdgeSide::Bottom,
            Some(VolumeShape::Ramp { high_side: EdgeSide::Bottom }) => EdgeSide::Left,
            Some(VolumeShape::Ramp { high_side: EdgeSide::Left }) => return Self::BASE,
            Some(VolumeShape::Flat { .. }) | None => EdgeSide::Top,
        };
        Self::from_volume_shape(VolumeShape::Ramp { high_side: next_high_side })
    }

    /// Toggles the tunnel opening on `side`. Opening a ramp or pit creates a flat tunnel.
    /// Closing the last opening returns to [`Self::BASE`].
    fn with_opening_toggled(self, side: EdgeSide) -> Self {
        let openings = self.openings().toggled(side);
        if openings.is_empty() { Self::BASE } else { Self::from_volume_shape(VolumeShape::Flat { openings }) }
    }
}

/// Layout for a horizontal control bar at the top of the screen.
pub(crate) fn controls_bar_node() -> Node {
    Node {
        position_type: PositionType::Absolute,
        top: Val::Px(5.0),
        left: Val::Percent(50.0),
        flex_direction: FlexDirection::Row,
        column_gap: Val::Px(6.0),
        ..default()
    }
}

/// Spawns the opening selector: a cell-sized square with one button per side, labelled "openings".
pub(crate) fn spawn_opening_selector<S: SelectableShape>(parent: &mut ChildSpawnerCommands, shape: S) {
    parent
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(2.0),
                padding: UiRect::all(Val::Px(2.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BorderColor::from(CONTROL_FRAME_IDLE),
            BackgroundColor(Color::BLACK),
        ))
        .with_children(|control| {
            control
                .spawn(Node {
                    width: Val::Px(CELL_SIZE),
                    height: Val::Px(CELL_SIZE),
                    ..default()
                })
                .with_children(|square| {
                    for side in EdgeSide::iter() {
                        square
                            .spawn((
                                OpeningEdgeButton(side),
                                OpeningEdgeButton::node(side),
                                BackgroundColor(OpeningEdgeButton::color(side, shape.openings())),
                            ))
                            .observe(OpeningEdgeButton::on_click_toggle_opening::<S>);
                    }
                });
            control.spawn((Text::new("openings"), TextFont::from_font_size(10.0)));
        });
}

/// Replaces the session's shape with the next one in the ramp cycle.
pub(crate) fn cycle_shape<S: SelectableShape>(
    mut commands: Commands,
    placement_session: Single<(Entity, &S), With<PlacementSession>>,
) {
    let (placement_session, &shape) = placement_session.into_inner();
    commands.entity(placement_session).insert(shape.next_in_cycle());
    commands.trigger(GridPlacerGhostRebuildRequest);
}

/// Updates the opening buttons to match the session's shape.
pub(crate) fn on_ghost_rebuild_request_refresh_opening_selector<S: SelectableShape>(
    _trigger: On<GridPlacerGhostRebuildRequest>,
    placement_session: Single<&S, With<PlacementSession>>,
    mut edges: Query<(&OpeningEdgeButton, &mut BackgroundColor)>,
) {
    let openings = placement_session.openings();
    for (&OpeningEdgeButton(side), mut color) in edges.iter_mut() {
        color.0 = OpeningEdgeButton::color(side, openings);
    }
}

/// Button for one side of the opening selector.
#[derive(Component, Clone, Copy)]
#[require(Button)]
pub(crate) struct OpeningEdgeButton(pub EdgeSide);
impl OpeningEdgeButton {
    /// Places an edge bar with gaps at the corners so adjacent buttons do not overlap.
    fn node(side: EdgeSide) -> Node {
        let thickness = Val::Px(OPENING_EDGE_THICKNESS);
        let mut node = Node { position_type: PositionType::Absolute, ..default() };
        match side {
            EdgeSide::Top | EdgeSide::Bottom => {
                node.left = thickness;
                node.right = thickness;
                node.height = thickness;
            }
            EdgeSide::Right | EdgeSide::Left => {
                node.top = thickness;
                node.bottom = thickness;
                node.width = thickness;
            }
        }
        match side {
            EdgeSide::Top => node.top = Val::Px(0.0),
            EdgeSide::Right => node.right = Val::Px(0.0),
            EdgeSide::Bottom => node.bottom = Val::Px(0.0),
            EdgeSide::Left => node.left = Val::Px(0.0),
        }
        node
    }

    fn color(side: EdgeSide, openings: EdgeSides) -> Color {
        if openings.contains(side) { OPENING_EDGE_OPEN } else { OPENING_EDGE_CLOSED }
    }

    fn on_click_toggle_opening<S: SelectableShape>(
        trigger: On<PointerClick>,
        mut commands: Commands,
        placement_session: Single<(Entity, &S), With<PlacementSession>>,
        edges: Query<&OpeningEdgeButton>,
    ) {
        let Ok(&OpeningEdgeButton(side)) = edges.get(trigger.entity) else { return; };
        let (placement_session, &shape) = placement_session.into_inner();
        commands.entity(placement_session).insert(shape.with_opening_toggled(side));
        commands.trigger(GridPlacerGhostRebuildRequest);
    }
}

/// Toggles `ReplaceExisting` on the placement session and updates its border to show the selection.
///
/// The border starts unselected; create this button for a session without `ReplaceExisting`.
#[derive(Component)]
#[require(Button)]
pub(crate) struct ReplaceToggleButton;
impl ReplaceToggleButton {
    fn on_add_construct_replace_toggle(
        trigger: On<Add<ReplaceToggleButton>>,
        mut commands: Commands,
    ) {
        commands
            .entity(trigger.entity)
            .insert((
                Node {
                    align_items: AlignItems::Center,
                    padding: UiRect::all(Val::Px(4.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BorderColor::from(CONTROL_FRAME_IDLE),
                BackgroundColor(Color::BLACK),
                children![(Text::new("replace"), TextFont::from_font_size(10.0))],
            ))
            .observe(Self::on_click_toggle_replace_existing);
    }

    fn on_click_toggle_replace_existing(
        trigger: On<PointerClick>,
        mut commands: Commands,
        placement_session: Single<(Entity, Has<ReplaceExisting>), With<PlacementSession>>,
        mut frames: Query<&mut BorderColor, With<ReplaceToggleButton>>,
    ) {
        let (placement_session, was_replacing) = placement_session.into_inner();
        if was_replacing {
            commands.entity(placement_session).remove::<ReplaceExisting>();
        } else {
            commands.entity(placement_session).insert(ReplaceExisting);
        }
        let Ok(mut frame) = frames.get_mut(trigger.entity) else { return; };
        *frame = BorderColor::from(if was_replacing { CONTROL_FRAME_IDLE } else { CONTROL_FRAME_SELECTED });
    }
}
