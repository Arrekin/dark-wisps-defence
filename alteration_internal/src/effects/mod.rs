pub(crate) mod visual;
pub(crate) mod brittle;
pub(crate) mod slow;

use bevy::prelude::*;

pub(crate) struct EffectsPlugin;
impl Plugin for EffectsPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                visual::EffectVisualsPlugin,
                brittle::BrittleEffectPlugin,
                slow::SlowEffectPlugin,
            ));
    }
}
