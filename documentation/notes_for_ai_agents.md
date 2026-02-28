# Notes for AI Agents

Architectural hints and patterns to maintain consistency across the codebase.

## Persistence

Entities are saved/loaded via `Builder*` components that implement `Saveable` and `Loadable` traits. The builder spawns the full entity on insertion via an `on_add` observer.

See [persistence.md](persistence.md) for full details.

## Builder Pattern for Persistable Entities

Every persistable entity type has a `Builder*` component (e.g., `BuilderTowerCannon`, `BuilderExpeditionDrone`).

### Structure

```rust
#[derive(Component, SSS)]
pub struct BuilderFoo {
    grid_position: GridCoords,           // or other spawn-time data
    save_data: Option<FooSaveData>,      // None for fresh spawn, Some for load
}

pub struct FooSaveData {
    entity: Entity,
    health: f32,
    // ... other runtime state to persist
}
```

### Key Methods

- `new(...)` - For spawning fresh entities (no save data)
- `new_for_saving(...)` - For loading (includes save data)
- `on_add(...)` - Observer that builds the real entity, applies save data, then removes the builder
- `on_game_save(...)` - System that collects live entities and queues builders for saving

### Registration in Plugin

```rust
app
    .add_observer(BuilderFoo::on_add)
    .register_db_loader::<BuilderFoo>(MapLoadingStage::SpawnMapElements)
    .register_db_saver(BuilderFoo::on_game_save);
```

### Why This Pattern

1. **Separation of concerns** - Spawn logic stays with the builder, not scattered in load code
2. **Consistent spawn path** - Fresh spawn and load both go through `on_add`
3. **Deferred entity creation** - Builder can be inserted, then observer fires with full ECS access

## Systems Inside Struct Implementations

Place systems as associated functions on the component/struct they operate on when ownership is clear:

```rust
impl ExpeditionDrone {
    fn refueling_system(...) { ... }
    fn patrol_system(...) { ... }
}

// In plugin:
app.add_systems(Update, (
    ExpeditionDrone::refueling_system,
    ExpeditionDrone::patrol_system,
));
```

**Benefits:**
- Plugin `build()` reads as a table of contents
- Related code is co-located
- Clear ownership of behavior

**Exception:** Cross-cutting systems that touch multiple unrelated types can be standalone functions.

## Component Requires

Use `#[require(...)]` to auto-insert dependent components:

```rust
#[derive(Component)]
#[require(Building, BuildingType = BuildingType::ExplorationCenter)]
pub struct ExplorationCenter { ... }
```

This ensures entities always have their required components without manual insertion.

## Immutable Components for State Machines

Use `#[component(immutable)]` for state enums to trigger observers on state change:

```rust
#[derive(Component)]
#[component(immutable)]
pub enum DroneState {
    Stationed,
    Deploying,
    Scanning,
    Returning,
    Refueling,
}
```

Then handle transitions via observer:

```rust
app.add_observer(Self::on_state_changed);

fn on_state_changed(trigger: On<Insert, DroneState>, ...) { ... }
```

## Relationships

Use Bevy's relationship macros for entity ownership:

```rust
#[derive(Component)]
#[relationship(relationship_target = HomeBaseLinkedObjects)]
pub struct HomeBase(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = HomeBase)]
pub struct HomeBaseLinkedObjects(Vec<Entity>);
```

Query the target side to find all entities with a given home.

## Prelude Pattern

Each library crate exports a prelude. The main crate's `prelude.rs` re-exports them:

```rust
pub use lib_core::prelude::*;
pub use lib_inventory::prelude::*;
pub use lib_grid::prelude::*;
```

Add commonly-used items to preludes, not individual `use` statements in every file.

## Module-Level Documentation

Add `//!` doc comments at the top of files that define significant systems:

```rust
//! # Expedition Drone System
//!
//! Brief description of purpose...
//!
//! ## Architecture
//! ...
```

Focus on **intent** and **design decisions**, not code descriptions.

## Plugin Organization

One plugin per logical feature/entity type. Plugin `build()` should clearly show:
- Systems (with run conditions)
- Observers
- DB loader/saver registration

```rust
impl Plugin for FooPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Update, (
                Foo::system_a.run_if(in_state(GameState::Running)),
                Foo::system_b,
            ))
            .add_observer(BuilderFoo::on_add)
            .register_db_loader::<BuilderFoo>(MapLoadingStage::SpawnMapElements)
            .register_db_saver(BuilderFoo::on_game_save);
    }
}
```

## Constants

Group related constants with section comments:

```rust
// Movement tuning
const PATROL_RADIUS: f32 = 150.0;
const DRONE_SPEED: f32 = 160.0;

// Fuel balance
const DEFAULT_MAX_FUEL: f32 = 60.0;
pub const FUEL_CONSUMPTION_RATE: f32 = 3.0;
```

Use `pub` only for constants needed outside the module.

## UI Components

UI components that belong to a specific feature live in that feature's file, not in a separate UI module. For example, `ExplorationCenterInfoPanel` is in `exploration_center.rs`.

Generic/reusable UI lives in `lib-ui` or `src/ui/`.

## Bevy Notes
- `Single<>` query type — system/observer is skipped entirely when not exactly one match. Good if it should only run when a specific entity exists. For 0 or 1, use `Option<Single<>>`.
- `EventReader`/`EventWriter` renamed to `MessageReader`/`MessageWriter`. The `Event` trait + `commands.trigger()` is now for immediate observer-based dispatch.

## Code Style
- Query variables use plural form (e.g., `tabs`, `segments`), not `_q` suffix(singular when using Single<>)
- Encapsulate component internals behind methods. Use the API, don't reach into fields.
- Don't put newlines between struct and its impl blocks
- **Use `pub`, not `pub(crate)`.** `pub(crate)` adds noise with no benefit(in this case).
- **Comments must be timeless.** Never leave comments that reference the current conversation, refactoring session, or rationale like "we moved this here because X was duplicated." Comments should make sense to a reader who has no context of how the code evolved. If the code is self-explanatory, no comment is needed.
- Prefer `query.iter()` over `&query` (the same for `iter_mut`)
- Avoid contractions in variable names — verbosity is preferred.