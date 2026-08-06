use bevy::app::{App, Plugin};

pub(crate) mod slots;
pub(crate) mod inventory;
pub(crate) mod blueprints;
pub(crate) mod shard_catalog;
pub(crate) mod outcomes;

pub struct ShardsPlugin;
impl Plugin for ShardsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            slots::ShardSlotsPlugin,
            inventory::ShardInventoryPlugin,
            blueprints::ShardBlueprintsPlugin,
            shard_catalog::ShardCatalogPlugin,
            outcomes::ShardOutcomesPlugin,
        ));
    }
}
