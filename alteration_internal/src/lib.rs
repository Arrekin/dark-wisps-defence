use bevy::app::{App, Plugin};

pub(crate) mod modifiers;
pub(crate) mod effects;

pub struct AlterationPlugin;
impl Plugin for AlterationPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            modifiers::ModifiersPlugin,
            effects::EffectsPlugin,
        ));
    }
}
