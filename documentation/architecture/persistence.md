# Persistence

The persistence crate provides tooling for domains to save and load their state.
The data is saved to SQLite databases (one `.dwd` file per saved game).

## Worth knowing

1. **A scenario is just a saved game.** Its `.dwd` file is a snapshot of the map, not a separate scenario format or load path.
2. **A `MapFile` names every save and load location, and its place decides the save kind.** `Scenario` (`maps/<name>.dwd`) resets playthrough state on save. `Quick` (`test_save.dwd`, the keybinds) and `Save` (`saves/<name>.dwd`) keep it. Names never carry a path.
3. Domain collectors and loaders belong to the domain's `_internal` crate.
4. Fresh spawns and loads use the same builder; loaders set restore values through ordinary builder fields.
5. `EntityIdMap` maps saved entity IDs to newly allocated entities so saved references survive loading.
6. Save writes run on a detached IO task. Staged loaders read on IO tasks and stream chunked `CommandQueue`s; `apply_load_queues` drains them within a per-frame time budget.
7. Schema lives in `persistence/migrations/` and uses refinery
8. Saves and loads report their outcome (`SaveGameReport`, `LoadGameReport`) to the `ResponseRequest` they carry.

## Save Flow

```
SaveGameSignal { file: MapFile, response }
    │
    ▼  on_save_game_signal_do_so (On<SaveGameSignal>)
    │  no map in play (InGame absent) → report NotInGame + return
    │  SaveRunner exists → report AlreadySaving + return (in-flight block)
    │  else: insert SaveRunner { file, response, outcome }
    │
    ▼  Last: drive_save (exclusive system, run_if resource_added::<SaveRunner>)
world.run_schedule(CollectSave)          ← runs ONCE; never part of the main loop
    │       domain collector systems query ECS, read SaveRunner for scenario mode,
    │       SaveWriter::submit(closure)
    ▼
PendingSaveJobs (Vec<SaveJob>) taken by driver; none → report NothingToSave, remove SaveRunner
    │
    ▼  detached IoTaskPool task
create the file's folder; write <path>.tmp: migrations → one transaction → all jobs → commit
    │
    ▼  drop connection → fs::rename(tmp, path) on success
    │
    ▼  outcome.set(Ok | Err(reason)) after the save attempt
    │
    ▼  Update: finalize_save (run_if resource_exists::<SaveRunner>)
    │  poll outcome → once set (success OR error): report Saved / Failed { reason }, remove SaveRunner
    │  if saves_as_scenario(): ScenarioList::refresh() (new map appears in menu)
```

**Why a custom schedule?** `CollectSave` only executes when the driver calls `run_schedule` — zero
cost on non-save frames, no `run_if` boilerplate on collectors, and the snapshot is atomic by
construction (one schedule run = one frame). Collectors parallelize under the normal Bevy
executor.

**Why tmp + rename?** The old save survives a mid-write crash; a failed save never corrupts the
target file. The SQLite connection is dropped before the rename (Windows file-handle semantics —
see `with_db_connection`'s doc comment).

**SaveRunner lifecycle:** `SaveRunner`'s *existence* is the save lifecycle: guard, carrier of the
file and the report recipient, and completion signal in one. `on_save_game_signal_do_so` inserts
it; `finalize_save` removes it on IO completion (success or error — else one bad write blocks
saving forever).

### Writing a collector

```rust
app.add_systems(CollectSave, collect_my_entities);

fn collect_my_entities(
    my_entities: Query<(Entity, &MyData), With<MyEntity>>,
    mut save: SaveWriter,
) {
    if my_entities.is_empty() { return; }
    // Copy into owned rows — the closure must not borrow the World.
    let rows: Vec<(u32, f32)> = my_entities.iter()
        .map(|(entity, data)| (entity.index_u32(), data.value))
        .collect();
    save.submit(move |ctx| {
        for (id, value) in rows {
            ctx.register_entity(id)?;
            ctx.tx.execute("INSERT OR REPLACE INTO my_entities (id, value) VALUES (?1, ?2)", rusqlite::params![id, value])?;
        }
        Ok(())
    });
}
```

`SaveJob` closures are `FnOnce(&SaveContext) -> rusqlite::Result<()> + Send + Sync + 'static`
(`Sync` because the buffer resource requires it). They run on the IO thread inside the single
save transaction; the first `Err` aborts the save.

### Scenario-aware collectors

Collectors that care about scenario mode (playthrough metadata) read
`Res<SaveRunner>` and write either real state or scenario defaults. The decision lives
in the one function that already knows the columns — no separate normalize jobs, no
collector/normalizer drift. Collectors that don't care never mention `SaveRunner`.

```rust
fn collect_my_entities(
    save_runner: Res<SaveRunner>,
    my_entities: Query<(Entity, &MyData), With<MyEntity>>,
    mut save: SaveWriter,
) {
    if my_entities.is_empty() { return; }
    let rows: Vec<(u32, f32)> = my_entities.iter()
        .map(|(entity, data)| (entity.index_u32(), if save_runner.saves_as_scenario() { 0.0 } else { data.value }))
        .collect();
    // ... same submit pattern
}
```

## Load Flow

```
LoadGameSignal(LoadMapConfig)
├─ Reject → report reason; current map untouched
│    ├─ already Loading
│    ├─ GameState transition queued
│    ├─ file missing
│    └─ migration failed
└─ Accept
     ├─ despawn MapBound entities; queue Loading / Init
     ▼
MapLoadingStage state machine (stages are ordering barriers)
    ├─► Init                 (no loaders; advances immediately)
    ├─► LoadMapInfo          build_entity_id_map (exclusive) → map_info loader
    ├─► LoadResources        global state (stats, stock, clock, ...)
    ├─► SpawnMapElements     entities (walls, buildings, wisps, projectiles, ...)
    ├─► SpawnEffectInstances effects referencing entities (brittle, shard sockets)
    └─► Ready                on_map_load_ready (queues game_start_state, admin mode)
    ▼
OnExit(GameState::Loading)   finish_map_load: LoadGameReport { Loaded } ·
                             remove LoadMapConfig + LoadRunner
```

Per stage: `OnEnter` spawns **one IO task per registered loader**. Each task opens its own
short-lived connection, streams a single cursor over its table (no `LIMIT/OFFSET`), and pushes
world mutations through `LoadContext`, which auto-chunks them into `CommandQueue`s (128 rows
each) sent over a crossbeam channel. Every frame, `apply_load_queues` drains the channel within a
~4 ms budget via `commands.append(&mut queue)`; `advance_stage` moves to the next stage only when
all of the stage's tasks are finished **and** the channel is empty.

**Why pre-allocate entities?** Saved entity IDs cannot be reused in the new Bevy world, and loaders may encounter a reference before its target's data has loaded—for example, a rocket targeting a wisp. Before the loaders start, `build_entity_id_map` creates an empty entity for every saved ID and records the mapping. Loaders can then resolve references regardless of load order; the target's components can be added later.

**Progress:** `LoadProgress { total_rows, done_rows }` is a public resource. Totals come from
`SELECT COUNT(*)` over every registered table at load start; `done_rows` is bumped per pushed
mutation. `fraction()` drives a determinate progress bar (approximate by design).

### Writing a loader

```rust
app.register_loader(MapLoadingStage::SpawnMapElements, "my_entities", load_my_entities);

fn load_my_entities(ctx: &mut LoadContext) -> LoadResult {
    ctx.for_each_entity("SELECT id, kind FROM my_entities", |ctx, old_id, entity, row| {
        let kind = row.get_parsed::<MyKind>(1)?;
        let grid_position = ctx.grid_position(old_id)?;
        ctx.insert(entity, BuilderMyEntity::new(grid_position, kind));
        Ok(())
    })
}
```

## Database Design

### Shared Tables

- `entities` — master registry; all saved entities register here first (`ctx.register_entity`).
  A collector registers only the entities it saves. Collectors run in any order, so a foreign key
  to `entities` goes only on a row's own id; references to other entities resolve at load, where
  an unsaved target fails `ctx.entity` and the loader logs it, then skips or degrades the row.
- `grid_coords` — grid coordinates and altitude level
- `world_positions` — pixel-precise positions (smooth movement resume)
- `integrity_points` — integrity-point values
- `time_bounds` — `TimeBound` lifetimes in absolute game time; no row means no saved lifetime

### Marker Tables

Each entity type has a marker table, such as `mining_complexes`, `tower_cannons`, or `wisps`.
Type-specific columns belong on that table; common data belongs in shared tables.
`SaveContext` and `LoadContext` provide helpers for shared data. For example,
`ctx.save_grid_position(id, position)?` saves coordinates and altitude to `grid_coords`,
and `ctx.grid_position(old_id)?` restores them as a `GridPosition`.

### Lists

Lists have their own tables, and their owners store only the list id. For enum entries, give each variant's data its own table so the main table isn't filled with NULLs.
Resource lists work this way: see `persistence/src/resource_lists.rs`, used by `stock` and `researches`.

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
4. Set `LaunchAction::RebuildSQLMigrationsMetadata` as the default launch action in `src/main.rs`.
   It clears `refinery_schema_history` and re-runs V1 on every `.dwd` file, then exits.
5. Rebuild the `persistence` crate before running the app: `embed_migrations!` embeds the migration
   files at compile time. Run `cargo clean -p persistence` to force the next build to embed the
   consolidated migration set.
6. Run the app once, then switch back to `LaunchAction::StartMap`.

All saves must already use the final schema. Under that condition, re-running V1 with
`IF NOT EXISTS` rebuilds migration metadata without changing saved data.
