# BYOAIC (Bring Your Own AI Companion)

The game starts a local HTTP server at `http://127.0.0.1:7461` on startup. Keep the game running while making requests. If the server cannot start or bind that address, it logs the error and the game continues without remote control.

## Available endpoints

- [`GET /game/status`](../byoaic/src/endpoints/status.rs) — game, admin, UI and map-loading states, current map, game clock.
- [`GET /game/maps`](../byoaic/src/endpoints/maps.rs) — maps in `maps/` with their headers; `file_name` is the load key.
- [`POST /game/load-map`](../byoaic/src/endpoints/load_map.rs) — Body: `{"file_name":"<file name without .dwd>"}`
- [`POST /game/pause`](../byoaic/src/endpoints/pause.rs) — Body: `{"paused":<bool>}`
- [`POST /game/screenshot`](../byoaic/src/endpoints/screenshot.rs) — captures the window into `screenshots/`; reports the absolute path.
- [`POST /game/close`](../byoaic/src/endpoints/close.rs) — request game shutdown.

Endpoint details live beside their handlers. Routes are registered in [`byoaic/src/endpoints/mod.rs`](../byoaic/src/endpoints/mod.rs).
