//! Excavation placement uses the shared tunnel opening buttons and ramp cycle to edit
//! [`ExcavationShape`] on the placer's session entity.
//!
//! In admin mode the bar also carries the replace switch, which toggles `ReplaceExisting` on the
//! session.

use bevy::{input::common_conditions::input_just_pressed, prelude::*};

use game_core::prelude::{GridArea, MapObject};
use grids::placement::{BeginPlacing, PlacementControls, PlacementModes, PlacementSession};
use map_objects::prelude::{ExcavationShape, VolumeShape};
use states::AdminMode;

use crate::volume_shape_controls::{
    ReplaceToggleButton, SHAPE_KEY, SelectableShape,
    controls_bar_node, cycle_shape, on_ghost_rebuild_request_refresh_opening_selector, spawn_opening_selector,
};
use crate::{excavations::EXCAVATION_GRID_IMPRINT, tunnel_reveal::RevealsTunnels};

pub(crate) struct ExcavationEditorUiPlugin;
impl Plugin for ExcavationEditorUiPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Update, cycle_shape::<ExcavationShape>.run_if(input_just_pressed(SHAPE_KEY)).run_if(any_with_component::<GridPlacerUiForExcavation>))
            .add_observer(GridPlacerUiForExcavation::on_add_construct_grid_placer_ui)
            .add_observer(GridPlacerUiForExcavation::on_begin_placing_start_excavation_session)
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
#[require(PlacementControls, RevealsTunnels)]
pub(crate) struct GridPlacerUiForExcavation;
impl GridPlacerUiForExcavation {
    fn on_add_construct_grid_placer_ui(
        trigger: On<Add<GridPlacerUiForExcavation>>,
        mut commands: Commands,
        admin_mode: Res<State<AdminMode>>,
        placement_session: Single<&ExcavationShape, With<PlacementSession>>,
    ) {
        let shape = *placement_session.into_inner();
        commands.entity(trigger.entity)
            .insert(controls_bar_node())
            .with_children(|parent| {
                spawn_opening_selector(parent, shape);
                if admin_mode.get().is_enabled() {
                    parent.spawn(ReplaceToggleButton);
                }
            });
    }

    fn on_begin_placing_start_excavation_session(
        trigger: On<BeginPlacing>,
        mut commands: Commands,
        placement_session: Single<Entity, With<PlacementSession>>,
    ) {
        let BeginPlacing { object: MapObject::Excavation, origin } = *trigger else { return };
        commands.entity(*placement_session).insert((
            GridArea::new(origin, EXCAVATION_GRID_IMPRINT),
            PlacementModes::on_press(),
            ExcavationShape::BASE,
        ));
        commands.spawn(GridPlacerUiForExcavation);
    }
}
