use bevy::app::{App, Plugin};

pub mod resources;
pub mod almanach;
pub mod stats;
pub mod modifiers;
pub mod placement;

pub struct LibInventoryPlugin;
impl Plugin for LibInventoryPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                almanach::AlmanachPlugin,
                resources::ResourcesPlugin,
                stats::StatsPlugin,
                modifiers::ModifiersPlugin,
            ));
    }
}

pub mod prelude {
    pub use crate::resources::resources_prelude::*;
    pub use crate::almanach::almanach_prelude::*;
    pub use crate::modifiers::modifiers_prelude::*;

    // Re-export the derive macros
    pub use lib_derive::Modifier;
}

pub mod lib_prelude {
    pub use serde::{Deserialize, Serialize};
    pub use bevy::prelude::*;
    pub use bevy::platform::collections::HashMap;

    pub use lib_core::prelude::*;
    pub use lib_grid::prelude::*;
    
    pub use super::prelude::*;
}