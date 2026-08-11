use bevy::prelude::*;

use game_core::prelude::ShardType;
use persistence::{
    creating_new_map,
    prelude::{AppGameLoadSaveExtension, CollectSave, LoadContext, SaveWriter},
    rusqlite,
};
use shards::blueprints::ShardBlueprints;
use states::prelude::MapLoadingStage;

pub struct ShardBlueprintsPlugin;
impl Plugin for ShardBlueprintsPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<ShardBlueprints>()
            .add_systems(OnEnter(MapLoadingStage::Init), |mut commands: Commands| { commands.insert_resource(ShardBlueprints::default()); })
            .add_systems(CollectSave, collect_shard_blueprints)
            .register_loader(MapLoadingStage::LoadResources, "shard_blueprints", load_shard_blueprints)
            .add_systems(OnEnter(MapLoadingStage::LoadResources), seed_starting_blueprints.run_if(creating_new_map))
            ;
    }
}

fn seed_starting_blueprints(mut blueprints: ResMut<ShardBlueprints>) {
    blueprints.unlock(ShardType::Range);
    blueprints.unlock(ShardType::Damage);
    blueprints.unlock(ShardType::Speed);
}

fn collect_shard_blueprints(blueprints: Res<ShardBlueprints>, mut save: SaveWriter) {
    let rows: Vec<String> = blueprints
        .iter()
        .map(|shard_type| shard_type.to_string())
        .collect();
    if rows.is_empty() { return; }
    save.submit(move |tx| {
        for shard_type in rows {
            tx.execute(
                "INSERT OR REPLACE INTO shard_blueprints (shard_type) VALUES (?1)",
                rusqlite::params![shard_type],
            )?;
        }
        Ok(())
    });
}

fn load_shard_blueprints(ctx: &mut LoadContext) -> rusqlite::Result<()> {
    let mut stmt = ctx.conn.prepare("SELECT shard_type FROM shard_blueprints")?;
    let mut rows = stmt.query([])?;

    let mut blueprints = ShardBlueprints::default();
    while let Some(row) = rows.next()? {
        let shard_str: String = row.get(0)?;
        if let Ok(shard_type) = shard_str.parse::<ShardType>() {
            blueprints.unlock(shard_type);
        }
    }

    ctx.insert_resource(blueprints);
    Ok(())
}
