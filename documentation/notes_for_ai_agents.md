# Notes for AI Agents

Architectural hints and patterns to maintain consistency across the codebase.

## Persistence

Entities are saved via collector systems in the `CollectSave` schedule and loaded via plain `LoaderFn`s registered per `MapLoadingStage`. All persistence logic lives in `_internal` crates; api crates carry none of it. Builders serve as cross-domain spawn contracts for both fresh spawns and loads.

See [persistence.md](persistence.md) for full details.

## Builder Pattern for Persistable Entities

Every persistable entity type has a `Builder*` component (e.g., `BuilderTowerCannon`, `BuilderExpeditionDrone`). The builder carries honest fields for both fresh-spawn and restore paths — no `SaveData` struct, no `save_data: Option<...>`.

### Structure

```rust
#[derive(Component, SSS)]
pub struct BuilderFoo {
    grid_position: GridCoords,           // or other spawn-time data
    integrity_points: Option<f32>,       // None => baseline (fresh spawn), Some => restore
    // ... other fields with fresh-spawn defaults
}
```

### Key Methods

- `new(...)` - For spawning fresh entities (defaults for all restore-relevant fields)
- `with_*(...)` - Setters for restore-relevant fields, chained after `new()` by loaders
- `on_builder_add_spawn_foo(...)` - Observer that builds the real entity, applies builder fields, then removes the builder
- `collect_foos(...)` - Collector system (registered in `CollectSave`) that queries live entities and submits save jobs

### Registration in Plugin

```rust
app
    .add_observer(on_builder_add_spawn_foo)
    .add_systems(CollectSave, collect_foos)
    .register_loader(MapLoadingStage::SpawnMapElements, "foos", load_foos);
```

### Why This Pattern

1. **Separation of concerns** - Spawn logic stays with the builder, not scattered in load code
2. **Consistent spawn path** - Fresh spawn and load both go through `on_builder_add_spawn_foo`
3. **Deferred entity creation** - Builder can be inserted, then observer fires with full ECS access

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
app.add_observer(Self::on_state_changed_handle_drone_state_change);

fn on_state_changed_handle_drone_state_change(trigger: On<Insert, DroneState>, ...) { ... }
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
- Collector systems (`CollectSave` schedule) and loader registration

```rust
impl Plugin for FooPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Update, (
                system_a.run_if(in_state(GameState::Running)),
                system_b,
            ))
            .add_observer(on_builder_add_spawn_foo)
            .add_systems(CollectSave, collect_foos)
            .register_loader(MapLoadingStage::SpawnMapElements, "foos", load_foos);
    }
}
```

Plugins and preludes are at the top of the file(see examples in other files)

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

Generic/reusable UI lives in `widgets` / `widgets_internal`.

## Bevy Notes
- Be wary that we are using the newest Bevy 0.19! You may have outdated info so if any code feels wierd always check the local code and/or online docs!
- `Single<>` query type — system/observer is skipped entirely when not exactly one match. Good if it should only run when a specific entity exists. For 0 or 1, use `Option<Single<>>`.
- `EventReader`/`EventWriter` are now `MessageReader`/`MessageWriter` (buffered, frame-delayed). The `Event` trait is now used with `commands.trigger()` for immediate(at flush-point) observer-based dispatch, including recursive event propagation.

### Observer Self-Cleanup Pattern

Observers attached to entity A via `.observe()` that reference entity B (which may be despawned independently) must self-clean to avoid leaking. The closure checks if B still exists; if not, it despawns its own observer via `trigger.observer()` + `try_despawn`.

**Custom events** (don't fire during despawn — no race):
```rust
move |trigger: On<E>, mut commands: Commands, moments: Query<&mut Moment>| {
    let Ok(mut moment) = moments.get_mut(moment_entity) else {
        commands.entity(trigger.observer()).try_despawn();
        return;
    };
    moment.fire(&mut commands.entity(moment_entity));
}
```

**Lifecycle events** (`Remove`/`Discard` fire during despawn — race with Bevy's auto-cleanup): guard with `new_archetype.is_some()` to skip self-despawn during despawn chains:
```rust
move |trigger: On<Remove, T>, mut commands: Commands, indicators: Query<&IndicatorType>| {
    if trigger.trigger().new_archetype.is_some() && indicators.get(entity).is_err() {
        commands.entity(trigger.observer()).try_despawn();
        return;
    }
    // ... do work
}
```
`new_archetype.is_none()` means the entity is being despawned — Bevy's auto-cleanup will handle the observer.

## Agent Guidelines
- **Think before implementing.** When asked to fix a bug or add a feature, first consider whether the change reveals a deeper architectural issue. Prefer fixing the root cause over patching symptoms.
- **Avoid tunnel vision.** Don't just implement the literal request — evaluate whether it fits the existing patterns. If it doesn't, flag it and suggest an approach that does.
- **Run `cargo clippy` before completing any delivery.** The workspace has a `[workspace.lints.clippy]` config in the root `Cargo.toml` that suppresses lints that don't fit a Bevy codebase. Any warnings that remain are genuine and must be fixed before the work is considered done.

## Code Style
- Query variables use plural form (e.g., `tabs`, `segments`), not `_q` suffix(singular when using Single<>)
- Encapsulate component internals behind methods. Designs APIs. 
- **Use `pub` in API crates, `pub(crate)` in `_internal` crates.** `pub(crate)` in internal crates enforces API boundaries — `pub` in internal is a sign of design issues.
- **Comments must be timeless.** Never leave comments that reference the current conversation, refactoring session, or rationale like "we moved this here because X was duplicated." Comments should make sense to a reader who has no context of how the code evolved. If the code is self-explanatory, no comment is needed.
- Prefer `query.iter()` over `&query` (the same for `iter_mut`)
- Avoid contractions in variable names — verbosity is preferred.

### System parameter order
```rust
fn my_bevy_system(
    trigger: Trigger<T>,
    mut commands: Commands,
    <resources>
    <queries>
    <locals>
)
```

## API / Internal Crate Split

Each domain has two crates: `foo` (API) and `foo_internal` (implementation).

- **API crate** (`foo`): components, events, traits, types, prelude. No systems, no plugins. Mark items `pub`.
- **Internal crate** (`foo_internal`): systems, observers, plugins, save/load logic. Mark items `pub(crate)` to prevent leakage. Depends on its API crate and other API crates.

The binary crate depends on `_internal` crates. API crates depend only on other API crates.

Exceptions to the split: `overlays` and `editor` are consumer-only leaves — only the binary depends on them, so there is no shared-types layer to extract; they keep types, systems, and Plugin in one crate with no `_internal` suffix. `almanach` is a hybrid: other crates use it like an api crate, but it also owns its systems and Plugin in the same crate; split it into `almanach`/`almanach_internal` only if its systems grow or start dragging heavy dependencies into consumers' builds.

All dependency versions live in `[workspace.dependencies]` in the root `Cargo.toml`; member crates reference them with `workspace = true`, and bevy's `dynamic_linking` is enabled in the workspace definition so every build graph (including per-crate `cargo check -p foo`) resolves the same bevy feature set and shares one bevy build in `target/`. Remove it there if release builds are ever needed.

## Naming Convention

### Observers: `on_<trigger>_<action>`

Observer names describe both the trigger and what the function does. The trigger is also visible in the `On<...>` parameter, but the name makes call sites in `build()` self-documenting.

- `on_builder_add_spawn_cannonball` — trigger: builder added, action: spawn cannonball
- `on_click_toggle_research` — trigger: click, action: toggle research
- `on_insert_obsolete_request_rebuild` — trigger: Obsolete inserted, action: request rebuild

**Tautology rule:** when trigger and action mean the same thing, use `do_so`:
- `on_building_place_request_do_so` (not `on_building_place_request_place_building`)
- `on_recall_drone_do_so` (not `on_recall_drone_recall_drone`)
- `on_check_for_obsoletion_do_so`

**Builder `on_add`:** always `on_builder_add_spawn_X` — works as both free function and impl method name.

### Systems: action-only names

Systems have no single trigger (they run every frame or on schedule), so they use plain action names:
- `update_emissions_grid`, `show_main_menu`, `apply_supplier_changes`
- Save collectors: `collect_cannonballs`, `collect_walls` (registered in `CollectSave`)
- Loaders: `load_cannonballs`, `load_walls` (registered via `register_loader`)

### Import Ordering

Five groups, separated by blank lines, alphabetical within each:
1. `std` / `core` / `alloc`
2. External crates (`bevy`, `strum`, ...)
3. Workspace crates (`game_core`, `grids`, ...)
4. `crate::` paths
5. `super::` / `self::` paths

Merge duplicate imports from the same crate into a single `use` statement.