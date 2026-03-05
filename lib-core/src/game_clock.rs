use crate::lib_prelude::*;

pub struct GameClockPlugin;
impl Plugin for GameClockPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<GameClock>()
            .add_systems(PreUpdate, GameClock::advance.run_if(in_state(GameState::Running)))
            .register_db_loader::<GameClock>(MapLoadingStage::LoadResources)
            .register_db_saver(GameClock::on_game_save)
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

    fn on_game_save(mut commands: Commands, clock: Res<GameClock>) {
        commands.queue(SaveableBatchCommand::from_single(clock.clone()));
    }
}
impl Saveable for GameClock {
    fn save(self, tx: &rusqlite::Transaction) -> rusqlite::Result<()> {
        tx.execute(
            "INSERT OR REPLACE INTO game_clock (id, elapsed) VALUES (1, ?1)",
            [self.elapsed],
        )?;
        Ok(())
    }
}
impl Loadable for GameClock {
    fn load(ctx: &mut LoadContext) -> rusqlite::Result<LoadResult> {
        let elapsed: f64 = ctx.conn
            .prepare("SELECT elapsed FROM game_clock WHERE id = 1")?
            .query_row([], |row| row.get(0))
            .unwrap_or(0.0);
        ctx.commands.insert_resource(GameClock { elapsed });
        Ok(LoadResult::Finished)
    }
}
