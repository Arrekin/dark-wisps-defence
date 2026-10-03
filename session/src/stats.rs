use bevy::prelude::*;

use logging::prelude::*;
use persistence::prelude::*;
use states::MapLoadingStage;
use wisps::prelude::WispDied;

pub struct StatsPlugin;
impl Plugin for StatsPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(OnEnter(MapLoadingStage::Init), |mut commands: Commands| { commands.insert_resource(StatsWispsKilled::default()); })
            .add_observer(on_wisp_died_increment_stats)
            .add_systems(CollectSave, collect_stats)
            .register_loader(MapLoadingStage::LoadResources, "stats", load_stats);
    }
}

#[derive(Resource, Default)]
pub struct StatsWispsKilled(pub usize);

fn on_wisp_died_increment_stats(
    _trigger: On<WispDied>,
    mut stats_wisps_killed: ResMut<StatsWispsKilled>,
) {
    stats_wisps_killed.0 += 1;
}

fn collect_stats(
    stats_wisps_killed: Res<StatsWispsKilled>,
    mut save: SaveWriter,
) {
    let wisps_killed = stats_wisps_killed.0;
    save.submit(move |ctx| {
        ctx.save_stat("wisps_killed", wisps_killed as f32)?;
        Ok(())
    });
}

#[log_tags(Tag::GameLoad)]
fn load_stats(ctx: &mut LoadContext) -> LoadResult {
    let wisps_killed = ctx.stat("wisps_killed")
        .inspect_err(|error| warn_dev!("Wisps killed stat not read from save ({error}); starting at 0"))
        .unwrap_or(0.0) as usize;
    ctx.insert_resource(StatsWispsKilled(wisps_killed));
    Ok(())
}
