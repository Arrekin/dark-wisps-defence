# BYOAIC (Bring Your Own AI Companion)

The game starts a local HTTP server at `http://127.0.0.1:7461` on startup. Keep the game running while making requests. If the server cannot start or bind that address, it logs the error and the game continues without remote control.

## Available endpoints

- [`GET /game/status`](../byoaic/src/endpoints/status.rs) — game, admin, UI and map-loading states, current map, game clock.
- [`GET /game/scenarios`](../byoaic/src/endpoints/map_files.rs) — scenarios in `maps/` with their headers; each `file` is the body that loads it.
- [`GET /game/saves`](../byoaic/src/endpoints/map_files.rs) — saves in `saves/`, each as the body that loads it.
- [`POST /game/load`](../byoaic/src/endpoints/map_files.rs) — Body: a map file, `{"scenario":"<name>"}`, `{"save":"<name>"}` or `"quick"`; names are file names without `.dwd`.
- [`POST /game/save`](../byoaic/src/endpoints/map_files.rs) — Body: `{"save":"<name>"}`; writes `saves/<name>.dwd` and reports once the file is in place. Never writes maps or the quicksave.
- [`POST /game/pause`](../byoaic/src/endpoints/pause.rs) — Body: `{"paused":<bool>}`
- [`POST /game/place`](../byoaic/src/endpoints/placement.rs) — Body: `{"object":<MapObject>,"coords":{"x":<i32>,"y":<i32>},"level":<AltitudeLevel>}` (`level` optional, defaults to `OnGround`); e.g. `{"object":{"Building":{"Tower":"Cannon"}},"coords":{"x":10,"y":12}}`. Places using default object settings (enforcing placement rules, costs, and cell reservations) and reports `PlaceReport` (`Placed`, `Invalid`, `CannotAfford`, `InvalidSession`). Ends with `WindowElapsed` if no domain handles the object.
- [`POST /game/remove`](../byoaic/src/endpoints/placement.rs) — Body: same as `/game/place`, e.g. `{"object":"Wall","coords":{"x":10,"y":12}}`. Reports `RemoveReport` (`Removed`, `NothingToRemove`, `InvalidSession`), or ends with `WindowElapsed` if the object does not support removal.
- [`POST /game/screenshot`](../byoaic/src/endpoints/screenshot.rs) — captures the window into `screenshots/`; reports the absolute path.
- [`POST /game/close`](../byoaic/src/endpoints/close.rs) — request game shutdown.

Endpoint details live beside their handlers. Routes are registered in [`byoaic/src/endpoints/mod.rs`](../byoaic/src/endpoints/mod.rs).
