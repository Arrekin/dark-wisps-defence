use bevy::prelude::*;

use crate::requests::{CollectionEndReason, FrameWindow, RequestFulfilled, ResponseChannel};

pub(crate) fn advance_request_windows(
    mut commands: Commands,
    mut windows: Query<(Entity, &mut FrameWindow), Without<RequestFulfilled>>,
) {
    for (entity, mut window) in windows.iter_mut() {
        if window.frames_remaining == 0 {
            commands.entity(entity).insert_if_new(RequestFulfilled { end_reason: CollectionEndReason::WindowElapsed });
        } else {
            window.frames_remaining -= 1;
        }
    }
}

/// Runs after every `RequestFulfilled` insert of the frame is applied, so reports triggered by
/// `On<Add<RequestFulfilled>>` observers are already in the channel.
pub(crate) fn cleanup_fulfilled_requests(
    mut commands: Commands,
    requests: Query<(Entity, &ResponseChannel, &RequestFulfilled)>,
) {
    for (entity, channel, fulfilled) in requests.iter() {
        channel.finish(fulfilled.end_reason);
        commands.entity(entity).despawn();
    }
}
