use axum::{Json, extract};
use bevy::prelude::*;

use persistence::{GameMapList, MapListEntry};

use crate::server::{IngressClosed, ServerState};

/// `GET /game/maps` — every map in `maps/` with its header.
pub(crate) async fn get_game_maps(
    extract::State(server): extract::State<ServerState>,
) -> Result<Json<Vec<MapListEntry>>, IngressClosed> {
    server.query(read_game_maps).await.map(Json)
}

fn read_game_maps(mut map_list: ResMut<GameMapList>) -> Vec<MapListEntry> {
    map_list.entries().to_vec()
}
