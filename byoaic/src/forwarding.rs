use bevy::prelude::*;
use serde::Serialize;

use game_core::prelude::*;

use crate::requests::{CollectionEndReason, RequestFulfilled, ResponseChannel, ResponseSender};

/// Spawns a request entity that forwards the first `E` reported to it and then ends the request.
/// Pass the returned `ResponseRequest` to the work that produces `E`.
pub(crate) fn spawn_request<E: EntityEvent + Serialize>(commands: &mut Commands, sender: ResponseSender) -> ResponseRequest {
    let request_entity = commands
        .spawn(ResponseChannel::new(sender))
        .observe(forward_and_finish::<E>)
        .id();
    ResponseRequest::new(request_entity)
}

/// Request-entity observer: forwards each `E` into the request's response channel.
#[expect(dead_code, reason = "request helper with no endpoint attaching it")]
pub(crate) fn forward<E: EntityEvent + Serialize>(trigger: On<E>, channels: Query<&ResponseChannel>) {
    let event = trigger.event();
    let Ok(channel) = channels.get(event.event_target()) else { return; };
    channel.report(event);
}

/// Request-entity observer: forwards `E` into the request's response channel, then ends
/// collection for the request with `CollectionEndReason::Completed`.
pub(crate) fn forward_and_finish<E: EntityEvent + Serialize>(
    trigger: On<E>,
    mut commands: Commands,
    channels: Query<&ResponseChannel>,
) {
    let event = trigger.event();
    let request_entity = event.event_target();
    let Ok(channel) = channels.get(request_entity) else { return; };
    channel.report(event);
    commands.entity(request_entity).insert_if_new(RequestFulfilled { end_reason: CollectionEndReason::Completed });
}
