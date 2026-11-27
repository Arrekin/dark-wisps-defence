//! Listing, loading and saving map files.

use axum::{Json, extract};
use bevy::prelude::*;
use serde::Deserialize;

use persistence::{
    LoadGameReport, LoadGameSignal, LoadMapConfig, MapFile, MapFileName, MapListEntry, SaveGameReport, SaveGameSignal,
    ScenarioList,
};

use crate::{
    collection::ResponseCollection,
    forwarding::spawn_request,
    requests::ResponseSender,
    server::{IngressClosed, ServerState},
};

/// `GET /game/scenarios` — every scenario in `maps/` with its header.
pub(crate) async fn get_game_scenarios(
    extract::State(server): extract::State<ServerState>,
) -> Result<Json<Vec<MapListEntry>>, IngressClosed> {
    server.query(read_game_scenarios).await.map(Json)
}

fn read_game_scenarios(mut scenario_list: ResMut<ScenarioList>) -> Vec<MapListEntry> {
    scenario_list.entries().to_vec()
}

/// `GET /game/saves` — every save in `saves/`.
pub(crate) async fn get_game_saves(
    extract::State(server): extract::State<ServerState>,
) -> Result<Json<Vec<MapFile>>, IngressClosed> {
    server.query(MapFile::list_saves).await.map(Json)
}

/// `POST /game/load` — loads a map file and reports why it was rejected, or the loaded map
/// once its `game_start_state` is in effect.
///
/// Body: a [`MapFile`]: `{"scenario":"<name>"}`, `{"save":"<name>"}` or `"quick"`.
pub(crate) async fn post_game_load(
    extract::State(server): extract::State<ServerState>,
    Json(file): Json<MapFile>,
) -> Result<Json<ResponseCollection>, IngressClosed> {
    server.request(begin_load_request, file).await.map(Json)
}

fn begin_load_request(In((sender, file)): In<(ResponseSender, MapFile)>, mut commands: Commands) {
    let response = spawn_request::<LoadGameReport>(&mut commands, sender);
    commands.trigger(LoadGameSignal(LoadMapConfig::file(file).with_response(response)));
}

/// Names only a save in `saves/`, so remote saves never overwrite scenarios or the quicksave.
#[derive(Deserialize)]
pub(crate) struct SaveBody {
    save: MapFileName,
}

/// `POST /game/save` — saves the game to `saves/<name>.dwd` and reports why it was rejected, or
/// the written file once it is in place.
///
/// Body: [`SaveBody`]
pub(crate) async fn post_game_save(
    extract::State(server): extract::State<ServerState>,
    Json(body): Json<SaveBody>,
) -> Result<Json<ResponseCollection>, IngressClosed> {
    server.request(begin_save_request, body).await.map(Json)
}

fn begin_save_request(In((sender, body)): In<(ResponseSender, SaveBody)>, mut commands: Commands) {
    let response = spawn_request::<SaveGameReport>(&mut commands, sender);
    commands.trigger(SaveGameSignal::new(MapFile::Save(body.save)).with_response(response));
}
