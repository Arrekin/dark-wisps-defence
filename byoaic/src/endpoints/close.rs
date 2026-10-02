use axum::{extract, http::StatusCode};
use bevy::prelude::*;

use logging::prelude::*;

use crate::server::{IngressClosed, ServerState};

/// `POST /game/close` — needs no body. Queues `AppExit` and returns HTTP 202 without
/// waiting for shutdown.
pub(crate) async fn post_game_close(extract::State(server): extract::State<ServerState>) -> Result<StatusCode, IngressClosed> {
    server.submit(request_app_exit)?;
    Ok(StatusCode::ACCEPTED)
}

#[log_tags(Tag::Byoaic)]
fn request_app_exit(world: &mut World) {
    #[info_dev("Game exit requested")]
    world.write_message(AppExit::Success);
}
