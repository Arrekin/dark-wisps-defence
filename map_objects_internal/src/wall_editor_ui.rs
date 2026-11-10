//! Wall placement controls for selecting a style, toggling tunnel openings, and cycling ramp direction.
//!
//! Controls replace `WallStyleKey` or `WallShape` on the placer's session entity and request a new
//! ghost preview. The same request refreshes the controls to reflect the selection.

use bevy::{input::common_conditions::input_just_pressed, prelude::*, ui_widgets::Button};
use strum::IntoEnumIterator;

use game_core::prelude::{CELL_SIZE, EdgeSide, MapObject, ObjectFaceRequest};
use grids::placement::{BeginPlacing, GridPlacerGhostRebuildRequest, StopPlacing};
use indexes::placement::GridPlacerSession;
use map_objects::{prelude::WallShape, wall_style::{WallStyleKey, WallStyles}};

pub(crate) struct WallEditorUiPlugin;
impl Plugin for WallEditorUiPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Update, GridPlacerUiForWall::cycle_shape.run_if(input_just_pressed(SHAPE_KEY)).run_if(any_with_component::<GridPlacerUiForWall>))
            .add_observer(GridPlacerUiForWall::on_add_construct_grid_placer_ui)
            .add_observer(GridPlacerUiForWall::on_begin_placing_start_wall_session)
            .add_observer(GridPlacerUiForWall::on_stop_placing_despawn_grid_placer_ui)
            .add_observer(GridPlacerUiForWall::on_ghost_rebuild_request_refresh_controls);
    }
}

const SHAPE_KEY: KeyCode = KeyCode::KeyR;

// Swatch-frame colors for selected and idle styles.
const SWATCH_FRAME_SELECTED: Color = Color::srgb(0.16, 0.78, 1.00); // cyan
const SWATCH_FRAME_IDLE: Color = Color::srgb(0.15, 0.15, 0.18); // dark grey

// Tunnel opening buttons
const OPENING_EDGE_OPEN: Color = SWATCH_FRAME_SELECTED;
const OPENING_EDGE_CLOSED: Color = Color::srgb(0.30, 0.30, 0.34); // grey
const OPENING_EDGE_THICKNESS: f32 = 6.0;

/// Root of the wall placement controls, created and removed with the wall placement session.
#[derive(Component)]
pub(crate) struct GridPlacerUiForWall;
impl GridPlacerUiForWall {
    /// Cycles through ramps rising top, right, bottom, and left, then a solid flat wall.
    /// Any flat wall starts the cycle at the top ramp; tunnel openings are discarded.
    fn cycle_shape(
        mut commands: Commands,
        placer_session: Single<(Entity, &WallShape), With<GridPlacerSession>>,
    ) {
        let (placer_session, shape) = placer_session.into_inner();
        let next = match *shape {
            WallShape::Flat { .. } => WallShape::Ramp { high_side: EdgeSide::Top },
            WallShape::Ramp { high_side: EdgeSide::Top } => WallShape::Ramp { high_side: EdgeSide::Right },
            WallShape::Ramp { high_side: EdgeSide::Right } => WallShape::Ramp { high_side: EdgeSide::Bottom },
            WallShape::Ramp { high_side: EdgeSide::Bottom } => WallShape::Ramp { high_side: EdgeSide::Left },
            WallShape::Ramp { high_side: EdgeSide::Left } => WallShape::default(),
        };
        commands.entity(placer_session).insert(next);
        commands.trigger(GridPlacerGhostRebuildRequest);
    }

    /// Builds the controls with the style and openings selected in the placement session.
    fn on_add_construct_grid_placer_ui(
        trigger: On<Add<GridPlacerUiForWall>>,
        mut commands: Commands,
        styles: Res<WallStyles>,
        placer_session: Single<(&WallStyleKey, &WallShape), With<GridPlacerSession>>,
    ) {
        let (&selected, &shape) = placer_session.into_inner();
        commands.entity(trigger.entity)
            .insert(Node {
                position_type: PositionType::Absolute,
                top: Val::Px(5.0),
                left: Val::Percent(50.0),
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(6.0),
                ..default()
            })
            .with_children(|parent| {
                for (index, entry) in styles.entries.iter().enumerate() {
                    let key = WallStyleKey(index as u8);
                    parent.spawn((
                        WallStyleButton(key),
                        Node {
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Center,
                            row_gap: Val::Px(2.0),
                            padding: UiRect::all(Val::Px(2.0)),
                            border: UiRect::all(Val::Px(1.0)),
                            ..default()
                        },
                        BorderColor::from(WallStyleButton::frame_color(key, selected)),
                        BackgroundColor(Color::BLACK),
                    ))
                    .with_children(|button| {
                        // The face renderer reads the style key from the swatch node.
                        button
                            .spawn((
                                Node {
                                    width: Val::Px(CELL_SIZE),
                                    height: Val::Px(CELL_SIZE),
                                    ..default()
                                },
                                key,
                            ))
                            .trigger(|entity| ObjectFaceRequest::ui(entity, MapObject::Wall));
                        button.spawn((Text::new(entry.name.clone()), TextFont::from_font_size(10.0)));
                    })
                    .observe(WallStyleButton::on_click_select_style);
                }

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
                        BorderColor::from(SWATCH_FRAME_IDLE),
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
                                            BackgroundColor(OpeningEdgeButton::color(side, shape)),
                                        ))
                                        .observe(OpeningEdgeButton::on_click_toggle_opening);
                                }
                            });
                        control.spawn((Text::new("openings"), TextFont::from_font_size(10.0)));
                    });
            });
    }

    fn on_begin_placing_start_wall_session(
        trigger: On<BeginPlacing>,
        mut commands: Commands,
        placer_session: Single<Entity, With<GridPlacerSession>>,
    ) {
        let BeginPlacing(MapObject::Wall) = *trigger else { return };
        commands.entity(*placer_session).insert((WallStyleKey::default(), WallShape::default()));
        commands.spawn(GridPlacerUiForWall);
    }

    fn on_stop_placing_despawn_grid_placer_ui(
        _trigger: On<StopPlacing>,
        mut commands: Commands,
        existing_ui: Single<Entity, With<GridPlacerUiForWall>>,
    ) {
        commands.entity(existing_ui.into_inner()).despawn();
    }

    /// Updates swatch borders and opening button colors to match the session's selections.
    fn on_ghost_rebuild_request_refresh_controls(
        _trigger: On<GridPlacerGhostRebuildRequest>,
        placer_session: Single<(&WallStyleKey, &WallShape), With<GridPlacerSession>>,
        mut swatches: Query<(&WallStyleButton, &mut BorderColor)>,
        mut edges: Query<(&OpeningEdgeButton, &mut BackgroundColor)>,
    ) {
        let (&selected, &shape) = placer_session.into_inner();
        for (&WallStyleButton(key), mut frame) in swatches.iter_mut() {
            *frame = BorderColor::from(WallStyleButton::frame_color(key, selected));
        }
        for (&OpeningEdgeButton(side), mut color) in edges.iter_mut() {
            color.0 = OpeningEdgeButton::color(side, shape);
        }
    }
}

/// A style button containing a swatch and its name.
/// The selection border surrounds the button so it does not obscure the preview.
#[derive(Component, Clone, Copy)]
#[require(Button)]
pub(crate) struct WallStyleButton(pub WallStyleKey);
impl WallStyleButton {
    fn frame_color(key: WallStyleKey, selected: WallStyleKey) -> Color {
        if key == selected { SWATCH_FRAME_SELECTED } else { SWATCH_FRAME_IDLE }
    }

    fn on_click_select_style(
        trigger: On<PointerClick>,
        mut commands: Commands,
        placer_session: Single<Entity, With<GridPlacerSession>>,
        buttons: Query<&WallStyleButton>,
    ) {
        let Ok(&WallStyleButton(selected)) = buttons.get(trigger.entity) else { return; };
        commands.entity(*placer_session).insert(selected);
        commands.trigger(GridPlacerGhostRebuildRequest);
    }
}

/// Button for one side of the tunnel opening selector.
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

    fn color(side: EdgeSide, shape: WallShape) -> Color {
        if shape.is_open_toward(side) { OPENING_EDGE_OPEN } else { OPENING_EDGE_CLOSED }
    }

    /// Toggles this side's opening. Converts a ramp to a flat wall with only this opening.
    fn on_click_toggle_opening(
        trigger: On<PointerClick>,
        mut commands: Commands,
        placer_session: Single<(Entity, &WallShape), With<GridPlacerSession>>,
        edges: Query<&OpeningEdgeButton>,
    ) {
        let Ok(&OpeningEdgeButton(side)) = edges.get(trigger.entity) else { return; };
        let (placer_session, shape) = placer_session.into_inner();
        let openings = shape.openings().toggled(side);
        commands.entity(placer_session).insert(WallShape::Flat { openings });
        commands.trigger(GridPlacerGhostRebuildRequest);
    }
}
