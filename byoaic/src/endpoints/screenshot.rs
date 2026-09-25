use std::{
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{Json, extract};
use bevy::{
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured},
};
use serde::Serialize;

use logging::prelude::*;

use crate::{
    collection::ResponseCollection,
    forwarding::spawn_request,
    requests::ResponseSender,
    server::{IngressClosed, ServerState},
};

/// Relative to the working directory; gitignored.
const SCREENSHOTS_DIRECTORY: &str = "screenshots";

#[derive(EntityEvent, Serialize)]
struct ScreenshotReport {
    #[serde(skip)]
    entity: Entity,
    result: ScreenshotResult,
}

#[derive(Serialize)]
enum ScreenshotResult {
    /// `path` is absolute.
    Saved { path: String, width: u32, height: u32 },
    /// Saving the capture failed; `error` names the failing step and its cause.
    Failed { error: String },
    /// Another capture is in flight. Bevy drops a second capture of the same window within a frame
    /// without ever reporting it.
    AlreadyCapturing,
}

/// `POST /game/screenshot` — captures the primary window into `screenshots/` and reports the
/// file's absolute path. The capture completes after rendering, a frame or more after the request.
pub(crate) async fn post_game_screenshot(
    extract::State(server): extract::State<ServerState>,
) -> Result<Json<ResponseCollection>, IngressClosed> {
    server.request(begin_screenshot_request, ()).await.map(Json)
}

/// Rejects the capture while another is in flight; otherwise spawns the Bevy screenshot entity,
/// whose capture observer reports back. Bevy despawns that entity after the capture, so the
/// request lives on its own entity.
fn begin_screenshot_request(
    In((sender, ())): In<(ResponseSender, ())>,
    mut commands: Commands,
    screenshots: Query<(), With<Screenshot>>,
) {
    let response = spawn_request::<ScreenshotReport>(&mut commands, sender);
    if !screenshots.is_empty() {
        response.report(&mut commands, |entity| ScreenshotReport { entity, result: ScreenshotResult::AlreadyCapturing });
        return;
    }
    commands
        .spawn(Screenshot::primary_window())
        .observe(move |trigger: On<ScreenshotCaptured>, mut commands: Commands| {
            let result = save_capture(&trigger.event().image);
            response.report(&mut commands, |entity| ScreenshotReport { entity, result });
        });
}

fn save_capture(image: &Image) -> ScreenshotResult {
    match save_png(image) {
        Ok(path) => ScreenshotResult::Saved {
            path: path.display().to_string(),
            width: image.width(),
            height: image.height(),
        },
        Err(error) => {
            Log::error().dev().tag(Tag::Byoaic).message(format!("Screenshot not saved: {error}"));
            ScreenshotResult::Failed { error }
        }
    }
}

/// Writes `image` as `screenshots/screenshot-<unix ms>.png` and returns the absolute path.
fn save_png(image: &Image) -> Result<PathBuf, String> {
    let directory = std::env::current_dir()
        .map_err(|error| format!("Failed to read the working directory: {error}"))?
        .join(SCREENSHOTS_DIRECTORY);
    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("Failed to create '{}': {error}", directory.display()))?;
    let captured_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("System clock is before 1970: {error}"))?
        .as_millis();
    let path = directory.join(format!("screenshot-{captured_at}.png"));

    // Drops the alpha channel: with HDR enabled it holds brightness, not transparency.
    image
        .clone()
        .try_into_dynamic()
        .map_err(|error| format!("Screenshot texture format is not convertible: {error}"))?
        .to_rgb8()
        .save(&path)
        .map_err(|error| format!("Failed to write '{}': {error}", path.display()))?;
    Ok(path)
}
