# Grid Object Placer

The grid object placer lets players place and remove map objects on the grid. The tool lives in [`hud_internal/src/grid_placer.rs`](../../hud_internal/src/grid_placer.rs); shared contract types live in [`grids/src/placement.rs`](../../grids/src/placement.rs).

A placement session is an entity holding `PlacementSession`, `PlacementObject`, a target `GridArea`, and any domain-specific settings. Lifecycle and action events (`BeginPlacing`, `StopPlacing`, `PlaceRequest`, `RemoveRequest`) are `EntityEvent`s targeting a session entity, allowing multiple independent sessions to coexist. The player's persistent session carries `UiPlacementSession`; programmatic callers (such as BYOAIC) spawn transient sessions via `PlacementSession::begin` (see [Programmatic Sessions & Reports](#programmatic-sessions--reports)).

Responsibilities are split between the tool and each object's domain:

- **The placer tool** tracks the cursor, owns the `UiPlacementSession` entity, renders the ghost preview and cell-validity overlay, and translates mouse input into `PlaceRequest` and `RemoveRequest` events on `UiPlacementSession`.
- **Each object's domain** owns its session initialization, placement rules, and optional UI controls. It publishes `PlacementFeedback` on `UiPlacementSession` for visual preview, validates live world state on `PlaceRequest` and `RemoveRequest`, and reports the outcome via `PlaceReport` or `RemoveReport`.

`PlacementFeedback` is strictly visual—the placer never uses it to gate input, and request handlers always re-validate against live world state rather than trusting stored feedback (which may be stale or absent on non-UI sessions).

## Entity Hierarchy & Session State

At startup, the placer spawns a persistent `GridObjectPlacer` entity with a persistent `UiPlacementSession` child. While active, it also parents a transient `GridPlacerGhost` child:

```text
GridObjectPlacer (entity)
 │  components: Transform (at area.world_center()), Mesh2d, MeshMaterial2d<GridPlacerMaterial>
 │
 ├── UiPlacementSession + PlacementSession (persistent child entity)
 │      components: PlacementObject(MapObject), PlacementModes,
 │                  GridArea (inserted by domain on BeginPlacing),
 │                  PlacementFeedback (inserted by domain validation),
 │                  ReplaceExisting (optional marker),
 │                  <domain settings> (e.g. WallStyleKey, StructureShape, ExcavationShape)
 │
 └── GridPlacerGhost (transient child entity, rebuilt on GridPlacerGhostRebuildRequest)
        components: GridImprint + domain-rendered world preview (via ObjectFaceRequest::ghost)
```

Domains write placement overlay previews into `EnergySupplyOverlayConfig::secondary_mode` and `TowersRangeOverlayConfig::secondary_mode`; the placer clears both when `UiPlacementSession` stops placing.

## Lifecycle & Event Flow

### 1. Starting or switching placement (`begin_placement`)

UI buttons and hotkeys request placement by calling `grid_object_placer_request.set(map_object)`. In `Update` (ordered `.before(PlacementFeedbackSystems)`), `begin_placement` processes a pending `GridObjectPlacerRequest`. If not yet in `UiInteraction::PlaceGridObject`, it queues that state transition and retains the request until the next frame.

```text
GridObjectPlacerRequest::take()
  ├─ trigger StopPlacing(ui_session)          → despawns PlacementControls, strips domain components from
  │                                             ui_session, and clears overlay secondary modes (no-op if idle)
  ├─ ui_session.insert(PlacementObject(..))   → also inserts default PlacementModes via #[require]
  ├─ trigger BeginPlacing(ui_session, origin) → domain inserts GridArea, optional PlacementModes, and
  │                                             default settings, then spawns UI controls if Has<UiPlacementSession>
  ├─ trigger GridPlacerGhostRebuildRequest    → despawns old ghost, spawns (GridPlacerGhost, area.imprint),
  │                                             triggers ObjectFaceRequest::ghost(ghost, map_object)
  └─ trigger GridPlacerChanged                → repositions/resizes overlay quad, redraws it; domains
                                                update supply previews and overlay secondary modes
```

**Ordering constraints:**
- `BeginPlacing` must flush before `GridPlacerGhostRebuildRequest` and `GridPlacerChanged` because the domain's `BeginPlacing` observer inserts `GridArea` and domain settings (such as `WallStyleKey` and `StructureShape`) that ghost and overlay observers read.
- Each domain initializes session components and spawns its UI controls inside a single `BeginPlacing` observer (in that order) so the controls' `On<Add>` observer can immediately read those settings from `UiPlacementSession`.

### 2. Updating area and settings

While in `UiInteraction::PlaceGridObject`, three paths mutate `UiPlacementSession` in place:

- **Cursor movement (`follow_mouse` in `PreUpdate`):** When the cursor enters a new grid cell, the placer updates `GridArea` on `UiPlacementSession` (preserving altitude level and imprint) and triggers `GridPlacerChanged`.
- **Imprint change:** Controls that resize the imprint (such as quantum field size buttons) insert a new `GridArea` at the session's origin and trigger both `GridPlacerGhostRebuildRequest` and `GridPlacerChanged`.
- **Domain setting change:** Controls that affect ghost visuals (such as wall style swatches and ramp/hollowed-side selectors) update their component on `UiPlacementSession` and trigger `GridPlacerGhostRebuildRequest`, which rebuilds the ghost and refreshes control selection frames. Settings that only affect validation (such as `ReplaceToggleButton`, which toggles `ReplaceExisting`) update the session component and their own border directly without rebuilding the ghost.

### 3. Per-frame validation (`PlacementFeedbackSystems`)

`PlacementFeedbackSystems` runs every frame in `Update` while in `UiInteraction::PlaceGridObject`, keeping feedback in sync with setting, stock, and world changes even when the cursor is stationary.

Each domain's `update_feedback` system reads `UiPlacementSession`, filters for its `MapObject`, evaluates current world state, and writes `PlacementFeedback` via `insert_if_neq`. Non-UI sessions do not compute or store `PlacementFeedback`. Because `PlacementFeedback` is immutable, `insert_if_neq` triggers an overlay redraw (`On<Insert<PlacementFeedback>>`) only when feedback changes (`GridPlacerChanged` also redraws the overlay when the area moves or resizes).

### 4. Mouse input (`handle_placement_click`)

Runs in `ObstacleChangeSystems` during `Update` and ignores clicks over UI. Input behavior is read from `PlacementModes { place, remove }` on `UiPlacementSession` (defaulting to `OnRelease` unless overridden by the domain on `BeginPlacing`):

| `PlacementMode` | Left click (`PlaceRequest`) | Right click (`RemoveRequest`) |
|---|---|---|
| `OnRelease` (default) | Fires once on button release. | Fires once on button release. |
| `OnPress` | Fires while held, at most once per origin cell per press (including rejected attempts); resets when the button is released. | Fires every frame while held. |

### 5. Ending placement (`StopPlacing` and `hide_placer`)

Exiting `UiInteraction::PlaceGridObject` hides the placer and triggers `StopPlacing` on `UiPlacementSession`. Switching to another placeable object also triggers `StopPlacing` first so every session starts clean.

`on_stop_placing_end_placement_session` handles `StopPlacing` for all sessions:
- **For `UiPlacementSession`:** Despawns all `PlacementControls` entities, strips domain components from the session via `retain::<(UiPlacementSession, PlacementSession, ChildOf)>()`, and clears overlay secondary modes. Other `StopPlacing` observers still see the session's components intact before commands flush. Any session-attached data (such as `EnergySupplyPreviewOperations`) is automatically removed by `retain`.
- **For non-UI sessions:** Despawns the session entity.

## Programmatic Sessions & Reports

`PlacementSession::begin(commands, object, origin)` spawns a standalone `PlacementSession` entity (without `UiPlacementSession`) and triggers `BeginPlacing` on it. Domains initialize `GridArea` and default settings identically to the UI session, but skip UI controls, ghost rendering, and per-frame feedback. The caller triggers `PlaceRequest` or `RemoveRequest` on the session and finishes with `StopPlacing`, which despawns it. [BYOAIC](../BYOAIC.md) uses this pattern (`begin` → request → `StopPlacing`).

`PlaceRequest` and `RemoveRequest` carry a `ResponseRequest`. The handling domain observer reports the outcome once via `trigger.report(&mut commands, outcome)`:

- `PlaceReport` (`PlaceResult`): `Placed`, `Invalid` (site rejected by placement rules), `CannotAfford` (insufficient resources), or `InvalidSession` (required session components missing).
- `RemoveReport` (`RemoveResult`): `Removed`, `NothingToRemove`, or `InvalidSession`.

If no domain handles the request (for example, `RemoveRequest` on an object that does not support removal), no report is sent.

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

Each domain encapsulates validation and execution in a `*PlacementRules` `SystemParam` that borrows the required world state (grids, stock, reservations) without querying the placement session directly:

| Rules | File | Notes |
|---|---|---|
| `WallPlacementRules` | `map_objects_internal/src/walls.rs` | Supports `ReplaceExisting` and right-click removal (clearing `OnElevation` occupants). |
| `ExcavationPlacementRules` | `map_objects_internal/src/excavations.rs` | Mutates `ObstacleGrid` directly and requests emissions recalculation; digging pits/ramps or filling excavations clears occupants, while flat excavations preserve the ground floor and structures above. |
| `DarkOrePlacementRules` | `map_objects_internal/src/dark_ore.rs` | Places and removes dark ore on unreserved open ground. |
| `QuantumFieldPlacementRules` | `map_objects_internal/src/quantum_field.rs` | Supports a variable square imprint resized via UI controls. |
| `WispPlacementRules` | `wisps_internal/src/spawning.rs` | Validates against `ObstacleGrid` support and `WispsGrid` occupancy. |
| `BuildingPlacementRules` | `buildings_internal/src/common.rs` | Shared ground, power (`ValidUnpowered`), cost, and site-claiming logic for buildings; used directly by towers, exploration center, and forge. |
| `MiningComplexPlacementRules` | `buildings_internal/src/mining_complex.rs` | Wraps `BuildingPlacementRules`; requires at least one dark ore cell (`CellHighlight::Positive`). |
| `EnergyRelayPlacementRules` | `buildings_internal/src/energy_relay.rs` | Wraps `BuildingPlacementRules`; checks whether the relay's supply range reaches powered cells. |
| `MainBasePlacementRules` | `buildings_internal/src/main_base.rs` | Relocates the existing `MainBase`, allowing overlap with its current area. |

Rule structs expose:
- `feedback(..) -> PlacementFeedback` — pure validation taking `area` and any domain parameters (such as shape or `replace`), shared by `update_feedback` and `place`.
- `place(..) -> Result<(), PlaceResult>` and (where supported) `remove(..) -> Result<(), RemoveResult>` — validate live world state and apply the operation, or return the rejection reason as `Err`.
- `on_place_request_do_so` / `on_remove_request_do_so` — query the target `PlacementSession`, match its `MapObject`, map missing session components to `Err(InvalidSession)`, delegate to `place` or `remove`, and send the report via `trigger.report(..)`.

For buildings with resource costs, `BuildingPlacementRules::claim(building_type, area, validity, make_builder)` rejects `Invalid`, deducts the Almanach cost from `Stock` (unless `AdminMode` is enabled, returning `Err(CannotAfford)` if insufficient), reserves `area` in `ReservedCoords`, and spawns the builder at `area.origin`. Buildings with custom validation pass their computed `feedback(..).validity` to `claim`; standard buildings call `BuildingPlacementRules::place(building_type, area, make_builder)`, which runs shared `feedback` and `claim`.

## Overlay Shader Encoding

The overlay quad is rendered by [`assets/shaders/grids/grid_placer.wesl`](../../assets/shaders/grids/grid_placer.wesl) using `GridPlacerUniform`, which must stay layout-compatible between Rust and WESL:

- **`cell_data: UVec4`** packs up to 64 cells of `area.bounding_box()` at 2 bits per cell: `0` = outside imprint (transparent), `1` = plain imprint cell, `2` = `CellHighlight::Negative`, `3` = framed (`CellHighlight::Positive` or `CellHighlight::Removed`).
- **Outline color** follows `PlacementValidity`: green (`Valid`), amber (`ValidUnpowered`), cyan (`Replaces`), or red (`Invalid`).
- **Cell fills & frames:** `Invalid` draws diagonal stripes across the imprint, rendered thicker and brighter on `Negative` cells. When valid (`Valid`, `ValidUnpowered`, or `Replaces`), framed cells (`Positive` or `Removed`) draw an inset border.
- **Affordability:** When `!is_affordable` on a non-`Invalid` site, the outline alternates between grey and the validity color (mostly grey per period). `Invalid` outlines remain solid red regardless of affordability.

## Adding a Placeable Object

1. **Identifier:** Add a variant to `MapObject` (`game_core/src/types.rs`).
2. **Session setup (`BeginPlacing`):** In a single observer, match `PlacementObject` on `trigger.placement_session`, insert `GridArea::new(trigger.origin, imprint)` (buildings share `on_begin_placing_set_up_building_placement_session`), override `PlacementModes` if needed (e.g. `PlacementModes::on_press()`), and insert default domain settings. If the session has `UiPlacementSession`, also spawn the controls root (requiring `PlacementControls` for automatic cleanup on `StopPlacing`).
3. **Rules & observers:** Implement a `*PlacementRules` `SystemParam` (or reuse `BuildingPlacementRules`), register `update_feedback` in `PlacementFeedbackSystems`, and register `on_place_request_do_so` (plus `on_remove_request_do_so` if removable), reporting outcomes via `trigger.report(..)`.
4. **Faces:** Handle `ObjectFaceRequest` for `FaceSurface::World` (ghost preview, reading any domain settings from `UiPlacementSession`) and `FaceSurface::Ui` (menus/swatches).
5. **Control updates (if configurable):** When a control changes the imprint, insert the updated `GridArea` on `UiPlacementSession` and trigger `GridPlacerGhostRebuildRequest` and `GridPlacerChanged`. When a control changes ghost-visible settings, update the component on `UiPlacementSession` and trigger `GridPlacerGhostRebuildRequest`.
6. **Menu & tooltip:** Register Almanach metadata, handle `MapObjectTooltipRequest`, and add a side-menu entry or hotkey if player-accessible.
