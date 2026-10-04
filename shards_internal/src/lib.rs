use bevy::app::{App, Plugin};

pub(crate) mod slots;
pub(crate) mod starting_shards;
pub(crate) mod blueprints;
pub(crate) mod shard_catalog;
pub(crate) mod outcomes;

pub struct ShardsPlugin;
impl Plugin for ShardsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            slots::ShardSlotsPlugin,
            starting_shards::StartingShardsPlugin,
            blueprints::ShardBlueprintsPlugin,
            shard_catalog::ShardCatalogPlugin,
            outcomes::ShardOutcomesPlugin,
        ));
    }
}
