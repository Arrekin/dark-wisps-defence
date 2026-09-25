//! # BYOAIC — Bring Your Own AI Companion
//!
//! Embedded HTTP server through which external callers steer the running game.
//!
//! ## How a request flows
//!
//! HTTP handlers never touch the ECS world directly. They submit commands onto the ingress
//! channel, and `apply_ingress_queues` drains that channel every `PreUpdate`, so commands execute
//! inside the frame.
//!
//! Action endpoints call `ServerState::request` with a one-shot system defined next to the
//! endpoint. The system receives the response sender and passes it to `spawn_request`, which
//! spawns a *request entity* holding the `ResponseChannel` and a `forward_and_finish` observer for
//! the action's report. The system hands the returned `ResponseRequest` to the work that decides
//! the outcome, usually by triggering a domain event that carries it.
//!
//! That work calls `response.report(...)` once, with the action's result, which triggers a typed
//! report event on the request entity. `forward_and_finish` serializes it into the response
//! channel and inserts `RequestFulfilled`. `cleanup_fulfilled_requests` then sends the end reason,
//! closes the channel and despawns the entity. `ServerState::request` returns everything gathered
//! as the JSON response.
//!
//! Query endpoints skip the request entity: `ServerState::query` runs a one-shot system and
//! returns its output.

mod collection;
mod endpoints;
mod forwarding;
mod ingress;
mod lifecycle;
mod requests;
mod server;

use bevy::prelude::*;

pub struct ByoaicPlugin;
impl Plugin for ByoaicPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Startup, server::start_server)
            .add_systems(PreUpdate, ingress::apply_ingress_queues)
            .add_systems(Last, (
                lifecycle::advance_request_windows,
                lifecycle::cleanup_fulfilled_requests,
            ).chain());
    }
}
