use bevy::prelude::*;

use game_core::prelude::{ShardType, SSS};
use persistence::{
    prelude::{AppGameLoadSaveExtension, Loadable, LoadContext, LoadResult, Saveable, SaveableBatchCommand},
    rusqlite,
};
use shards::blueprints::ShardBlueprints;
use states::prelude::MapLoadingStage;

pub struct ShardBlueprintsPlugin;
impl Plugin for ShardBlueprintsPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<ShardBlueprints>()
            .register_db_saver(save_shard_blueprints_on_game_save)
            .register_db_loader::<ShardBlueprintsLoader>(MapLoadingStage::LoadResources)
            .add_systems(OnEnter(MapLoadingStage::Ready), seed_starting_blueprints)
            ;
    }
}

fn seed_starting_blueprints(mut blueprints: ResMut<ShardBlueprints>) {
    blueprints.unlock(ShardType::Range);
    blueprints.unlock(ShardType::Damage);
    blueprints.unlock(ShardType::Speed);
}

fn save_shard_blueprints_on_game_save(mut commands: Commands, blueprints: Res<ShardBlueprints>) {
    let batch = blueprints
        .iter()
        .map(|shard_type| ShardBlueprintsSaveData { shard_type })
        .collect::<SaveableBatchCommand<_>>();
    commands.queue(batch);
}

#[derive(Clone, Debug, SSS)]
struct ShardBlueprintsSaveData {
    shard_type: ShardType,
}

impl Saveable for ShardBlueprintsSaveData {
    fn save(self, tx: &rusqlite::Transaction) -> rusqlite::Result<()> {
        tx.execute(
            "INSERT OR REPLACE INTO shard_blueprints (shard_type) VALUES (?1)",
            rusqlite::params![self.shard_type.to_string()],
        )?;
        Ok(())
    }
}

struct ShardBlueprintsLoader;
impl Loadable for ShardBlueprintsLoader {
    fn load(ctx: &mut LoadContext) -> rusqlite::Result<LoadResult> {
        let mut stmt = ctx.conn.prepare("SELECT shard_type FROM shard_blueprints")?;
        let mut rows = stmt.query([])?;

        let mut blueprints = ShardBlueprints::default();
        while let Some(row) = rows.next()? {
            let shard_str: String = row.get(0)?;
            if let Ok(shard_type) = shard_str.parse::<ShardType>() {
                blueprints.unlock(shard_type);
            }
        }

        ctx.commands.insert_resource(blueprints);
        Ok(LoadResult::Finished)
    }
}
