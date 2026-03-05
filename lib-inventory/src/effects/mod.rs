use bevy::app::{App, Plugin};

pub mod brittle;

pub struct EffectsPlugin;
impl Plugin for EffectsPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                brittle::BrittleEffectPlugin,
            ));
    }
}