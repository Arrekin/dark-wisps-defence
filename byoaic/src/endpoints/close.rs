use axum::{extract, http::StatusCode};
use bevy::prelude::*;

use crate::server::{IngressClosed, ServerState};

/// `POST /game/close` — needs no body. Queues `AppExit` and returns HTTP 202 without
/// waiting for shutdown.
pub(crate) async fn post_game_close(extract::State(server): extract::State<ServerState>) -> Result<StatusCode, IngressClosed> {
    server.submit(request_app_exit)?;
    Ok(StatusCode::ACCEPTED)
}

fn request_app_exit(world: &mut World) {
    world.write_message(AppExit::Success);
}
