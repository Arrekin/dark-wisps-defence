mod close;
mod map_files;
mod pause;
mod placement;
mod screenshot;
mod status;

use axum::{Router, routing::{get, post}};

use crate::server::ServerState;

pub(crate) fn router(state: ServerState) -> Router {
    Router::new()
        .route("/game/status", get(status::get_game_status))
        .route("/game/scenarios", get(map_files::get_game_scenarios))
        .route("/game/saves", get(map_files::get_game_saves))
        .route("/game/pause", post(pause::post_game_pause))
        .route("/game/place", post(placement::post_game_place))
        .route("/game/remove", post(placement::post_game_remove))
        .route("/game/load", post(map_files::post_game_load))
        .route("/game/save", post(map_files::post_game_save))
        .route("/game/screenshot", post(screenshot::post_game_screenshot))
        .route("/game/close", post(close::post_game_close))
        .with_state(state)
}
