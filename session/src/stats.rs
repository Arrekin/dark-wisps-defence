use bevy::prelude::*;

use game_core::prelude::SSS;
use persistence::{prelude::*, rusqlite};
use states::MapLoadingStage;

pub struct StatsPlugin;
impl Plugin for StatsPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(OnEnter(MapLoadingStage::Init), |mut commands: Commands| { commands.insert_resource(StatsWispsKilled::default()); })
            .register_db_loader::<StatsLoader>(MapLoadingStage::LoadResources)
            .register_db_saver(on_game_save_stats);
    }
}

#[derive(Resource, Default)]
pub struct StatsWispsKilled(pub usize);

struct StatsLoader;
impl Loadable for StatsLoader {
    fn load(ctx: &mut LoadContext) -> rusqlite::Result<LoadResult> {
        // Load wisps_killed stat
        let wisps_killed = ctx.conn.get_stat("wisps_killed").unwrap_or(0.0) as usize;
        ctx.commands.insert_resource(StatsWispsKilled(wisps_killed));
        Ok(LoadResult::Finished)
    }
}

#[derive(SSS)]
struct StatsSaver {
    wisps_killed: usize,
}
impl Saveable for StatsSaver {
    fn save(self, tx: &rusqlite::Transaction) -> rusqlite::Result<()> {
        tx.save_stat("wisps_killed", self.wisps_killed as f32)?;
        Ok(())
    }
}

fn on_game_save_stats(
    mut commands: Commands,
    stats_wisps_killed: Res<StatsWispsKilled>,
) {
    let saver = StatsSaver { wisps_killed: stats_wisps_killed.0 };
    commands.queue(SaveableBatchCommand::from_single(saver));
}
