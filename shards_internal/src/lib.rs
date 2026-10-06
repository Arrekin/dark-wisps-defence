use bevy::app::{App, Plugin};

pub(crate) mod sockets;
pub(crate) mod starting_shards;
pub(crate) mod blueprints;
pub(crate) mod shard_catalog;
pub(crate) mod outcomes;

pub struct ShardsPlugin;
impl Plugin for ShardsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            sockets::ShardSocketsPlugin,
            starting_shards::StartingShardsPlugin,
            blueprints::ShardBlueprintsPlugin,
            shard_catalog::ShardCatalogPlugin,
            outcomes::ShardOutcomesPlugin,
        ));
    }
}
