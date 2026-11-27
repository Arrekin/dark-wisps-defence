//! Placement controls for editing [`ExcavationShape`] (hollowed sides and ramp orientation) on
//! [`UiPlacementSession`], plus a [`ReplaceExisting`](grids::placement::ReplaceExisting) toggle in [`AdminMode`].

use bevy::{input::common_conditions::input_just_pressed, prelude::*};

use grids::placement::{PlacementControls, UiPlacementSession};
use map_objects::prelude::{ExcavationShape, StructureShape};
use states::AdminMode;

use crate::structure_shape_controls::{
    ReplaceToggleButton, SHAPE_KEY, SelectableShape,
    controls_bar_node, cycle_shape, on_ghost_rebuild_request_refresh_hollowed_side_selector, spawn_hollowed_side_selector,
};
use crate::tunnel_reveal::RevealsTunnels;

pub(crate) struct ExcavationEditorUiPlugin;
impl Plugin for ExcavationEditorUiPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Update, cycle_shape::<ExcavationShape>.run_if(input_just_pressed(SHAPE_KEY)).run_if(any_with_component::<GridPlacerUiForExcavation>))
            .add_observer(GridPlacerUiForExcavation::on_add_construct_grid_placer_ui)
            .add_observer(on_ghost_rebuild_request_refresh_hollowed_side_selector::<ExcavationShape>);
    }
}

impl SelectableShape for ExcavationShape {
    const BASE: Self = Self::Pit;

    fn structure_shape(self) -> Option<StructureShape> {
        match self {
            Self::Pit => None,
            Self::Shaped(shape) => Some(shape),
        }
    }

    fn from_structure_shape(shape: StructureShape) -> Self {
        Self::Shaped(shape)
    }
}

/// Root UI node for excavation placement controls.
#[derive(Component)]
#[require(PlacementControls, RevealsTunnels)]
pub(crate) struct GridPlacerUiForExcavation;
impl GridPlacerUiForExcavation {
    fn on_add_construct_grid_placer_ui(
        trigger: On<Add<GridPlacerUiForExcavation>>,
        mut commands: Commands,
        admin_mode: Res<State<AdminMode>>,
        ui_placement_session: Single<&ExcavationShape, With<UiPlacementSession>>,
    ) {
        let shape = *ui_placement_session.into_inner();
        commands.entity(trigger.entity)
            .insert(controls_bar_node())
            .with_children(|parent| {
                spawn_hollowed_side_selector(parent, shape);
                if admin_mode.get().is_enabled() {
                    parent.spawn(ReplaceToggleButton);
                }
            });
    }
}
