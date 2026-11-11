//! Wall placement controls for selecting a style, toggling tunnel openings, and cycling ramp direction.
//!
//! Controls replace `WallStyleKey` or `VolumeShape` on the placer's session entity and request a new
//! ghost preview. The same request refreshes the controls to reflect the selection.

use bevy::{input::common_conditions::input_just_pressed, prelude::*, ui_widgets::Button};

use game_core::prelude::{CELL_SIZE, EdgeSides, MapObject, ObjectFaceRequest};
use grids::placement::{BeginPlacing, GridPlacerGhostRebuildRequest, StopPlacing};
use indexes::placement::GridPlacerSession;
use map_objects::{prelude::VolumeShape, wall_style::{WallStyleKey, WallStyles}};

use crate::volume_shape_controls::{
    CONTROL_FRAME_IDLE, CONTROL_FRAME_SELECTED, SHAPE_KEY, SelectableShape,
    controls_bar_node, cycle_shape, on_ghost_rebuild_request_refresh_opening_selector, spawn_opening_selector,
};

pub(crate) struct WallEditorUiPlugin;
impl Plugin for WallEditorUiPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Update, cycle_shape::<VolumeShape>.run_if(input_just_pressed(SHAPE_KEY)).run_if(any_with_component::<GridPlacerUiForWall>))
            .add_observer(GridPlacerUiForWall::on_add_construct_grid_placer_ui)
            .add_observer(GridPlacerUiForWall::on_begin_placing_start_wall_session)
            .add_observer(GridPlacerUiForWall::on_stop_placing_despawn_grid_placer_ui)
            .add_observer(GridPlacerUiForWall::on_ghost_rebuild_request_refresh_style_swatches)
            .add_observer(on_ghost_rebuild_request_refresh_opening_selector::<VolumeShape>);
    }
}

impl SelectableShape for VolumeShape {
    const BASE: Self = Self::Flat { openings: EdgeSides::NONE };

    fn volume_shape(self) -> Option<VolumeShape> {
        Some(self)
    }

    fn from_volume_shape(shape: VolumeShape) -> Self {
        shape
    }
}

/// Root of the wall placement controls, created and removed with the wall placement session.
#[derive(Component)]
pub(crate) struct GridPlacerUiForWall;
impl GridPlacerUiForWall {
    /// Builds the controls with the style and openings selected in the placement session.
    fn on_add_construct_grid_placer_ui(
        trigger: On<Add<GridPlacerUiForWall>>,
        mut commands: Commands,
        styles: Res<WallStyles>,
        placer_session: Single<(&WallStyleKey, &VolumeShape), With<GridPlacerSession>>,
    ) {
        let (&selected, &shape) = placer_session.into_inner();
        commands.entity(trigger.entity)
            .insert(controls_bar_node())
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

                spawn_opening_selector(parent, shape);
            });
    }

    fn on_begin_placing_start_wall_session(
        trigger: On<BeginPlacing>,
        mut commands: Commands,
        placer_session: Single<Entity, With<GridPlacerSession>>,
    ) {
        let BeginPlacing(MapObject::Wall) = *trigger else { return };
        commands.entity(*placer_session).insert((WallStyleKey::default(), VolumeShape::default()));
        commands.spawn(GridPlacerUiForWall);
    }

    fn on_stop_placing_despawn_grid_placer_ui(
        _trigger: On<StopPlacing>,
        mut commands: Commands,
        existing_ui: Single<Entity, With<GridPlacerUiForWall>>,
    ) {
        commands.entity(existing_ui.into_inner()).despawn();
    }

    /// Updates swatch borders to match the session's style.
    fn on_ghost_rebuild_request_refresh_style_swatches(
        _trigger: On<GridPlacerGhostRebuildRequest>,
        placer_session: Single<&WallStyleKey, With<GridPlacerSession>>,
        mut swatches: Query<(&WallStyleButton, &mut BorderColor)>,
    ) {
        let selected = *placer_session.into_inner();
        for (&WallStyleButton(key), mut frame) in swatches.iter_mut() {
            *frame = BorderColor::from(WallStyleButton::frame_color(key, selected));
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
        if key == selected { CONTROL_FRAME_SELECTED } else { CONTROL_FRAME_IDLE }
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
