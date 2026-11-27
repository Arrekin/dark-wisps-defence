//! Placement controls for editing [`WallStyleKey`] and [`StructureShape`] on
//! [`UiPlacementSession`], plus a [`ReplaceExisting`](grids::placement::ReplaceExisting) toggle in [`AdminMode`].
//!
//! Style and shape changes trigger [`GridPlacerGhostRebuildRequest`], which rebuilds the ghost
//! preview and refreshes control selection highlights.

use bevy::{input::common_conditions::input_just_pressed, prelude::*, ui_widgets::Button};

use game_core::prelude::{CELL_SIZE, EdgeSides, MapObject, ObjectFaceRequest};
use grids::placement::{GridPlacerGhostRebuildRequest, PlacementControls, UiPlacementSession};
use map_objects::{prelude::StructureShape, wall_style::{WallStyleKey, WallStyles}};
use states::AdminMode;

use crate::structure_shape_controls::{
    CONTROL_FRAME_IDLE, CONTROL_FRAME_SELECTED, ReplaceToggleButton, SHAPE_KEY, SelectableShape,
    controls_bar_node, cycle_shape, on_ghost_rebuild_request_refresh_hollowed_side_selector, spawn_hollowed_side_selector,
};
use crate::tunnel_reveal::RevealsTunnels;

pub(crate) struct WallEditorUiPlugin;
impl Plugin for WallEditorUiPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Update, cycle_shape::<StructureShape>.run_if(input_just_pressed(SHAPE_KEY)).run_if(any_with_component::<GridPlacerUiForWall>))
            .add_observer(GridPlacerUiForWall::on_add_construct_grid_placer_ui)
            .add_observer(GridPlacerUiForWall::on_ghost_rebuild_request_refresh_style_swatches)
            .add_observer(on_ghost_rebuild_request_refresh_hollowed_side_selector::<StructureShape>);
    }
}

impl SelectableShape for StructureShape {
    const BASE: Self = Self::Flat { hollowed_sides: EdgeSides::NONE };

    fn structure_shape(self) -> Option<StructureShape> {
        Some(self)
    }

    fn from_structure_shape(shape: StructureShape) -> Self {
        shape
    }
}

/// Root UI node for wall placement controls.
#[derive(Component)]
#[require(PlacementControls, RevealsTunnels)]
pub(crate) struct GridPlacerUiForWall;
impl GridPlacerUiForWall {
    fn on_add_construct_grid_placer_ui(
        trigger: On<Add<GridPlacerUiForWall>>,
        mut commands: Commands,
        styles: Res<WallStyles>,
        admin_mode: Res<State<AdminMode>>,
        ui_placement_session: Single<(&WallStyleKey, &StructureShape), With<UiPlacementSession>>,
    ) {
        let (&selected, &shape) = ui_placement_session.into_inner();
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

                spawn_hollowed_side_selector(parent, shape);
                if admin_mode.get().is_enabled() {
                    parent.spawn(ReplaceToggleButton);
                }
            });
    }

    /// Updates swatch borders to match the UI placement session's style.
    fn on_ghost_rebuild_request_refresh_style_swatches(
        _trigger: On<GridPlacerGhostRebuildRequest>,
        ui_placement_session: Single<&WallStyleKey, With<UiPlacementSession>>,
        mut swatches: Query<(&WallStyleButton, &mut BorderColor)>,
    ) {
        let selected = *ui_placement_session.into_inner();
        for (&WallStyleButton(key), mut frame) in swatches.iter_mut() {
            *frame = BorderColor::from(WallStyleButton::frame_color(key, selected));
        }
    }
}

/// A style button containing a swatch and its name.
/// The selection border surrounds the button so it does not obscure the preview.
#[derive(Component, Clone, Copy)]
#[require(Button)]
pub(crate) struct WallStyleButton(WallStyleKey);
impl WallStyleButton {
    fn frame_color(key: WallStyleKey, selected: WallStyleKey) -> Color {
        if key == selected { CONTROL_FRAME_SELECTED } else { CONTROL_FRAME_IDLE }
    }

    fn on_click_select_style(
        trigger: On<PointerClick>,
        mut commands: Commands,
        ui_placement_session: Single<Entity, With<UiPlacementSession>>,
        buttons: Query<&WallStyleButton>,
    ) {
        let Ok(&WallStyleButton(selected)) = buttons.get(trigger.entity) else { return; };
        commands.entity(*ui_placement_session).insert(selected);
        commands.trigger(GridPlacerGhostRebuildRequest);
    }
}
