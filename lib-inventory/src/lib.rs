use bevy::app::{App, Plugin};

pub mod almanach;
pub mod effects;
pub mod placement;
pub mod resources;
pub mod stats;
pub mod upgrades;

pub struct LibInventoryPlugin;
impl Plugin for LibInventoryPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                almanach::AlmanachPlugin,
                effects::EffectsPlugin,
                resources::ResourcesPlugin,
                stats::StatsPlugin,
                upgrades::UpgradesPlugin,
            ));
    }
}

pub mod prelude {
    pub use crate::almanach::almanach_prelude::*;
    pub use crate::resources::resources_prelude::*;
    pub use crate::upgrades::upgrades_prelude::*;
}

pub mod lib_prelude {
    pub use serde::{Deserialize, Serialize};
    pub use bevy::prelude::*;

    pub use lib_core::prelude::*;
    pub use lib_grid::prelude::*;
    
    pub use super::prelude::*;
}