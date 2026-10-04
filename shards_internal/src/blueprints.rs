use bevy::prelude::*;

use game_core::prelude::ShardType;
use persistence::{creating_new_map, prelude::*, rusqlite};
use shards::blueprints::ShardBlueprints;
use states::prelude::MapLoadingStage;

pub(crate) struct ShardBlueprintsPlugin;
impl Plugin for ShardBlueprintsPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<ShardBlueprints>()
            .add_systems(OnEnter(MapLoadingStage::Init), |mut commands: Commands| { commands.insert_resource(ShardBlueprints::default()); })
            .add_systems(CollectSave, collect_shard_blueprints)
            .register_loader(MapLoadingStage::LoadResources, "shard_blueprints", load_shard_blueprints)
            .add_systems(OnEnter(MapLoadingStage::LoadResources), seed_starting_blueprints.run_if(creating_new_map));
    }
}

fn seed_starting_blueprints(mut blueprints: ResMut<ShardBlueprints>) {
    blueprints.unlock(ShardType::Reach);
    blueprints.unlock(ShardType::Strength);
    blueprints.unlock(ShardType::Speed);
}

fn collect_shard_blueprints(blueprints: Res<ShardBlueprints>, mut save: SaveWriter) {
    let shard_blueprints = blueprints.clone();
    save.submit(move |ctx| {
        for shard_type in shard_blueprints.iter() {
            ctx.tx.execute(
                "INSERT OR REPLACE INTO shard_blueprints (shard_type) VALUES (?1)",
                rusqlite::params![shard_type.as_ref()],
            )?;
        }
        Ok(())
    });
}

fn load_shard_blueprints(ctx: &mut LoadContext) -> LoadResult {
    let mut blueprints = ShardBlueprints::default();
    ctx.for_each_row("SELECT shard_type FROM shard_blueprints", |_, row| {
        let shard_type = row.get_parsed(0)?;
        blueprints.unlock(shard_type);
        Ok(())
    })?;
    ctx.insert_resource(blueprints);
    Ok(())
}
