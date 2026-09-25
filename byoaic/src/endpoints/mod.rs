mod close;
mod load_map;
mod maps;
mod pause;
mod screenshot;
mod status;

use axum::{Router, routing::{get, post}};

use crate::server::ServerState;

pub(crate) fn router(state: ServerState) -> Router {
    Router::new()
        .route("/game/status", get(status::get_game_status))
        .route("/game/maps", get(maps::get_game_maps))
        .route("/game/pause", post(pause::post_game_pause))
        .route("/game/load-map", post(load_map::post_game_load_map))
        .route("/game/screenshot", post(screenshot::post_game_screenshot))
        .route("/game/close", post(close::post_game_close))
        .with_state(state)
}
