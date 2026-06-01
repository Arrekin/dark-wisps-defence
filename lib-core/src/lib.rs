use bevy::app::{App, Plugin};

pub mod buildings;
pub mod camera;
pub mod game_clock;
pub mod grids;
pub mod logging;
pub mod mouse;
pub mod placement;
pub mod states;
pub mod common;
pub mod utils;
pub mod persistence;
pub mod wisps;
pub mod map_objects;
pub mod modifiers;
pub mod effects;
pub mod effect_visuals;
pub mod damage;
pub mod shards;
pub mod post_processing;

pub struct LibCorePlugin;
impl Plugin for LibCorePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            logging::LoggingPlugin,
            camera::CameraPlugin,
            mouse::MousePlugin,
            states::StatesPlugin,
            common::CommonPlugin,
            grids::GridPlugin,
            buildings::BuildingsPlugin,
            persistence::LoadSavePlugin,
            game_clock::GameClockPlugin,
            modifiers::ModifiersPlugin,
            effects::EffectsPlugin,
            effect_visuals::EffectVisualsPlugin,
            damage::DamagePlugin,
            shards::ShardsPlugin,
        ));
    }
}


pub mod prelude {
    pub use bevy::platform::collections::{HashMap, HashSet};

    pub use crate::buildings::buildings_prelude::*;
    pub use crate::grids::grids_prelude::*;
    pub use crate::logging::logging_prelude::*;
    pub use crate::mouse::mouse_prelude::*;
    pub use crate::states::states_prelude::*;
    pub use crate::common::common_prelude::*;
    pub use crate::persistence::load_save_prelude::*;
    pub use crate::wisps::wisps_prelude::*;
    pub use crate::modifiers::modifiers_prelude::*;
    pub use crate::effects::effects_prelude::*;
    pub use crate::effect_visuals::effect_visuals_prelude::*;
    pub use crate::damage::damage_prelude::*;
    pub use crate::shards::shards_prelude::*;
    // Re-export the derive macros
    pub use lib_derive::{Property, SSS};
}

pub mod lib_prelude {
    pub use serde::{Deserialize, Serialize};
    pub use bevy::prelude::*;

    pub use crate::prelude::*;
}