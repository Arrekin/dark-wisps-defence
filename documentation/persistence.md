# Persistence Architecture

The save/load system enables full game state serialization to SQLite databases using a Builder pattern integrated with Bevy's ECS.

## Core Concepts

The persistence system is built around three ideas:

1. **Builder Pattern** - Each persistable entity type has a `Builder*` component that encapsulates all data needed to reconstruct it. The same builder is used for fresh spawns and loaded entities.

2. **Trait-based Serialization** - `Saveable` and `Loadable` traits define how builders serialize to/from the database.

3. **Entity ID Mapping** - A `DbEntityMap` maintains correspondence between old (saved) and new (loaded) entity IDs, enabling cross-entity references to survive save/load cycles.

## High-Level Flow

### Save Flow

```
SaveGameSignal
    │
    ▼
on_game_save systems (one per entity type)
    │
    ▼
Builder components collected into SaveableBatchCommand
    │
    ▼
GameSaveExecutor writes to SQLite (background thread)
```

**Why background thread?** Prevents frame hitches during save.

### Load Flow

```
LoadGameSignal
    │
    ▼
MapLoadingStage state machine
    │
    ├─► LoadMapInfo    - Pre-allocate entities, build ID mapping
    ├─► LoadResources  - Global state (stats, stock, objectives)
    └─► SpawnMapElements - All game entities
    │
    ▼
Loadable::load() inserts Builder components
    │
    ▼
on_add observers spawn full entities
```

**Why pre-allocate entities?** Cross-references (e.g., rocket → target wisp) require knowing new entity IDs before data is loaded. Pre-allocation creates empty entities upfront, allowing loaders to resolve references.

## The Builder Pattern

### Why Builders?

- **Decoupling** - Separates persistence concerns from runtime components
- **Consistency** - Same code path for fresh spawns and loaded entities  
- **Observer-based** - Bevy's `on_add` observer triggers entity construction
- **Transient** - Builders are removed after spawning, leaving only runtime components

### Structure

Every persistable entity follows this pattern:

```rust
#[derive(Component, SSS)]
pub struct BuilderMyEntity {
    pub position: GridCoords,
    pub save_data: Option<MyEntitySaveData>,  // None = fresh spawn
}
```

The `save_data` field distinguishes:
- `None` → Fresh spawn (from editor or gameplay)
- `Some(...)` → Loaded from save (contains persisted state)

The `on_add` observer checks this to apply saved state (health, upgrades, etc.) before spawning the full entity.

## Database Design

Schema lives in `lib-core/migrations/` and uses refinery for migrations.

### Shared Tables

Common data types have dedicated tables to avoid duplication:

- `entities` - Master registry; all saved entities register here first
- `grid_coords` - Grid-based positions
- `world_positions` - Pixel-precise positions (for smooth movement resume)
- `healths` - Health values

### Marker Tables

Each entity type has a marker table (e.g., `mining_complexes`, `tower_cannons`, `wisps`). These allow efficient type-specific queries while the shared tables store common data.

Entity-specific data (e.g., wisp type, rocket damage) goes in columns on the marker table.

## Key Traits

### `Saveable`

```rust
pub trait Saveable: SSS {
    fn save(self, tx: &rusqlite::Transaction) -> rusqlite::Result<()>;
}
```

Consumes the builder and writes data to the database transaction.

### `Loadable`

```rust
pub trait Loadable {
    fn load(ctx: &mut LoadContext) -> rusqlite::Result<LoadResult>;
}
```

Reads from database and inserts builder components on pre-allocated entities. Returns `LoadResult::Progressed(count)` for pagination or `LoadResult::Finished` when done.

### `GameDbHelpers`

Extension trait on `rusqlite::Connection` providing reusable save/load operations for common data types. Check `lib-core/src/persistence/common.rs` for available helpers.

## Adding a New Persistable Entity

### 1. Create the Builder

```rust
#[derive(Component, SSS)]
pub struct BuilderMyEntity {
    pub my_field: SomeType,
    pub save_data: Option<MyEntitySaveData>,
}
```

### 2. Implement Saveable

Use helpers for common data, custom SQL for entity-specific fields.

### 3. Implement Loadable

Query the marker table, use helpers to fetch common data, resolve entity references via `ctx.get_new_entity_for_old()`.

### 4. Create the Save Snapshot System

The saver needs a system that queries live entities and produces builders when `SaveGameSignal` is received:

```rust
fn on_game_save(
    mut commands: Commands,
    entities: Query<(Entity, &MyData, &Health), With<MyEntity>>,
) {
    if entities.is_empty() { return; }
    
    let batch = entities.iter().map(|(entity, data, health)| {
        let save_data = MyEntitySaveData { 
            entity, 
            health: health.get_current(),
            // ... other persisted state
        };
        BuilderMyEntity::new_for_saving(data.clone(), save_data)
    }).collect::<SaveableBatchCommand<_>>();
    
    commands.queue(batch);
}
```

This system converts live ECS state into builder components that know how to serialize themselves.

### 5. Register in Plugin

```rust
app
    // Observer spawns entities when builder is inserted (used by both fresh spawns and loads)
    .add_observer(BuilderMyEntity::on_add)
    
    // Loader: just register the builder type with appropriate stage
    // The Loadable::load() implementation handles the rest
    .register_db_loader::<BuilderMyEntity>(MapLoadingStage::SpawnMapElements)
    
    // Saver: register the snapshot system that produces builders
    // This system runs when SaveGameSignal is received
    .register_db_saver(BuilderMyEntity::on_game_save);
```

**Key distinction:**
- **Loader** - Register the builder type; the `Loadable` trait implementation does the work
- **Saver** - Register a *system* that queries entities and produces `SaveableBatchCommand<Builder>`

### 6. Add Schema

Create/update migration in `lib-core/migrations/`. Add marker table, reuse shared tables for common data.

## Handling Entity References

Some entities reference others (e.g., projectile → target). During load:

```rust
let new_target = ctx.get_new_entity_for_old(old_target_id)
    .unwrap_or(Entity::PLACEHOLDER);
```

If the referenced entity doesn't exist, use a placeholder. Runtime systems should detect invalid references and handle gracefully (e.g., find new target).

For nullable references, use nullable columns in the schema.

## Best Practices

- **Use helpers** for common data types to ensure consistency
- **Handle missing references** gracefully - entities may be despawned between save and load
- **Batch saves** using `SaveableBatchCommand::from_iter()` 
- **Respect pagination** in loaders for large datasets
- **Skip transient state** - some runtime state (animations, timers) can be reset on load
- **Save world position** for moving entities to avoid snapping to grid centers

## Merging Migrations

During development, schema changes accumulate as incremental migrations (V2, V3, etc.). Once a feature is complete and all save files are migrated to the latest version, these can be consolidated back into V1 for cleaner history.

### When to Merge

- **Feature complete** - All schema changes for the feature are finalized
- **All saves migrated** - Every save file has already run through all migrations
- **No legacy saves** - You accept that older saves (pre-migration) will become incompatible

⚠️ **Warning**: If any saves exist that haven't been migrated to the latest version, merging will corrupt them. For released games with player saves, this is generally not safe unless you're certain no unmigrated saves exist.

### How to Merge

1. **Consolidate V1**: Apply all migration changes directly to `V1__initial.sql`:
   - Add new tables/columns from later migrations
   - Remove dropped columns (don't include them at all)
   - Use final column types (e.g., if V5 changed INTEGER→REAL, use REAL in V1)
   - Drop intermediate table names (e.g., if V2 added `foo2` and V4 renamed it to `foo`, just define `foo`)

2. **Use `IF NOT EXISTS`**: Change all `CREATE TABLE` to `CREATE TABLE IF NOT EXISTS` so the migration is idempotent on existing databases.

3. **Delete later migrations**: Remove V2, V3, etc. files.

4. **Clear refinery metadata**: Before loading each save, clear the migration history so refinery re-runs V1:
   ```sql
   DELETE FROM refinery_schema_history;
   ```
   A commented helper exists in `LoadGameSignal::on_trigger` for this purpose.

### Why This Works

Since all saves are already at the final schema state, re-running V1 with `IF NOT EXISTS` is a no-op for data—tables already exist with correct structure. Refinery simply re-registers that V1 has run.

## File Locations

- `lib-core/src/persistence/` - Core infrastructure (traits, executor, registry)
- `lib-core/migrations/` - SQLite schema migrations
- `lib-core/src/states.rs` - `MapLoadingStage` definitions
