# Modifiers & Effects

Effects determine the stats of towers, wisps, and other entities, including their starting values and changes from shards, auras, or debuffs. A tower's attack range and a wisp's movement speed both use this system.

Each effect instance contributes one or more stat values to a target. The target's `ModifierBank` aggregates those contributions by `ModifierType` and writes the resulting stat components for gameplay systems to read.

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
| `EffectSource(Entity)` | Link to the source so its domain can find and clean up the effects it spawned |
| Custom markers | Any condition, managed by a dedicated system |

These compose freely. A temporary aura uses `EffectSource` + `ExpiresAt`. A fire-and-forget
debuff uses only `ExpiresAt`. An indefinite aura uses `EffectSource` and source-specific cleanup.

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
MaxIntegrityPoints(f32) MovementSpeed(f32)      AttackSpeed(f32)
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
  → On<Insert<ModifierContributions>> fires
  → observer updates ModifierBank, re-aggregates, materializes derived components
  → On<Insert<ExpiresAt>> fires (if timed) → pushed to ExpiryQueue
```

**Effect expiring:**
```
ExpiryQueue pops entry → despawn effect instance
  → On<Remove<ModifierContributions>> fires
  → observer removes from bank, re-aggregates, materializes
```

**Source force field despawned:**
```
ForceField despawned
  → On<Despawn<ForceField>> reads EffectSourceOf to find its FieldEffect instances
  → domain observer despawns those effects
  → On<Remove<ModifierContributions>> removes their values from each target's ModifierBank
  → affected stat components are recalculated
```
`EffectSource` is a lookup relationship, not a despawn cascade; each source domain decides how to remove its effects.
