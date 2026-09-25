use axum::{Json, extract};
use bevy::prelude::*;
use serde::Deserialize;

use states::{SetGamePaused, SetGamePausedReport};

use crate::{
    collection::ResponseCollection,
    forwarding::spawn_request,
    requests::ResponseSender,
    server::{IngressClosed, ServerState},
};

#[derive(Deserialize)]
pub(crate) struct PauseBody {
    paused: bool,
}

/// `POST /game/pause` — sets the requested pause state and reports the decision.
///
/// Body: [`PauseBody`]
pub(crate) async fn post_game_pause(
    extract::State(server): extract::State<ServerState>,
    Json(body): Json<PauseBody>,
) -> Result<Json<ResponseCollection>, IngressClosed> {
    server.request(begin_pause_request, body).await.map(Json)
}

/// The response goes out in `Last` of the dispatch frame. Ingress runs in `PreUpdate`, before
/// `StateTransition`, so an `Applied` pause state is already in effect by then.
fn begin_pause_request(In((sender, body)): In<(ResponseSender, PauseBody)>, mut commands: Commands) {
    let response = spawn_request::<SetGamePausedReport>(&mut commands, sender);
    commands.trigger(SetGamePaused { paused: body.paused, response });
}
