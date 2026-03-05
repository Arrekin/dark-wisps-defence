# Modifiers & Effects

Every stat on every entity — a tower's attack range, a wisp's movement speed, a damage
multiplier from a debuff — is produced by the same mechanism: a set of **effect instance
entities** contribute values into a **modifier bank**, which aggregates them and writes the
result into **derived components** that gameplay systems read.

This document covers how the system works and why it is designed this way.

## Why a Unified System

Before this system, stats were set directly on entities and modified through ad-hoc
mechanisms. Each modifier source (upgrades, auras, debuffs) had its own way of reading and
writing stat values, which meant every new modifier type required touching multiple systems.

The unified approach means:
- Adding a new effect type requires no changes to existing effects or the bank itself.
- Any entity with a `ModifierBank` can receive any stat modification.
- Cleanup is automatic — despawning an effect instance removes its contribution.

## The Three Layers

### Layer 1: Effect Instance Entities

An effect instance is a lightweight ECS entity. Its core components are:

- **`EffectTarget(Entity)`** — relationship to the entity being modified. The target entity
  gets an `EffectInstances` inverse relationship with `linked_spawn`, so despawning the
  target cascade-despawns all its effect instances.
- **`ModifierContributions(HashMap<ModifierType, f32>)`** — what stats this effect
  contributes and by how much. A single effect can contribute to multiple stats.

Lifecycle is controlled by optional, composable additional components:

| Component | Purpose |
|-----------|---------|
| `ExpiresAt(f64)` | Despawn at this absolute `GameClock` time |
| `EffectSource(Entity)` | Cascade-despawn when the source entity despawns |
| Custom markers | Any condition, managed by a dedicated system |

These compose freely. A temporary aura uses `EffectSource` + `ExpiresAt`. A fire-and-forget
debuff uses only `ExpiresAt`. An indefinite aura uses only `EffectSource`.

### Layer 2: ModifierBank

A component on any entity that has stats. Stores contributions keyed by effect instance
entity, grouped by `ModifierType`:

```
ModifierBank on a Wisp:
    MovementSpeed:
        entity_3 (baseline)    → 60.0
    IncomingDamageMultiplier:
        entity_8 (Brittle, A)  → 1.5
        entity_12 (Brittle, B) → 1.3
```

The bank has no concept of where contributions come from — that is the effect instance's
concern. The bank is an internal cache; no gameplay system reads or writes it directly.

When an effect instance's `ModifierContributions` are inserted or removed, observers on the
bank update the entries and immediately re-aggregate and materialize the affected stats.

### Layer 3: Derived Components

Immutable stat components populated by the bank's materialization step:

```
MaxHealth(f32)          MovementSpeed(f32)      AttackSpeed(f32)
AttackDamage(f32)       AttackRange(f32)        EnergySupplyRange(f32)
IncomingDamageMultiplier(f32)
```

Because these are `#[component(immutable)]`, Bevy treats any value change as a remove +
insert, which fires `On<Insert>` observers automatically. Systems that need to react to stat
changes observe the derived component directly.

## Stat Aggregation

Each `ModifierType` variant defines how multiple contributions combine:

| Stat | Rule | Identity | Semantics |
|------|------|----------|-----------|
| AttackRange, AttackDamage, etc. | Sum | 0.0 | Flat bonuses stack additively |
| IncomingDamageMultiplier | Max | 1.0 | Worst active debuff wins |

The identity value is the fold starting point, so an empty contributor set naturally produces
the correct "no effect" value.

**Aggregation vs. application:** The bank only provides a number. How it is used in a
formula is the caller's responsibility. `IncomingDamageMultiplier` uses Max aggregation to
select the strongest active stack, and weapon systems multiply damage by it.

## Baseline Effects

An entity's starting stats come from a permanent effect instance spawned at entity creation.
For buildings, the values come from the `Almanach` (centralized metadata registry). For
wisps, they are defined in the wisp builder.

Baseline effects:
- Target the entity itself via `EffectTarget(self_entity)`
- Carry a `BaselineEffect` marker component
- Have no `ExpiresAt` or `EffectSource` — they are permanent
- Are never saved; they are reconstructed when the entity spawns or loads
- Are spawned using the `related!` macro through the `EffectInstances` relationship

## Game Clock and Expiry

Timed effects reference absolute game time, not countdowns. The `GameClock` resource tracks
elapsed game-time seconds (advances only while `GameState::Running`).

An `EffectsExpiryQueue` (min-heap) holds `(expires_at, entity)` pairs. Each frame the head
is checked; expired entries trigger entity despawn. Most frames this is O(1).

Entities removed early (e.g., target despawned) are handled by the tombstone pattern — the
queue entry is ignored when popped if the entity no longer exists.

## Lifecycle Flow

**Spawning an effect:**
```
spawn effect instance with (EffectTarget, ModifierContributions, ...)
  → On<Insert, ModifierContributions> fires
  → observer updates ModifierBank, re-aggregates, materializes derived components
  → On<Insert, ExpiresAt> fires (if timed) → pushed to ExpiryQueue
```

**Effect expiring:**
```
ExpiryQueue pops entry → despawn effect instance
  → On<Remove, ModifierContributions> fires
  → observer removes from bank, re-aggregates, materializes
```

**Target entity despawned:**
```
target despawned
  → EffectInstances linked_spawn cascades → all effect instances despawned
  → their On<Remove> observers fire (bank entries cleaned up, though the bank
    itself is also being despawned)
```

## Save / Load

- **GameClock**: saved and restored as a global resource. Must load before effect instances.
- **Timed effects** (those with `ExpiresAt`): saved via their own `Builder*` component,
  loaded in `MapLoadingStage::SpawnEffectInstances` (after map elements).
- **Baseline effects**: never saved — reconstructed at entity spawn.
- **Source-coupled effects** (no `ExpiresAt`): not saved — re-applied by the system that
  manages them when the source entity is reloaded.

## Adding a New Effect Type

1. Define a marker component (e.g., `struct SlowEffect`).
2. Choose lifecycle: `ExpiresAt`, `EffectSource`, custom marker + system, or a combination.
3. At the application site, `commands.spawn(...)` the effect entity with `EffectTarget`,
   `ModifierContributions`, the marker, and any lifecycle components.
4. If custom condition: write one lifecycle system that queries the marker and despawns when
   the condition is no longer met.
5. If the effect has `ExpiresAt` and must survive save/load: add a `Builder*` following the
   standard persistence pattern, with a DB migration for the effect table.

Nothing in `ModifierBank`, `ModifierType`, or existing effect types changes.

## Key Files

| File | Contents |
|------|----------|
| `lib-core/src/effects.rs` | Effect relationships, `ExpiresAt`, expiry queue |
| `lib-core/src/modifiers.rs` | `ModifierType`, `ModifierBank`, derived stat components |
| `lib-core/src/game_clock.rs` | `GameClock` resource, save/load |
| `lib-inventory/src/effects/brittle.rs` | Brittle debuff (first concrete effect type) |
