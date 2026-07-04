use bevy::prelude::*;

use game_core::prelude::SSS;
use persistence::{
    prelude::{AppGameLoadSaveExtension, CollectSave, LoadContext, SaveWriter},
    rusqlite,
};
use states::{GameState, MapLoadingStage};

pub struct GameClockPlugin;
impl Plugin for GameClockPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<GameClock>()
            .add_systems(PreUpdate, GameClock::advance.run_if(in_state(GameState::Running)))
            .add_systems(CollectSave, collect_game_clock)
            .register_loader(MapLoadingStage::LoadResources, "game_clock", load_game_clock)
            ;
    }
}

/// Monotonic game-time counter in seconds. Advances only while `GameState::Running`.
///
/// Timed effects store absolute expiry timestamps relative to this clock.
/// Save and restore `elapsed` to preserve effect timing across sessions.
#[derive(Resource, Default, SSS, Clone)]
pub struct GameClock {
    pub elapsed: f64,
}
impl GameClock {
    fn advance(time: Res<Time>, mut clock: ResMut<GameClock>) {
        clock.elapsed += time.delta_secs_f64();
    }
}

fn collect_game_clock(clock: Res<GameClock>, mut save: SaveWriter) {
    let elapsed = clock.elapsed;
    save.submit(move |tx| {
        tx.execute(
            "INSERT OR REPLACE INTO game_clock (id, elapsed) VALUES (1, ?1)",
            [elapsed],
        )?;
        Ok(())
    });
}

fn load_game_clock(ctx: &mut LoadContext) -> rusqlite::Result<()> {
    let elapsed: f64 = ctx
        .conn
        .prepare("SELECT elapsed FROM game_clock WHERE id = 1")?
        .query_row([], |row| row.get(0))
        .unwrap_or(0.0);
    ctx.insert_resource(GameClock { elapsed });
    Ok(())
}
