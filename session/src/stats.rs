use bevy::prelude::*;

use persistence::{
    prelude::{AppGameLoadSaveExtension, CollectSave, GameDbHelpers, LoadContext, SaveWriter},
    rusqlite,
};
use states::MapLoadingStage;

pub struct StatsPlugin;
impl Plugin for StatsPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(OnEnter(MapLoadingStage::Init), |mut commands: Commands| { commands.insert_resource(StatsWispsKilled::default()); })
            .add_systems(CollectSave, collect_stats)
            .register_loader(MapLoadingStage::LoadResources, "stats_wisps_killed", load_stats)
            ;
    }
}

#[derive(Resource, Default)]
pub struct StatsWispsKilled(pub usize);

fn collect_stats(
    stats_wisps_killed: Res<StatsWispsKilled>,
    mut save: SaveWriter,
) {
    let wisps_killed = stats_wisps_killed.0;
    save.submit(move |tx| {
        tx.save_stat("wisps_killed", wisps_killed as f32)?;
        Ok(())
    });
}

fn load_stats(ctx: &mut LoadContext) -> rusqlite::Result<()> {
    // Load wisps_killed stat
    let wisps_killed = ctx.conn.get_stat("wisps_killed").unwrap_or(0.0) as usize;
    ctx.insert_resource(StatsWispsKilled(wisps_killed));
    Ok(())
}
