use serde::Serialize;

use crate::requests::{CollectionEndReason, ReportEnvelope, ResponseItem};

#[derive(Serialize)]
pub(crate) struct ResponseCollection {
    end_reason: CollectionEndReason,
    reports: Vec<ReportEnvelope>,
}

/// Receives until the channel is closed and drained.
pub(crate) async fn collect(receiver: async_channel::Receiver<ResponseItem>) -> ResponseCollection {
    let mut end_reason = None;
    let mut reports = Vec::new();
    while let Ok(item) = receiver.recv().await {
        match item {
            ResponseItem::Report(report) => reports.push(report),
            ResponseItem::End(reason) => end_reason = Some(reason),
        }
    }
    ResponseCollection {
        end_reason: end_reason.unwrap_or(CollectionEndReason::Interrupted),
        reports,
    }
}
