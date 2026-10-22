pub(crate) mod crucible_field;
pub(crate) mod field;
pub(crate) mod force_field;

use bevy::prelude::*;

pub struct FieldsPlugin;
impl Plugin for FieldsPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                crucible_field::CrucibleFieldPlugin,
                field::FieldPlugin,
                force_field::ForceFieldPlugin,
            ));
    }
}
