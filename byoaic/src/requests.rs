use bevy::prelude::*;
use serde::Serialize;

/// Sending half of a request's response channel; the HTTP handler holds the receiver.
pub(crate) type ResponseSender = async_channel::Sender<ResponseItem>;

/// Sender half of a request's response channel. Lives on the request entity.
#[derive(Component)]
pub(crate) struct ResponseChannel {
    sender: ResponseSender,
}
impl ResponseChannel {
    pub(crate) fn new(sender: ResponseSender) -> Self {
        Self { sender }
    }

    /// Send failures mean the caller stopped listening; the request still ends through its policy.
    pub(crate) fn report<T: Serialize>(&self, report: &T) {
        let _ = self.sender.try_send(ResponseItem::Report(ReportEnvelope::of(report)));
    }

    /// Sends the end reason and closes the channel. The receiver still drains buffered items.
    pub(crate) fn finish(&self, end_reason: CollectionEndReason) {
        let _ = self.sender.try_send(ResponseItem::End(end_reason));
        self.sender.close();
    }
}

pub(crate) enum ResponseItem {
    Report(ReportEnvelope),
    End(CollectionEndReason),
}

#[derive(Serialize)]
pub(crate) struct ReportEnvelope {
    kind: String,
    payload: serde_json::Value,
}
impl ReportEnvelope {
    fn of<T: Serialize>(report: &T) -> Self {
        Self {
            kind: ShortName::of::<T>().to_string(),
            payload: serde_json::to_value(report).expect("Report types serialize to JSON"),
        }
    }
}

/// Why collection for a request ended.
#[derive(Clone, Copy, Debug, Serialize)]
pub(crate) enum CollectionEndReason {
    /// A report the endpoint treats as final arrived (`forward_and_finish`).
    Completed,
    WindowElapsed,
    /// The channel closed without an end reason. Set by the HTTP side only.
    Interrupted,
}

/// Collection for this request is over. The first reason inserted wins.
#[derive(Component)]
pub(crate) struct RequestFulfilled {
    pub(crate) end_reason: CollectionEndReason,
}

/// Fulfills the request once `frames_remaining` reaches zero at the end of a frame.
#[derive(Component)]
pub(crate) struct FrameWindow {
    pub(crate) frames_remaining: u32,
}
impl FrameWindow {
    #[expect(dead_code, reason = "fulfillment policy with no endpoint attaching it")]
    pub(crate) fn new(frames_remaining: u32) -> Self {
        Self { frames_remaining }
    }
}
