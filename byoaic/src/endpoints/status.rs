use axum::{Json, extract};
use bevy::prelude::*;
use serde::Serialize;

use game_core::prelude::*;
use session::GameClock;
use states::{AdminMode, prelude::*};

use crate::server::{IngressClosed, ServerState};

#[derive(Serialize)]
pub(crate) struct GameStatus {
    game_state: GameState,
    admin_mode: AdminMode,
    ui_interaction: UiInteraction,
    map_loading_stage: MapLoadingStage,
    map: MapInfo,
    game_clock: GameClock,
}

/// `GET /game/status` — snapshot of the game's states, the current map and the game clock.
///
/// `map` switches to the new map at `MapLoadingStage::LoadMapInfo`, so early in a load it still
/// describes the previous one.
pub(crate) async fn get_game_status(
    extract::State(server): extract::State<ServerState>,
) -> Result<Json<GameStatus>, IngressClosed> {
    server.query(read_game_status).await.map(Json)
}

fn read_game_status(
    game_state: Res<State<GameState>>,
    admin_mode: Res<State<AdminMode>>,
    ui_interaction: Res<State<UiInteraction>>,
    map_loading_stage: Res<State<MapLoadingStage>>,
    map_info: Res<MapInfo>,
    game_clock: Res<GameClock>,
) -> GameStatus {
    GameStatus {
        game_state: *game_state.get(),
        admin_mode: *admin_mode.get(),
        ui_interaction: ui_interaction.get().clone(),
        map_loading_stage: map_loading_stage.get().clone(),
        map: map_info.clone(),
        game_clock: game_clock.clone(),
    }
}
