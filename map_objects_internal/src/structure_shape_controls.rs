//! # Structure shape controls
//!
//! Shared placement UI controls for walls and excavations: hollowed-side toggles, ramp direction
//! cycling, and the [`ReplaceExisting`] toggle.
//!
//! Shape controls update a [`SelectableShape`] component on [`UiPlacementSession`] and trigger
//! [`GridPlacerGhostRebuildRequest`] to refresh both the ghost preview and button highlights.
//! [`ReplaceToggleButton`] toggles [`ReplaceExisting`] and updates its own border directly.
//!
//! Each placement tool registers [`cycle_shape`] and
//! [`on_ghost_rebuild_request_refresh_hollowed_side_selector`] for its shape type.

use bevy::{prelude::*, ui_widgets::Button};
use strum::IntoEnumIterator;

use game_core::prelude::{CELL_SIZE, EdgeSide, EdgeSides};
use grids::placement::{GridPlacerGhostRebuildRequest, ReplaceExisting, UiPlacementSession};
use map_objects::prelude::StructureShape;

pub(crate) struct StructureShapeControlsPlugin;
impl Plugin for StructureShapeControlsPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(ReplaceToggleButton::on_add_construct_replace_toggle);
    }
}

pub(crate) const SHAPE_KEY: KeyCode = KeyCode::KeyR;

// Control frame colors
pub(crate) const CONTROL_FRAME_SELECTED: Color = Color::srgb(0.16, 0.78, 1.00); // cyan
pub(crate) const CONTROL_FRAME_IDLE: Color = Color::srgb(0.15, 0.15, 0.18); // dark grey

// Hollowed side buttons
const HOLLOWED_SIDE_SELECTED: Color = CONTROL_FRAME_SELECTED;
const HOLLOWED_SIDE_IDLE: Color = Color::srgb(0.30, 0.30, 0.34); // grey
const HOLLOWED_SIDE_THICKNESS: f32 = 6.0;

/// Adapts a placement session's shape component to the shared controls.
pub(crate) trait SelectableShape: Component + Copy {
    /// Selection after the last ramp direction or after removing the last hollowed side.
    const BASE: Self;

    /// Returns the selected structure shape, or `None` for an open pit.
    fn structure_shape(self) -> Option<StructureShape>;

    fn from_structure_shape(shape: StructureShape) -> Self;

    fn hollowed_sides(self) -> EdgeSides {
        self.structure_shape().map_or(EdgeSides::NONE, StructureShape::hollowed_sides)
    }

    /// Cycles uphill direction through top, right, bottom and left, then returns to [`Self::BASE`].
    /// Other shapes start at the top ramp, discarding any hollowed sides.
    fn next_in_cycle(self) -> Self {
        let next_high_side = match self.structure_shape() {
            Some(StructureShape::Ramp { high_side: EdgeSide::Top }) => EdgeSide::Right,
            Some(StructureShape::Ramp { high_side: EdgeSide::Right }) => EdgeSide::Bottom,
            Some(StructureShape::Ramp { high_side: EdgeSide::Bottom }) => EdgeSide::Left,
            Some(StructureShape::Ramp { high_side: EdgeSide::Left }) => return Self::BASE,
            Some(StructureShape::Flat { .. }) | None => EdgeSide::Top,
        };
        Self::from_structure_shape(StructureShape::Ramp { high_side: next_high_side })
    }

    /// Toggles the hollowed side `side`. Hollowing a side of a ramp or pit creates a flat shape.
    /// Removing the last hollowed side returns to [`Self::BASE`].
    fn with_hollowed_side_toggled(self, side: EdgeSide) -> Self {
        let hollowed_sides = self.hollowed_sides().toggled(side);
        if hollowed_sides.is_empty() { Self::BASE } else { Self::from_structure_shape(StructureShape::Flat { hollowed_sides }) }
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

/// Spawns the hollowed side selector: a cell-sized square with one button per side, labelled "hollowed sides".
pub(crate) fn spawn_hollowed_side_selector<S: SelectableShape>(parent: &mut ChildSpawnerCommands, shape: S) {
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
                                HollowedSideButton(side),
                                HollowedSideButton::node(side),
                                BackgroundColor(HollowedSideButton::color(side, shape.hollowed_sides())),
                            ))
                            .observe(HollowedSideButton::on_click_toggle_hollowed_side::<S>);
                    }
                });
            control.spawn((Text::new("hollowed sides"), TextFont::from_font_size(10.0)));
        });
}

/// Replaces the UI placement session's shape with the next one in the ramp cycle.
pub(crate) fn cycle_shape<S: SelectableShape>(
    mut commands: Commands,
    ui_placement_session: Single<(Entity, &S), With<UiPlacementSession>>,
) {
    let (ui_placement_session_entity, &shape) = ui_placement_session.into_inner();
    commands.entity(ui_placement_session_entity).insert(shape.next_in_cycle());
    commands.trigger(GridPlacerGhostRebuildRequest);
}

/// Updates the hollowed side buttons to match the UI placement session's shape.
pub(crate) fn on_ghost_rebuild_request_refresh_hollowed_side_selector<S: SelectableShape>(
    _trigger: On<GridPlacerGhostRebuildRequest>,
    ui_placement_session: Single<&S, With<UiPlacementSession>>,
    mut buttons: Query<(&HollowedSideButton, &mut BackgroundColor)>,
) {
    let hollowed_sides = ui_placement_session.hollowed_sides();
    for (&HollowedSideButton(side), mut color) in buttons.iter_mut() {
        color.0 = HollowedSideButton::color(side, hollowed_sides);
    }
}

/// Button for one side of the hollowed side selector.
#[derive(Component, Clone, Copy)]
#[require(Button)]
pub(crate) struct HollowedSideButton(EdgeSide);
impl HollowedSideButton {
    /// Places an edge bar with gaps at the corners so adjacent buttons do not overlap.
    fn node(side: EdgeSide) -> Node {
        let thickness = Val::Px(HOLLOWED_SIDE_THICKNESS);
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

    fn color(side: EdgeSide, hollowed_sides: EdgeSides) -> Color {
        if hollowed_sides.contains(side) { HOLLOWED_SIDE_SELECTED } else { HOLLOWED_SIDE_IDLE }
    }

    fn on_click_toggle_hollowed_side<S: SelectableShape>(
        trigger: On<PointerClick>,
        mut commands: Commands,
        ui_placement_session: Single<(Entity, &S), With<UiPlacementSession>>,
        buttons: Query<&HollowedSideButton>,
    ) {
        let Ok(&HollowedSideButton(side)) = buttons.get(trigger.entity) else { return; };
        let (ui_placement_session_entity, &shape) = ui_placement_session.into_inner();
        commands.entity(ui_placement_session_entity).insert(shape.with_hollowed_side_toggled(side));
        commands.trigger(GridPlacerGhostRebuildRequest);
    }
}

/// Button that toggles [`ReplaceExisting`] on [`UiPlacementSession`] and highlights its border
/// while active. Starts unselected.
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
        ui_placement_session: Single<(Entity, Has<ReplaceExisting>), With<UiPlacementSession>>,
        mut frames: Query<&mut BorderColor, With<ReplaceToggleButton>>,
    ) {
        let (ui_placement_session_entity, was_replacing) = ui_placement_session.into_inner();
        if was_replacing {
            commands.entity(ui_placement_session_entity).remove::<ReplaceExisting>();
        } else {
            commands.entity(ui_placement_session_entity).insert(ReplaceExisting);
        }
        let Ok(mut frame) = frames.get_mut(trigger.entity) else { return; };
        *frame = BorderColor::from(if was_replacing { CONTROL_FRAME_IDLE } else { CONTROL_FRAME_SELECTED });
    }
}
