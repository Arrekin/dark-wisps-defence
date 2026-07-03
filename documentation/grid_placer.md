# Grid Object Placer System

Architecture documentation for placing and removing grid-based objects (buildings, walls, wisps, quantum fields, etc.).

## Overview

The placer system separates **what to place** (domain knowledge in Almanach) from **how to place** (UI/input handling in GridObjectPlacer). Domain modules own their placement/removal logic via observers.

## Architecture Layers

```
┌─────────────────────────────────────────────────────────────────┐
│  Input Sources                                                  │
│  - Keyboard shortcuts (keyboard_input_system)                   │
│  - UI buttons (construction_menu, etc.)                         │
│  └──> GridObjectPlacerRequest                                   │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  GridObjectPlacer (grids_internal/src/placement.rs)             │
│  - Singleton entity with GridCoords, GridImprint, Sprite        │
│  - Follows mouse cursor                                         │
│  - Runs validation and shows color feedback                     │
│  - Emits PlaceRequest<T>/RemoveRequest<T> on mouse clicks       │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  Domain Observers (walls.rs, buildings/common_systems.rs, etc.) │
│  - Listen for their specific PlaceRequest<T>/RemoveRequest<T>   │
│  - Pull coords/imprint from placer entity                       │
│  - Execute final validation and spawn/despawn entities          │
└─────────────────────────────────────────────────────────────────┘
```

## Key Components

### grids/src/placement.rs
Generic events, traits, and data structures:
- **`PlaceRequest<T>`** - Trigger event for placement
- **`RemoveRequest<T>`** - Trigger event for removal
- **`BeginPlacing<T>`** - Trigger event when placement mode activates for type T (used for domain setup UI, e.g., QuantumField size selector)
- **`StopPlacing`** - Trigger event when placement exits or switches type (used for domain cleanup)
- **`PlacementEmitter`** - Trait for boxed dynamic dispatch of events
- **`GridPlacerChanged`** - Event to trigger revalidation
- **`GridPlacerOverridePropertyRequest`** - Event to modify placer properties at runtime (e.g., quantum field size)
- **`PlacementValidatorFn`** - Static function pointer for validation: `fn(MapObject, GridCoords, GridImprint, &GridsCollectionParam) -> PlacementValidity`
- **`GridsCollectionParam`** - Read-only grid data bundle for validators (ObstacleGrid, EnergySupplyGrid, ReservedCoords)
- **`PlacementMode`** - OnRelease (single click) vs OnPress (burst/drag mode)
- **`ObjectPlacementInfo`** - Complete placement config extracted from Almanach

### almanach/src/lib.rs
Central metadata store:
- **`Almanach::get_placement_info_for(MapObject)`** - Returns `ObjectPlacementInfo` for any placeable type
- Each `*Info` struct (BuildingInfo, WallInfo, DarkOreInfo, QuantumFieldInfo) stores:
  - Domain data (name, costs, stats)
  - Placement validator function
- `From<&*Info> for ObjectPlacementInfo` impls construct the generic placement config (emitters, modes) from domain info

### grids_internal/src/placement.rs
UI controller:
- **`GridObjectPlacerRequest`** - Resource to request placement mode activation
- **`ActivePlacement`** - Current session data (MapObject + ObjectPlacementInfo)
- **`GridObjectPlacer`** - Singleton component that:
  - Tracks cursor position (GridCoords)
  - Stores current footprint (GridImprint)
  - Shows validation feedback via sprite color
  - Emits place/remove requests based on PlacementMode

## Data Flow

### Activation
1. Input sets `GridObjectPlacerRequest` with target `MapObject`
2. `begin_placement` triggers `StopPlacing` (cleanup for previous type), extracts `ObjectPlacementInfo` from Almanach
3. If `begin_placing_emitter` is set, emits `BeginPlacing<T>` for domain setup (e.g., QuantumField spawns its size selector UI)
4. Placer stores `ActivePlacement`, updates imprint, enters `UiInteraction::PlaceGridObject` state

### Validation (continuous while placing)
1. Mouse movement triggers `follow_mouse_system` → `GridPlacerChanged`
2. `revalidate_placement` runs validator with current coords/imprint/grids
3. Sprite color updated

### Placement/Removal
1. Mouse click detected in `handle_placement_click`
2. Based on `PlacementMode` (OnPress vs OnRelease), emits `PlaceRequest<T>` or `RemoveRequest<T>`
3. Domain observer (e.g., `on_wall_place_request_do_so`) receives event
4. Observer reads coords/imprint from placer entity, performs final validation, spawns/despawns

## Adding a New Placeable Type

1. **Define marker type** in game_core (e.g., `pub struct NewThing;`)

2. **Add to MapObject enum** in game_core/src/types.rs

3. **Create *Info struct** in almanach/src/lib.rs with domain data and a `validate: PlacementValidatorFn` field

4. **Implement `From<&NewThingInfo> for ObjectPlacementInfo`** to wire up emitters and modes:
   ```rust
   impl From<&NewThingInfo> for ObjectPlacementInfo {
       fn from(info: &NewThingInfo) -> Self {
           Self {
               imprint: info.grid_imprint,
               validate: info.validate,
               place_emitter: Box::new(PlaceRequest::<NewThing>::default()),
               remove_emitter: Some(Box::new(RemoveRequest::<NewThing>::default())),  // if removable
               begin_placing_emitter: None,  // Some(...) only if setup UI needed
               place_mode: PlacementMode::OnRelease,
               remove_mode: PlacementMode::OnRelease,
           }
       }
   }
   ```

5. **Add to `Almanach::get_placement_info_for()`** match arm

6. **Create domain observers** in the feature module:
   ```rust
   fn on_new_thing_place_request_do_so(
       _trigger: On<PlaceRequest<NewThing>>,
       mut commands: Commands,
       placer: Single<(&GridObjectPlacer, &GridCoords, &GridImprint)>,
       // ... other resources
   ) {
       let (gop, coords, imprint) = placer.into_inner();
       let Some(active_placement) = &gop.active_placement else { return };
       // Validate and spawn...
   }
   ```

7. **Register in plugin's `build()`** using `AlmanachAppExt`:
   ```rust
   .register_new_things(NewThingInfo { ... })  // via AlmanachAppExt
   .add_observer(on_new_thing_place_request_do_so)
   .add_observer(on_new_thing_remove_request_do_so)  // if removable
   ```

## Runtime Property Override

Some placeable types need runtime configuration (e.g., QuantumField size selector). Use:
```rust
commands.trigger(GridPlacerOverridePropertyRequest::OverrideImprint(new_imprint));
```

This triggers `GridObjectPlacer::on_modify_apply_placer_override` which updates the placer and fires `GridPlacerChanged` for revalidation.
