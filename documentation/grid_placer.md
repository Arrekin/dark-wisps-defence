# Grid Object Placer System

Architecture and extension guide for placing and removing grid-based objects.

## Overview

The placer owns input, cursor tracking, validation feedback, and placement-session state. Domain modules own object metadata, rendering, validation, and the final placement or removal operation.

Communication between the placer and domains uses non-generic events carrying a `MapObject`. Domain observers match the variants they support.

## Architecture

```text
Input source
    |
    | sets GridObjectPlacerRequest(MapObject)
    v
GridObjectPlacer
    |- stores GridCoords, GridImprint, and PlacementStyle
    |- renders the validity overlay
    |- owns a child entity containing the object preview
    |- emits BeginPlacing, PlaceRequest, RemoveRequest, and StopPlacing
    v
Domain observers
    |- match their MapObject variant
    |- attach UI or world faces
    |- provide optional placement controls
    |- validate and spawn or remove domain entities
```

The Almanach connects the two sides. It supplies a generic `ObjectPlacementInfo` for each `MapObject` while retaining domain-specific metadata such as costs, sprites, and configuration.

## Placement Types

The public placement types are defined in `grids/src/placement.rs`.

### Requests and session events

- **`GridObjectPlacerRequest`** stores a pending `MapObject`. UI and keyboard input set this resource to start or switch placement.
- **`BeginPlacing(MapObject)`** is emitted once after a new placement session has been initialized. Domains use it to create controls such as the wall style picker or quantum-field size selector.
- **`StopPlacing`** is emitted when placement ends or switches to another object. Domain placement controls use it for cleanup.
- **`PlaceRequest(MapObject)`** asks the matching domain to place the object at the current placer coordinates.
- **`RemoveRequest(MapObject)`** asks the matching domain to remove an object at the current placer coordinates. A type supports removal only when its domain registers a matching observer.
- **`GridPlacerChanged`** requests validation feedback to be recalculated.
- **`GridPlacerOverridePropertyRequest`** changes the active `GridImprint` or opaque `PlacementStyle` value.

These events are deliberately non-generic. A domain observer filters them by matching the carried `MapObject`:

```rust
fn on_wall_place_request(
    trigger: On<PlaceRequest>,
    mut commands: Commands,
    mut grids: GridsCollectionParam,
    placer: Single<(&GridCoords, &GridImprint, &PlacementStyle), With<GridObjectPlacer>>,
) {
    let PlaceRequest(MapObject::Wall) = *trigger else { return };
    let (coords, imprint, style) = placer.into_inner();
    // Validate and place the wall.
}
```

### Placement configuration

`ObjectPlacementInfo` is the domain-independent configuration stored by the active placer:

- `imprint: GridImprint`
- `validate: PlacementValidatorFn`
- `annotate: PlacementAnnotatorFn`
- `placement: PlacementModes`

`PlacementModes` controls when place and remove requests are emitted:

- `PlacementModes::default()` emits each request once when its mouse button is released.
- `PlacementModes::on_press()` emits while the button is held, supporting drag placement and removal.

`PlacementStyle(u32)` is an opaque domain-owned selection. The placer stores and forwards it without interpreting it. Walls, for example, convert it to `WallStyleKey`.

`GridsCollectionParam` provides validators and placement handlers with the obstacle, energy-supply, reserved-coordinate, and wisp grids. `reserved_coords` is mutable so successful placement handlers can reserve their footprint.

## Object Faces and the Placement Preview

Object rendering is requested through `ObjectFaceRequest` in `game_core/src/display.rs`:

- `ObjectFaceRequest::ui(entity, object)` requests an opaque UI face.
- `ObjectFaceRequest::world(entity, object)` requests an opaque world-space face.
- `ObjectFaceRequest::ghost(entity, object)` requests a world-space face using `GHOST_ALPHA`.

Each domain registers an `ObjectFaceRequest` observer and matches the `MapObject` variants it renders. `FaceSurface::Ui` attaches UI components such as `ImageNode` or `MaterialNode`; `FaceSurface::World` attaches world components such as `Sprite` or `MeshMaterial2d`.

The target entity already carries any context required by the renderer. Placement ghosts carry the active `GridImprint` and `PlacementStyle`; a wall renderer reads the style, while world-space renderers use the imprint to size their output.

The placer consists of two visual layers:

1. The placer entity renders the validity overlay at `ZDepth::GRID_PLACER`.
2. Its child renders the object preview at `ZDepth::GRID_PLACER_GHOST`.

The preview is rebuilt when the object, imprint, or style changes. This lets each domain use its normal rendering implementation instead of reducing all previews to image handles.

Side-menu placement tiles use the same protocol: the tile creates a sized face node and sends `ObjectFaceRequest::ui` for its `MapObject`.

## Validation Feedback

A placement validator returns one of:

- `PlacementValidity::Valid`
- `PlacementValidity::ValidUnpowered`
- `PlacementValidity::Invalid`

The enum discriminants match the `VALIDITY_*` constants in `assets/shaders/grid_placer.wgsl`.

The annotator returns `(GridCoords, CellHighlight)` pairs for cells that need more specific feedback:

- `CellHighlight::Negative` identifies cells blocking placement.
- `CellHighlight::Positive` identifies beneficial cells, such as dark ore covered by a mining complex.

`build_cell_data` packs the footprint and annotations into two bits per bounding-box cell. The shader uses this data to draw the footprint outline, invalid-cell hatching, and positive-cell frames. The object preview is rendered independently below the overlay.

Validation runs when the cursor moves, when a session starts, and after an imprint or style override. Placement handlers validate again before changing the game world.

## Placement Flow

### Starting or switching placement

1. Input stores a `MapObject` in `GridObjectPlacerRequest`.
2. `begin_placement` emits `StopPlacing` if another object is active.
3. It obtains `ObjectPlacementInfo` from the Almanach.
4. It resets the active imprint and style, then stores `ActivePlacement`.
5. It rebuilds the preview and emits `BeginPlacing` and `GridPlacerChanged`.
6. It transitions to `UiInteraction::PlaceGridObject`; entering that state shows the placer.

### Cursor and validation

1. `follow_mouse_system` copies `MouseInfo::grid_coords` to the placer when the cursor changes cells.
2. It emits `GridPlacerChanged`.
3. `revalidate_placement` runs the active validator and annotator.
4. It updates the overlay mesh when the imprint bounds change and uploads the new validity data.

### Placement and removal

1. `handle_placement_click` applies the active `PlacementModes` to the left and right mouse buttons.
2. It emits `PlaceRequest(map_object)` or `RemoveRequest(map_object)`.
3. Domain observers match the object variant.
4. The matching observer reads the current placer state, performs final validation, and spawns or removes domain entities.

### Ending placement

Leaving `UiInteraction::PlaceGridObject` hides the placer, clears `ActivePlacement`, and emits `StopPlacing` so domain controls can remove themselves.

## Runtime Overrides

Use `OverrideImprint` when domain controls change the active footprint:

```rust
commands.trigger(GridPlacerOverridePropertyRequest::OverrideImprint(new_imprint));
```

Use `OverrideStyle` for a domain-owned variant:

```rust
commands.trigger(GridPlacerOverridePropertyRequest::OverrideStyle(style_index));
```

Both overrides rebuild the object preview and emit `GridPlacerChanged`.

## Adding a Placeable Object

1. Add the object to `MapObject` in `game_core/src/types.rs`.
2. Add or extend its Almanach info type with an imprint, validator, annotator, and `PlacementModes`.
3. Implement conversion from the domain info to `ObjectPlacementInfo`.
4. Add the object to `Almanach::get_placement_info_for` and `Almanach::presentation_for`.
5. Register the metadata through `AlmanachAppExt` in the domain plugin.
6. Register `PlaceRequest` and, if supported, `RemoveRequest` observers. Each observer must match the domain's `MapObject` variant before acting.
7. Register an `ObjectFaceRequest` observer for the surfaces the object supports.
8. If the object needs placement controls, observe `BeginPlacing` to create them and `StopPlacing` to remove them. Controls update the placer through `GridPlacerOverridePropertyRequest`.
9. Add the object to the appropriate side-menu offering or fixed section.

A minimal registration uses release-on-click placement:

```rust
placement: PlacementModes::default(),
presentation: ObjectPresentation {
    tooltip: Some(new_thing_tooltip),
},
```

Use `PlacementModes::on_press()` for objects intended to be painted across the grid.