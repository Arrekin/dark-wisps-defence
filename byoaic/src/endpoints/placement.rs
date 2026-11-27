//! Endpoints for placing and removing map objects.
//!
//! Each call spawns a transient [`PlacementSession`], triggers a single [`PlaceRequest`] or
//! [`RemoveRequest`], and ends the session with [`StopPlacing`]. Domains validate rules, deduct
//! costs, and reserve cells identically to player placement, using the default settings inserted
//! on `BeginPlacing`.

use axum::{Json, extract};
use bevy::prelude::*;
use serde::Deserialize;

use game_core::prelude::{AltitudeLevel, GridCoords, GridPosition, MapObject};
use grids::placement::{PlaceReport, PlaceRequest, PlacementSession, RemoveReport, RemoveRequest, StopPlacing};

use crate::{
    collection::ResponseCollection,
    forwarding::spawn_request_within,
    requests::ResponseSender,
    server::{IngressClosed, ServerState},
};

/// Placement and removal observers resolve within the dispatch frame; if no report arrives by the
/// end of that frame, no domain handled the request.
const REPORT_WINDOW_FRAMES: u32 = 0;

/// Target object and origin position for placement or removal.
#[derive(Deserialize)]
pub(crate) struct PlacementBody {
    object: MapObject,
    coords: GridCoords,
    #[serde(default)]
    level: AltitudeLevel,
}
impl PlacementBody {
    fn origin(&self) -> GridPosition {
        GridPosition { coords: self.coords, level: self.level }
    }
}

/// `POST /game/place` — places `object` with its origin at `coords` and reports [`PlaceReport`],
/// or ends with `WindowElapsed` if no domain handles the object.
///
/// Body: [`PlacementBody`], e.g. `{"object":{"Building":{"Tower":"Cannon"}},"coords":{"x":10,"y":12}}`
pub(crate) async fn post_game_place(
    extract::State(server): extract::State<ServerState>,
    Json(body): Json<PlacementBody>,
) -> Result<Json<ResponseCollection>, IngressClosed> {
    server.request(begin_place_request, body).await.map(Json)
}

fn begin_place_request(In((sender, body)): In<(ResponseSender, PlacementBody)>, mut commands: Commands) {
    let response = spawn_request_within::<PlaceReport>(&mut commands, sender, REPORT_WINDOW_FRAMES);
    let placement_session = PlacementSession::begin(&mut commands, body.object, body.origin());
    commands.trigger(PlaceRequest::new(placement_session).with_response(response));
    commands.trigger(StopPlacing::new(placement_session));
}

/// `POST /game/remove` — removes content matching `object` at `coords` and reports
/// [`RemoveReport`], or ends with `WindowElapsed` if the object does not support removal.
///
/// Body: [`PlacementBody`], e.g. `{"object":"Wall","coords":{"x":10,"y":12}}`
pub(crate) async fn post_game_remove(
    extract::State(server): extract::State<ServerState>,
    Json(body): Json<PlacementBody>,
) -> Result<Json<ResponseCollection>, IngressClosed> {
    server.request(begin_remove_request, body).await.map(Json)
}

fn begin_remove_request(In((sender, body)): In<(ResponseSender, PlacementBody)>, mut commands: Commands) {
    let response = spawn_request_within::<RemoveReport>(&mut commands, sender, REPORT_WINDOW_FRAMES);
    let placement_session = PlacementSession::begin(&mut commands, body.object, body.origin());
    commands.trigger(RemoveRequest::new(placement_session).with_response(response));
    commands.trigger(StopPlacing::new(placement_session));
}
