# Grid Object Placer

The grid object placer lets players place and remove map objects on the grid. The tool lives in [`hud_internal/src/grid_placer.rs`](../../hud_internal/src/grid_placer.rs); shared contract types live in [`grids/src/placement.rs`](../../grids/src/placement.rs).

Responsibilities are split between the tool and each object's domain:

- **The placer tool** tracks the cursor, manages the active `PlacementSession` entity, renders the ghost preview and cell-validity overlay, and translates mouse input into `PlaceRequest` and `RemoveRequest` events.
- **Each object's domain** owns its placement rules, session settings, and optional UI controls. It publishes `PlacementFeedback` on the session for display, re-validates live world state on `PlaceRequest`, and checks for removable content on `RemoveRequest`.

`PlacementFeedback` is strictly visual—the placer never uses it to block input, and request handlers never trust stored session feedback, which may be stale or absent.

## Entity Hierarchy & Session State

At startup, the placer spawns a persistent `GridObjectPlacer` entity with a persistent `PlacementSession` child. While active, it also parents a transient `GridPlacerGhost` child:

```text
GridObjectPlacer (entity)
 │  components: Transform (at area.world_center()), Mesh2d, MeshMaterial2d<GridPlacerMaterial>
 │
 ├── PlacementSession (persistent child entity)
 │      components: PlacementObject(MapObject), PlacementModes,
 │                  GridArea (inserted by domain on BeginPlacing),
 │                  PlacementFeedback (inserted by domain validation),
 │                  ReplaceExisting (optional marker),
 │                  <domain settings> (e.g. WallStyleKey, VolumeShape, ExcavationShape)
 │
 └── GridPlacerGhost (transient child entity, rebuilt on GridPlacerGhostRebuildRequest)
        components: GridImprint + domain-rendered world preview (via ObjectFaceRequest::ghost)
```

Tower range and energy supply overlays also observe `GridPlacerChanged` and read `(&PlacementObject, &GridArea)` from `PlacementSession` to preview coverage at the cursor.

## Lifecycle & Event Flow

### 1. Starting or switching a session (`begin_placement`)

UI buttons and hotkeys request placement by calling `grid_object_placer_request.set(map_object)`. In `Update` (ordered `.before(PlacementFeedbackSystems)`), `begin_placement` consumes `GridObjectPlacerRequest` and runs:

```text
GridObjectPlacerRequest::take()
  ├─ if already in UiInteraction::PlaceGridObject → trigger StopPlacing (placer despawns PlacementControls)
  ├─ session.retain::<(PlacementSession, ChildOf)>()
  │    .insert(PlacementObject(map_object))        → also inserts default PlacementModes via #[require]
  ├─ trigger BeginPlacing { object, origin }       → domain inserts GridArea, optional PlacementModes,
  │                                                  default settings on session, & spawns UI controls
  ├─ trigger GridPlacerGhostRebuildRequest         → despawns old ghost, spawns (GridPlacerGhost, area.imprint),
  │                                                  triggers ObjectFaceRequest::ghost(ghost, map_object)
  ├─ trigger GridPlacerChanged                     → moves/resizes overlay quad, redraws it, notifies overlays
  └─ set UiInteraction::PlaceGridObject
```

**Ordering constraint:** `BeginPlacing` must flush before `GridPlacerGhostRebuildRequest` and `GridPlacerChanged` because the domain's `BeginPlacing` observer inserts `GridArea` and any domain settings (such as `WallStyleKey` and `VolumeShape`) that the ghost and overlay read from `PlacementSession`.

### 2. Updating area and settings

While in `UiInteraction::PlaceGridObject`, three paths mutate the session without ending it:

- **Cursor movement (`follow_mouse` in `PreUpdate`):** When the cursor enters a different grid cell, the placer updates `GridArea` on the session (preserving altitude level and imprint) and triggers `GridPlacerChanged`.
- **Imprint change:** A control that changes the imprint after the session has started (e.g. quantum field size buttons) inserts a new `GridArea` at the session's origin and triggers both `GridPlacerGhostRebuildRequest` and `GridPlacerChanged`.
- **Domain setting change:** Controls that affect the ghost's appearance (such as wall style swatches and ramp/opening selectors) replace their component on `PlacementSession` and trigger `GridPlacerGhostRebuildRequest`, which both rebuilds the ghost and refreshes those controls' selection frames. Settings that only affect validation—such as `ReplaceToggleButton`, which toggles `ReplaceExisting`—update the session component and their own border directly without rebuilding the ghost.

### 3. Per-frame validation (`PlacementFeedbackSystems`)

`PlacementFeedbackSystems` re-evaluates the session every frame in `Update` while in `UiInteraction::PlaceGridObject`, keeping feedback current with setting, stock, and world changes even when the cursor is stationary.

Each domain's `update_feedback` system filters for its `MapObject`, evaluates current world state, and writes `PlacementFeedback` with `insert_if_neq`. Updates to `PlacementFeedback` trigger a feedback redraw only when its value changes (`GridPlacerChanged` also redraws the overlay when the area moves or resizes).

### 4. Mouse input (`handle_placement_click`)

Runs in `ObstacleChangeSystems` during `Update` and ignores clicks over UI. Input behavior is read from `PlacementModes { place, remove }` on `PlacementSession` (defaulting to `OnRelease` unless overridden by the domain on `BeginPlacing`):

| `PlacementMode` | Left click (`PlaceRequest(MapObject)`) | Right click (`RemoveRequest(MapObject)`) |
|---|---|---|
| `OnRelease` (default) | Fires once on button release. | Fires once on button release. |
| `OnPress` | Fires while held, at most once per origin cell per press (including rejected attempts); resets when the button is released. | Fires every frame while held. |

### 5. Leaving placement (`hide_placer`)

On `OnExit(UiInteraction::PlaceGridObject)`, the placer hides itself and triggers `StopPlacing`, on which it despawns every entity marked `PlacementControls`.

## Validation, Reservations, and Domain Rules

### `PlacementFeedback`

```rust
pub struct PlacementFeedback {
    pub validity: PlacementValidity,                  // Valid, ValidUnpowered, Replaces, or Invalid
    pub is_affordable: bool,                          // True when stock covers cost (or in AdminMode / free)
    pub highlights: Vec<(GridCoords, CellHighlight)>, // Negative, Positive, or Removed
}
```

Combinators in `grids::placement` build feedback across an area:
- **`PlacementFeedback::every_cell(area, reserved_coords, accepts)`** — Marks cells failing `accepts` with `CellHighlight::Negative`. Returns `Valid` if every cell passes and none is reserved; otherwise `Invalid`.
- **`.replacing(area, holds_content)`** — When `Valid`, checks whether any cell holds replaceable content of the same kind; if so, changes validity to `Replaces` and replaces highlights with `CellHighlight::Removed` for those cells.
- **`.with_validity(...)` / `.with_highlights(...)` / `.with_affordability(...)`** — Overrides individual fields.

### `ReservedCoords`

`Builder*` spawns do not imprint onto `ObstacleGrid` until commands flush. Handlers record claimed cells in `ReservedCoords` so same-frame placement requests and `PlacementFeedbackSystems` reject those cells before the flush; reservations clear in the next frame's `First` schedule.

- **Walls, buildings, dark ore, and quantum fields** check `ReservedCoords` via `every_cell` and call `reserved_coords.reserve(area)` on placement.
- **Excavations** check `ReservedCoords` via `every_cell` but mutate `ObstacleGrid` immediately instead of reserving.
- **Wisps** validate against `ObstacleGrid` and `WispsGrid` directly.

### Domain `*PlacementRules` (`SystemParam`)

Each domain groups validation and request handling in a `*PlacementRules` `SystemParam`:

| Rules | File | Notes |
|---|---|---|
| `WallPlacementRules` | `map_objects_internal/src/walls.rs` | Supports `ReplaceExisting` and right-click removal (clearing `OnElevation` occupants). |
| `ExcavationPlacementRules` | `map_objects_internal/src/excavations.rs` | Mutates `ObstacleGrid` directly and requests emissions recalculation; digging pits/ramps or filling excavations clears occupants, while tunnels preserve structures above. |
| `DarkOrePlacementRules` | `map_objects_internal/src/dark_ore.rs` | Places/removes dark ore on unreserved ground. |
| `QuantumFieldPlacementRules` | `map_objects_internal/src/quantum_field.rs` | Supports a variable square imprint, resized by its size buttons. |
| `WispPlacementRules` | `wisps_internal/src/spawning.rs` | Checks `ObstacleGrid` support and `WispsGrid` occupancy. |
| `BuildingPlacementRules` | `buildings_internal/src/common.rs` | Shared ground, power (`ValidUnpowered`), cost, and site-claiming logic for buildings; used directly by towers, exploration center, and forge. |
| `MiningComplexPlacementRules` | `buildings_internal/src/mining_complex.rs` | Wraps `BuildingPlacementRules`; requires at least one dark ore cell (`CellHighlight::Positive`). |
| `EnergyRelayPlacementRules` | `buildings_internal/src/energy_relay.rs` | Wraps `BuildingPlacementRules`; checks whether the relay's supply range reaches powered cells. |
| `MainBasePlacementRules` | `buildings_internal/src/main_base.rs` | Relocates the existing `MainBase`, allowing overlap with its current area. |

Rule structs expose:
- `feedback(&self) -> PlacementFeedback` — pure validation used by both `update_feedback` and `on_place_request_do_so`.
- `on_place_request_do_so` — matches the `MapObject`, recomputes `feedback()`, and applies placement when not `Invalid`. Session components needed only on placement (such as `(&WallStyleKey, &VolumeShape)`) are queried in that observer rather than on the shared `SystemParam`.
- `on_remove_request_do_so` (where supported) — matches the `MapObject`, checks whether removable content of that kind exists at the target area, and removes it without calling `feedback()`.

For buildings with resource costs, `BuildingPlacementRules::claim(&mut self, validity)` rejects `Invalid`, deducts the Almanach cost from `Stock` (skipped when `AdminMode` is enabled), and reserves `area` in `ReservedCoords`. Each caller passes the validity of its own `feedback()`, computed from current state: `rules.claim(rules.feedback().validity)`.

## Overlay Shader Encoding

The overlay quad is rendered by [`assets/shaders/grids/grid_placer.wesl`](../../assets/shaders/grids/grid_placer.wesl) using `GridPlacerUniform`, which must stay layout-compatible between Rust and WESL:

- **`cell_data: UVec4`** packs up to 64 cells of `area.bounding_box()` at 2 bits per cell: `0` = outside imprint (transparent), `1` = plain imprint cell, `2` = `CellHighlight::Negative`, `3` = framed (`CellHighlight::Positive` or `CellHighlight::Removed`).
- **Outline color** follows `PlacementValidity`: green (`Valid`), amber (`ValidUnpowered`), cyan (`Replaces`), or red (`Invalid`).
- **Cell fills & frames:** `Invalid` draws diagonal stripes across the imprint, rendered thicker and brighter on `Negative` cells. When valid (`Valid`, `ValidUnpowered`, or `Replaces`), framed cells (`Positive` or `Removed`) draw an inset border.
- **Affordability:** When `!is_affordable` on a non-`Invalid` site, the outline alternates between grey and the validity color (mostly grey per period). `Invalid` outlines remain solid red regardless of affordability.

## Adding a Placeable Object

1. **Identifier:** Add a variant to `MapObject` (`game_core/src/types.rs`).
2. **Session initialization (`BeginPlacing`):** Observe `BeginPlacing { object, origin }` to insert `GridArea::new(origin, imprint)` onto `PlacementSession` (buildings share `on_begin_placing_start_building_session`), override `PlacementModes` if needed (e.g. `PlacementModes::on_press()`), and initialize any domain settings or UI controls. Mark spawned controls with `PlacementControls` so the placer despawns them on `StopPlacing`.
3. **Rules & observers:** Implement a `*PlacementRules` `SystemParam` (or reuse `BuildingPlacementRules`), add `update_feedback` to `PlacementFeedbackSystems`, and register `on_place_request_do_so` (plus `on_remove_request_do_so` if removable).
4. **Faces:** Handle `ObjectFaceRequest` for `FaceSurface::World` (the ghost preview, reading any session settings from `PlacementSession`) and `FaceSurface::Ui` (menus/swatches).
5. **Control updates (if configurable):** When a control changes the imprint, insert the new `GridArea` on the session and trigger `GridPlacerGhostRebuildRequest` and `GridPlacerChanged`. When a control changes ghost-visible session components, trigger `GridPlacerGhostRebuildRequest`.
6. **Menu & tooltip:** Register any Almanach metadata, handle `MapObjectTooltipRequest`, and add a side-menu entry or hotkey if player-accessible.

