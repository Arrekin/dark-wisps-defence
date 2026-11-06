//! Wall placement controls: style swatches and a keyboard shortcut for cycling ramp direction.
//! Selections update the active placer, which rebuilds the ghost preview.

use bevy::{input::common_conditions::input_just_pressed, prelude::*, ui_widgets::Button};

use game_core::prelude::{CELL_SIZE, EdgeSide, MapObject, ObjectFaceRequest};
use grids::placement::{BeginPlacing, GridPlacerOverridePropertyRequest, PlacementDirection, PlacementStyle, StopPlacing};
use indexes::placement::GridObjectPlacer;
use map_objects::wall_style::{WallStyleKey, WallStyles};

pub(crate) struct WallEditorUiPlugin;
impl Plugin for WallEditorUiPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Update, GridPlacerUiForWall::cycle_shape.run_if(input_just_pressed(SHAPE_KEY)).run_if(any_with_component::<GridPlacerUiForWall>))
            .add_observer(GridPlacerUiForWall::on_add_construct_grid_placer_ui)
            .add_observer(GridPlacerUiForWall::on_begin_placing_spawn_grid_placer_ui)
            .add_observer(GridPlacerUiForWall::on_stop_placing_despawn_grid_placer_ui);
    }
}

const SHAPE_KEY: KeyCode = KeyCode::KeyR;

// Swatch-frame colors for selected and idle styles.
const SWATCH_FRAME_SELECTED: Color = Color::srgb(0.16, 0.78, 1.00); // cyan
const SWATCH_FRAME_IDLE: Color = Color::srgb(0.15, 0.15, 0.18); // dark grey

/// Root of the style picker, created and removed with the wall placement session.
/// Initializes the selected swatch from the placer's [`PlacementStyle`].
#[derive(Component)]
pub(crate) struct GridPlacerUiForWall;
impl GridPlacerUiForWall {
    fn cycle_shape(
        mut commands: Commands,
        placer: Single<&PlacementDirection, With<GridObjectPlacer>>,
    ) {
        let next = match placer.into_inner().0 {
            None => Some(EdgeSide::Top),
            Some(EdgeSide::Top) => Some(EdgeSide::Right),
            Some(EdgeSide::Right) => Some(EdgeSide::Bottom),
            Some(EdgeSide::Bottom) => Some(EdgeSide::Left),
            Some(EdgeSide::Left) => None,
        };
        commands.trigger(GridPlacerOverridePropertyRequest::Direction(next));
    }

    fn on_add_construct_grid_placer_ui(
        trigger: On<Add<GridPlacerUiForWall>>,
        mut commands: Commands,
        styles: Res<WallStyles>,
        placer: Single<&PlacementStyle, With<GridObjectPlacer>>,
    ) {
        let selected = WallStyleKey::from(*placer.into_inner());
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
                        BorderColor::from(if key == selected { SWATCH_FRAME_SELECTED } else { SWATCH_FRAME_IDLE }),
                        BackgroundColor(Color::BLACK),
                    ))
                    .with_children(|button| {
                        // The face renderer reads PlacementStyle from the swatch node.
                        button
                            .spawn((
                                Node {
                                    width: Val::Px(CELL_SIZE),
                                    height: Val::Px(CELL_SIZE),
                                    ..default()
                                },
                                PlacementStyle(key.0),
                            ))
                            .trigger(|entity| ObjectFaceRequest::ui(entity, MapObject::Wall));
                        button.spawn((Text::new(entry.name.clone()), TextFont::from_font_size(10.0)));
                    })
                    .observe(WallStyleButton::on_click_select_style);
                }
            });
    }

    fn on_begin_placing_spawn_grid_placer_ui(
        trigger: On<BeginPlacing>,
        mut commands: Commands,
    ) {
        let BeginPlacing(MapObject::Wall) = *trigger else { return };
        commands.spawn(GridPlacerUiForWall);
    }

    fn on_stop_placing_despawn_grid_placer_ui(
        _trigger: On<StopPlacing>,
        mut commands: Commands,
        existing_ui: Single<Entity, With<GridPlacerUiForWall>>,
    ) {
        commands.entity(existing_ui.into_inner()).despawn();
    }
}

/// A style button containing a swatch and its name.
/// The selection border surrounds the button so it does not obscure the preview.
#[derive(Component, Clone, Copy)]
#[require(Button, Pickable)]
pub(crate) struct WallStyleButton(pub WallStyleKey);
impl WallStyleButton {
    fn on_click_select_style(
        trigger: On<PointerClick>,
        mut commands: Commands,
        mut buttons: Query<(&WallStyleButton, &mut BorderColor)>,
    ) {
        let Ok((&WallStyleButton(selected), _)) = buttons.get(trigger.entity) else { return; };
        for (button, mut frame) in buttons.iter_mut() {
            *frame = BorderColor::from(if button.0 == selected { SWATCH_FRAME_SELECTED } else { SWATCH_FRAME_IDLE });
        }
        commands.trigger(GridPlacerOverridePropertyRequest::Style(selected.0));
    }
}
