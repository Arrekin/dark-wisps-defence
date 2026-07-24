# Persistence Architecture

The save/load system serializes full game state to SQLite databases (one `.dwd` file per map).
Domains contribute *collector systems* to a dedicated save schedule and *loader functions* to a
per-stage registry. All persistence behavior (SQL, schema knowledge) lives in `_internal` crates;
api crates carry none of it.

## Core Concepts

1. **Persistence is behavior, so it lives in `_internal`** — Saving is a system in the
   `CollectSave` schedule; loading is a plain `fn(&mut LoadContext) -> rusqlite::Result<()>`.
   Systems and functions register from `_internal` plugins, so api crates never depend on
   `persistence` (see `crates_expansion.md` for the api/internal split rules).

2. **Builder Pattern for spawning** — Each spawnable entity type has a `Builder*` component; an
   `Add` observer expands it into the full entity. The same builder serves fresh spawns and loads:
   a *restore is just a spawn with fully-specified state*, carried in ordinary builder fields
   (e.g. `initial_distance: f32`, `integrity_points: Option<f32>`).

3. **Entity ID Mapping** — `EntityIdMap` (old saved id → freshly pre-spawned `Entity`) lets
   cross-entity references survive save/load. Saved ids are `entity.index_u32() as i64`.

4. **Off-thread I/O, on-thread application** — DB writes happen on a detached IO task; DB reads
   happen on IO tasks that ship `CommandQueue`s to the main thread, which applies them within a
   per-frame time budget. Disk I/O never sits inside a frame.

## Save Flow

```
SaveGameSignal { target: SaveTarget } (Event; dev keybind Z → Quick, editor → Scenario)
    │
    ▼  repack observer (On<SaveGameSignal>)
    │  SaveContext exists → log + return (in-flight block)
    │  else: resolve path from target, insert SaveContext { path, save_as_scenario, done, error }
    │
    ▼  Last: drive_save (exclusive system, run_if resource_added::<SaveContext>)
world.run_schedule(CollectSave)          ← runs ONCE; never part of the main loop
    │       domain collector systems query ECS, read SaveContext for scenario mode,
    │       SaveWriter::submit(closure)
    ▼
PendingSaveJobs (Vec<SaveJob>) taken by driver
    │
    ▼  detached IoTaskPool task
write <path>.tmp: migrations → one transaction → all jobs → commit
    │
    ▼  done.store(true)  (error.store(true) on failure)
drop connection → fs::rename(tmp, path)   ← atomic replace
    │
    ▼  Update: finalize_save (run_if resource_exists::<SaveContext>)
    │  poll done atomic → on completion (success OR error): remove SaveContext
    │  if save_as_scenario: GameMapList::refresh() (new map appears in menu)
```

**Why a custom schedule?** `CollectSave` only executes when the driver calls `run_schedule` — zero
cost on non-save frames, no `run_if` boilerplate on collectors, and the snapshot is atomic by
construction (one schedule run = one frame). Collectors parallelize under the normal Bevy
executor.

**Why tmp + rename?** The old save survives a mid-write crash; a failed save never corrupts the
target file. The SQLite connection is dropped before the rename (Windows file-handle semantics —
see `with_db_connection`'s doc comment).

**SaveContext lifecycle:** `SaveContext`'s *existence* is the save lifecycle — guard + mode
carrier + completion signal in one. The repack observer inserts it (one place requests become
plans); the finalize system removes it on IO completion (success or error — else one bad write
blocks saving forever). Collectors read `save_as_scenario` to choose between real state and
scenario defaults (see Save as Scenario below). `SaveTarget::{ Quick, Scenario(String) }` makes
destination + scenario-ness one decision — a bool+path pair would allow scenario-saving to
`test_save.dwd`.

### Writing a collector

```rust
app.add_systems(CollectSave, collect_my_entities);

fn collect_my_entities(
    q: Query<(Entity, &MyData), With<MyEntity>>,
    mut save: SaveWriter,
) {
    if q.is_empty() { return; }
    // Copy into owned rows — the closure must not borrow the World.
    let rows: Vec<(i64, f32)> = q.iter()
        .map(|(e, d)| (e.index_u32() as i64, d.value))
        .collect();
    save.submit(move |tx| {
        for (id, value) in rows {
            tx.register_entity(id)?;
            tx.execute("INSERT OR REPLACE INTO my_entities (id, value) VALUES (?1, ?2)",
                       rusqlite::params![id, value])?;
        }
        Ok(())
    });
}
```

`SaveJob` closures are `FnOnce(&Transaction) -> rusqlite::Result<()> + Send + Sync + 'static`
(`Sync` because the buffer resource requires it). They run on the IO thread inside the single
save transaction; the first `Err` aborts the save.

### Scenario-aware collectors

Collectors that care about scenario mode (playthrough metadata) read
`Option<Res<SaveContext>>` and write either real state or scenario defaults. The decision lives
in the one function that already knows the columns — no separate normalize jobs, no
collector/normalizer drift. Collectors that don't care never mention `SaveContext`.

```rust
fn collect_my_entities(
    q: Query<(Entity, &MyData), With<MyEntity>>,
    save_ctx: Option<Res<SaveContext>>,
    mut save: SaveWriter,
) {
    if q.is_empty() { return; }
    let save_as_scenario = save_ctx.map(|c| c.save_as_scenario).unwrap_or(false);
    let rows: Vec<(i64, f32)> = q.iter()
        .map(|(e, d)| (e.index_u32() as i64, if save_as_scenario { 0.0 } else { d.value }))
        .collect();
    // ... same submit pattern
}
```

Scenario mode resets: objectives state → `Inactive`, runtime columns → their `DEFAULT 0`/`0.0`
(`current`, `elapsed`), summonings state → `Inactive` + `produced`/`next_spawn_time` → `0`/`0.0`,
moments `fired_count` → `0`. `activated_by` is NOT reset (it's authoring, not playthrough).

## Load Flow

```
LoadGameSignal (observer; dev keybind A)
    │  migrations (sync) · LoadRunner + fresh LoadProgress
    │  despawn MapBound · GameState::Loading · MapLoadingStage::Init
    ▼
MapLoadingStage state machine (stages are ordering barriers)
    ├─► Init                 (no loaders; advances immediately)
    ├─► LoadMapInfo          build_entity_id_map (exclusive) → map_info loader
    ├─► LoadResources        global state (stats, stock, clock, objectives, ...)
    ├─► SpawnMapElements     entities (walls, buildings, wisps, projectiles, ...)
    ├─► SpawnEffectInstances effects referencing entities (brittle, shard slots)
    └─► Ready                on_map_load_ready
```

Per stage: `OnEnter` spawns **one IO task per registered loader**. Each task opens its own
short-lived connection, streams a single cursor over its table (no `LIMIT/OFFSET`), and pushes
world mutations through `LoadContext`, which auto-chunks them into `CommandQueue`s (128 rows
each) sent over a crossbeam channel. Every frame, `apply_load_queues` drains the channel within a
~4 ms budget via `commands.append(&mut queue)`; `advance_stage` moves to the next stage only when
all of the stage's tasks are finished **and** the channel is empty.

**Why pre-allocate entities?** Cross-references (rocket → target wisp) need new entity IDs before
row data loads. `build_entity_id_map` (exclusive system, `OnEnter(LoadMapInfo)`, before the
stage's loaders spawn) reads the `entities` table and `world.spawn_empty()`s one entity per row.
It is exclusive on purpose: deferred `Commands` inserts would be invisible to
`spawn_stage_loaders` running `.after()` it in the same `OnEnter` schedule.

**Progress:** `LoadProgress { total_rows, done_rows }` is a public resource. Totals come from
`SELECT COUNT(*)` over every registered table at load start; `done_rows` is bumped per pushed
mutation. `fraction()` drives a determinate progress bar (approximate by design).

**Cancellation:** a new `LoadGameSignal` during an in-flight load flags the old runner's cancel
`AtomicBool` and replaces it; in-flight loaders drain harmlessly (`push` no-ops when cancelled).

### Writing a loader

```rust
app.register_loader(MapLoadingStage::SpawnMapElements, "my_entities", load_my_entities);

fn load_my_entities(ctx: &mut LoadContext) -> rusqlite::Result<()> {
    let mut stmt = ctx.conn.prepare("SELECT id, value FROM my_entities")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let old_id: i64 = row.get(0)?;
        let Some(entity) = ctx.entity(old_id) else {
            Log::warn().dev().tag(Tag::GameLoad).message(format!("my_entities: unmapped id {old_id}"));
            continue;
        };
        ctx.insert(entity, BuilderMyEntity::new(row.get(1)?));  // observer expands it
    }
    Ok(())
}
```

`LoadContext` API: `entity(old_id) -> Option<Entity>` (warn + `continue` on `None`),
`insert(entity, bundle)`, `insert_resource(res)`, `push(FnOnce(&mut World))` as the escape hatch,
`cancelled()` for optional early-exit in long loops. The context runs on an IO thread — it never
touches the World directly; everything is deferred through the channel. The `table` argument of
`register_loader` feeds the progress totals — use the primary table the loader reads. For moment
kinds, use `register_moment_persistence::<M>()` instead, which combines the collector and loader
in one call (all moments load at `SpawnEffectInstances`).

## Where Code Lives

- **`persistence` crate** — infrastructure only: `CollectSave` + `SaveWriter` + driver,
  `LoadContext` + registry + runner systems, `LoadProgress`, `GameDbHelpers`, migrations. Only
  `_internal` crates (plus `session`, `hud_internal`, the binary) depend on it. **Api crates must
  never depend on `persistence`.**
- **`<domain>_internal`** — collector systems and loader fns, owning their SQL end-to-end.
  Registered in the domain's plugin: `.add_systems(CollectSave, ...)` and `.register_loader(...)`.
- **`<domain>` (api)** — builders as cross-domain *spawn contracts* only. Builder fields describe
  the state of the thing to spawn; anything that exists only to talk to the DB doesn't belong
  here. Single-crate domains (e.g. `session`) keep collectors/loaders in place.

## The Builder Pattern and Restores

Builders are transient components: an `on_builder_add_spawn_*` observer expands them into full
entities and removes them. Loaders reuse this path — they fill a builder from row data and
`ctx.insert` it, staying ignorant of construction internals.

Restore-relevant state is a plain builder field with fresh-spawn semantics built in:

```rust
pub struct BuilderWisp {
    pub wisp_type: WispType,
    pub grid_coords: GridCoords,
    pub integrity_points: Option<f32>,  // None => baseline (fresh spawn)
    pub world_position: Option<Vec2>,   // None => computed from grid_coords
}
```

The observer branches on the field, not on "is this a load". If a type has no cross-domain spawn
contract, skip the builder and `ctx.insert` the real components directly — the loader lives in
`_internal` and is allowed to know them.

## Database Design

Schema lives in `persistence/migrations/` and uses refinery.

### Shared Tables

- `entities` — master registry; all saved entities register here first (`tx.register_entity`)
- `grid_coords` — grid-based positions
- `world_positions` — pixel-precise positions (smooth movement resume)
- `integrity_points` — integrity-point values

### Marker Tables

Each entity type has a marker table (`mining_complexes`, `tower_cannons`, `wisps`, ...); shared
tables hold common data, entity-specific columns go on the marker table. `GameDbHelpers`
(extension trait on `rusqlite::Connection`, see `persistence/src/common.rs`) provides the
save/get helpers for the shared tables.

## Adding a New Persistable Entity

1. **Builder + observer** (if the type is spawnable cross-domain): builder in api with honest
   spawn-state fields, observer in `_internal`.
2. **Collector system** in `_internal`: query live entities → owned rows → `save.submit(closure)`.
3. **Loader fn** in `_internal`: stream the marker table, `ctx.entity(old_id)`, fill the builder
   (or insert components directly), `ctx.insert`.
4. **Register** in the domain plugin:
   ```rust
   app.add_observer(on_builder_add_spawn_my_entity)
      .add_systems(CollectSave, collect_my_entities)
      .register_loader(MapLoadingStage::SpawnMapElements, "my_entities", load_my_entities);
   ```
5. **Schema**: add/extend a migration in `persistence/migrations/`; reuse shared tables.

Pick the stage by dependency: `LoadResources` for global state, `SpawnMapElements` for entities,
`SpawnEffectInstances` for things referencing entities loaded a stage earlier. Within a stage,
loaders run in parallel with no ordering — if A must precede B, put them in different stages.

## Handling Entity References

Loaders resolve references through the id map; a missing target is a warn + skip (or a nullable
field), never a panic:

```rust
let Some(target) = ctx.entity(old_target_id) else { /* warn */ continue; };
```

Runtime systems should tolerate dangling references anyway — entities can despawn between save
and load. For nullable references use nullable columns.

## Best Practices

- **Copy, don't borrow** — collector closures must own their data; snapshot in the query, write
  in the closure.
- **One cursor per loader** — never paginate with `LIMIT/OFFSET`; the multi-frame behavior comes
  from chunked application, not from re-querying.
- **Use `GameDbHelpers`** for shared-table data.
- **Skip transient state** (animations, timers) — reset on load; save world positions for moving
  entities to avoid grid-snapping.
- **Log with tags** — `Tag::GameSave` / `Tag::GameLoad`, `.dev()` for diagnostics, `.player()`
  for user-facing results.

## Merging Migrations

During development, schema changes accumulate as incremental migrations (V2, V3, ...). Once a
feature is complete and all save files are at the latest version, consolidate back into V1.

**When:** feature complete, every save migrated, no legacy saves you care about. ⚠️ Unmigrated
saves are corrupted by this — for released builds, don't.

**How:**

1. Apply all changes directly into `V1__initial.sql` (final column types, final table names,
   dropped columns simply absent).
2. Use `CREATE TABLE IF NOT EXISTS` everywhere so V1 is idempotent on existing databases.
3. Delete the later migration files.
4. Clear refinery metadata per save so V1 re-runs (`DELETE FROM refinery_schema_history;` — a
   commented helper exists in `LoadGameSignal::on_trigger`, and `run_migrations_on_paths` supports
   a rebuild mode).

Since all saves already have the final schema, re-running V1 with `IF NOT EXISTS` is a data no-op.

## File Locations

- `persistence/src/save.rs` — `CollectSave`, `SaveWriter`, `SaveContext`, `SaveGameSignal`,
  `SaveTarget`, repack observer, driver, finalize, atomic write
- `persistence/src/load.rs` — `LoadContext`, registry, runner systems, `LoadProgress`,
  `LoadGameSignal` observer
- `persistence/src/common.rs` — `with_db_connection`, `GameDbHelpers`, `register_loader`
  + `register_moment_persistence` extension, migration helpers
- `persistence/migrations/` — SQLite schema
- `states/src/map_loading_stage.rs` — `MapLoadingStage` definitions
