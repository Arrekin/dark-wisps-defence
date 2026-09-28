# Persistence

The persistence crate provides tooling for domains to save and load their state.
The data is saved to SQLite databases (one `.dwd` file per map).

## Worth knowing

1. **A scenario is just a saved game.** Its `.dwd` file is a snapshot of the map, not a separate scenario format or load path.
2. Domain collectors and loaders belong to the domain's `_internal` crate.
3. Fresh spawns and loads use the same builder; loaders set restore values through ordinary builder fields.
4. `EntityIdMap` maps saved entity IDs to newly allocated entities so saved references survive loading.
5. Save writes run on a detached IO task. Staged loaders read on IO tasks and stream chunked `CommandQueue`s; `apply_load_queues` drains them within a per-frame time budget.
6. Schema lives in `persistence/migrations/` and uses refinery

## Save Flow

```
SaveGameSignal { target: SaveTarget }
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
    ▼  drop connection → fs::rename(tmp, path) on success
    │
    ▼  error.store(true) on failure; done.store(true) after the save attempt
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
blocks saving forever).

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
`Res<SaveContext>` and write either real state or scenario defaults. The decision lives
in the one function that already knows the columns — no separate normalize jobs, no
collector/normalizer drift. Collectors that don't care never mention `SaveContext`.

```rust
fn collect_my_entities(
    q: Query<(Entity, &MyData), With<MyEntity>>,
    save_ctx: Res<SaveContext>,
    mut save: SaveWriter,
) {
    if q.is_empty() { return; }
    let rows: Vec<(i64, f32)> = q.iter()
        .map(|(e, d)| (e.index_u32() as i64, if save_ctx.save_as_scenario { 0.0 } else { d.value }))
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
│    └─ file missing
└─ Accept
     ├─ migrate if loading from a file
     ├─ despawn MapBound entities; queue Loading / Init
     ▼
MapLoadingStage state machine (stages are ordering barriers)
    ├─► Init                 (no loaders; advances immediately)
    ├─► LoadMapInfo          build_entity_id_map (exclusive) → map_info loader
    ├─► LoadResources        global state (stats, stock, clock, ...)
    ├─► SpawnMapElements     entities (walls, buildings, wisps, projectiles, ...)
    ├─► SpawnEffectInstances effects referencing entities (brittle, shard slots)
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

fn load_my_entities(ctx: &mut LoadContext) -> rusqlite::Result<()> {
    let mut stmt = ctx.conn.prepare("SELECT id, value FROM my_entities")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let old_id: i64 = row.get(0)?;
        let Some(entity) = ctx.entity(old_id) else {
            Log::warn().dev().tag(Tag::GameLoad).message(format!("my_entities: unmapped id {old_id}"));
            continue;
        };
        ctx.insert(entity, BuilderMyEntity::new(row.get(1)?));
    }
    Ok(())
}
```

## Database Design

### Shared Tables

- `entities` — master registry; all saved entities register here first (`tx.register_entity`)
- `grid_coords` — grid-based positions
- `world_positions` — pixel-precise positions (smooth movement resume)
- `integrity_points` — integrity-point values

### Marker Tables

Entity types have marker tables (`mining_complexes`, `tower_cannons`, `wisps`, ...); shared
tables hold common data, entity-specific columns go on the marker table. `GameDbHelpers` provides the
save/get helpers for the shared tables.

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
   It clears `refinery_schema_history` and re-runs V1 on every `.dwd` file, then exits. Run the
   app once, then switch back to `LaunchAction::StartMap`.

Since all saves already have the final schema, re-running V1 with `IF NOT EXISTS` is a data no-op.
