use bevy::prelude::*;

use game_core::shared_gpu::{SharedGpuBinding, SharedGpuWriter};
use logging::prelude::*;
use persistence::prelude::*;
use session::GameClock;
use states::prelude::{GameState, MapLoadingStage};

pub(crate) struct GameClockPlugin;
impl Plugin for GameClockPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<GameClock>()
            .add_systems(OnEnter(MapLoadingStage::Init), |mut commands: Commands| { commands.insert_resource(GameClock::default()); })
            .add_systems(PreUpdate, advance_game_clock.run_if(in_state(GameState::Running)))
            .add_systems(PostUpdate, upload_gpu_game_time.run_if(resource_changed::<GameClock>))
            .add_systems(CollectSave, collect_game_clock)
            .register_loader(MapLoadingStage::LoadResources, "game_clock", load_game_clock);
    }
}

fn advance_game_clock(time: Res<Time>, mut clock: ResMut<GameClock>) {
    clock.elapsed += time.delta_secs_f64();
}

fn upload_gpu_game_time(
    mut shared_gpu: SharedGpuWriter,
    clock: Res<GameClock>,
) {
    shared_gpu.write(SharedGpuBinding::GameTime, [clock.wrapped()]);
}

fn collect_game_clock(clock: Res<GameClock>, mut save: SaveWriter) {
    let elapsed = clock.elapsed;
    save.submit(move |ctx| {
        ctx.tx.execute(
            "INSERT OR REPLACE INTO game_clock (id, elapsed) VALUES (1, ?1)",
            [elapsed],
        )?;
        Ok(())
    });
}

#[log_tags(Tag::GameLoad)]
fn load_game_clock(ctx: &mut LoadContext) -> LoadResult {
    let elapsed: f64 = ctx.conn
        .query_row("SELECT elapsed FROM game_clock WHERE id = 1", [], |row| row.get(0))
        .inspect_err(|error| warn_dev!("Game clock not read from save ({error}); starting at 0"))
        .unwrap_or(0.0);
    ctx.insert_resource(GameClock { elapsed });
    Ok(())
}
