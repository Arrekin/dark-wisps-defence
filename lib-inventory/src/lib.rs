use bevy::app::{App, Plugin};

pub mod almanach;
pub mod effects;
pub mod placement;
pub mod resources;
pub mod shard_blueprints;
pub mod shard_catalog;
pub mod shard_inventory;
pub mod stats;

pub struct LibInventoryPlugin;
impl Plugin for LibInventoryPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                almanach::AlmanachPlugin,
                effects::EffectsPlugin,
                resources::ResourcesPlugin,
                stats::StatsPlugin,
                shard_blueprints::ShardBlueprintsPlugin,
                shard_catalog::ShardCatalogPlugin,
                shard_inventory::ShardInventoryPlugin,
            ));
    }
}

pub mod prelude {
    pub use crate::almanach::almanach_prelude::*;
    pub use crate::resources::resources_prelude::*;
    pub use crate::shard_blueprints::shard_blueprints_prelude::*;
    pub use crate::shard_inventory::shard_inventory_prelude::*;
}

pub mod lib_prelude {
    pub use serde::{Deserialize, Serialize};
    pub use bevy::prelude::*;

    pub use lib_core::prelude::*;
    pub use lib_grid::prelude::*;
    
    pub use super::prelude::*;
}