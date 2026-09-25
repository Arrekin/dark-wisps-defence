use axum::{Json, extract};
use bevy::prelude::*;
use serde::Deserialize;

use persistence::{LoadGameReport, LoadGameSignal, LoadMapConfig, MapFileName};

use crate::{
    collection::ResponseCollection,
    forwarding::spawn_request,
    requests::ResponseSender,
    server::{IngressClosed, ServerState},
};

#[derive(Deserialize)]
pub(crate) struct LoadMapBody {
    file_name: MapFileName,
}

/// `POST /game/load-map` — loads a map from `maps/` and reports why it was rejected, or the loaded
/// map once its `game_start_state` is in effect.
///
/// Body: [`LoadMapBody`]
pub(crate) async fn post_game_load_map(
    extract::State(server): extract::State<ServerState>,
    Json(body): Json<LoadMapBody>,
) -> Result<Json<ResponseCollection>, IngressClosed> {
    server.request(begin_load_map_request, body).await.map(Json)
}

fn begin_load_map_request(In((sender, body)): In<(ResponseSender, LoadMapBody)>, mut commands: Commands) {
    let response = spawn_request::<LoadGameReport>(&mut commands, sender);
    commands.trigger(LoadGameSignal(LoadMapConfig::map(&body.file_name).with_response(response)));
}
