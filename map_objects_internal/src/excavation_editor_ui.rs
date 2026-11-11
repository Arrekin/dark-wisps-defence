//! Excavation placement uses the shared tunnel opening buttons and ramp cycle to edit
//! [`ExcavationShape`] on the placer's session entity.

use bevy::{input::common_conditions::input_just_pressed, prelude::*};

use game_core::prelude::MapObject;
use grids::placement::{BeginPlacing, StopPlacing};
use indexes::placement::GridPlacerSession;
use map_objects::prelude::{ExcavationShape, VolumeShape};

use crate::volume_shape_controls::{
    SHAPE_KEY, SelectableShape,
    controls_bar_node, cycle_shape, on_ghost_rebuild_request_refresh_opening_selector, spawn_opening_selector,
};

pub(crate) struct ExcavationEditorUiPlugin;
impl Plugin for ExcavationEditorUiPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Update, cycle_shape::<ExcavationShape>.run_if(input_just_pressed(SHAPE_KEY)).run_if(any_with_component::<GridPlacerUiForExcavation>))
            .add_observer(GridPlacerUiForExcavation::on_add_construct_grid_placer_ui)
            .add_observer(GridPlacerUiForExcavation::on_begin_placing_start_excavation_session)
            .add_observer(GridPlacerUiForExcavation::on_stop_placing_despawn_grid_placer_ui)
            .add_observer(on_ghost_rebuild_request_refresh_opening_selector::<ExcavationShape>);
    }
}

impl SelectableShape for ExcavationShape {
    const BASE: Self = Self::Pit;

    fn volume_shape(self) -> Option<VolumeShape> {
        match self {
            Self::Pit => None,
            Self::Shaped(shape) => Some(shape),
        }
    }

    fn from_volume_shape(shape: VolumeShape) -> Self {
        Self::Shaped(shape)
    }
}

/// Root of the excavation placement controls, created and removed with the excavation placement
/// session.
#[derive(Component)]
pub(crate) struct GridPlacerUiForExcavation;
impl GridPlacerUiForExcavation {
    fn on_add_construct_grid_placer_ui(
        trigger: On<Add<GridPlacerUiForExcavation>>,
        mut commands: Commands,
        placer_session: Single<&ExcavationShape, With<GridPlacerSession>>,
    ) {
        let shape = *placer_session.into_inner();
        commands.entity(trigger.entity)
            .insert(controls_bar_node())
            .with_children(|parent| spawn_opening_selector(parent, shape));
    }

    fn on_begin_placing_start_excavation_session(
        trigger: On<BeginPlacing>,
        mut commands: Commands,
        placer_session: Single<Entity, With<GridPlacerSession>>,
    ) {
        let BeginPlacing(MapObject::Excavation) = *trigger else { return };
        commands.entity(*placer_session).insert(ExcavationShape::BASE);
        commands.spawn(GridPlacerUiForExcavation);
    }

    fn on_stop_placing_despawn_grid_placer_ui(
        _trigger: On<StopPlacing>,
        mut commands: Commands,
        existing_ui: Single<Entity, With<GridPlacerUiForExcavation>>,
    ) {
        commands.entity(existing_ui.into_inner()).despawn();
    }
}
