# Modifiers and Effects

Effects determine the stats of towers, wisps, and other entities, including their starting values and changes from shards, auras, or debuffs. A tower's attack range and a wisp's movement speed both use this system.

Each effect instance contributes one or more stat values to a target. The target's `ModifierBank` aggregates those contributions by `ModifierType` and writes the resulting stat components for gameplay systems to read.

## How Stats Are Stored

### Effect Instances

An effect instance is a lightweight ECS entity. Its core components are:

- **`EffectTarget(Entity)`** links the effect to the entity whose stats it modifies.
  The target's `EffectInstances` relationship uses `linked_spawn`, so despawning the target
  also despawns its effects.
- **`ModifierContributions(HashMap<ModifierType, f32>)`** stores the effect's contribution
  to each stat. One effect can contribute to several stats.

Optional components control the effect's lifetime and associate it with a source:

| Component | Purpose |
|-----------|---------|
| `TimeBound` | Despawn when `GameClock` reaches `ends_at` |
| `EffectSource(Entity)` | Let the source's systems find and remove effects it spawned |
| Custom markers | Identify effects managed by a dedicated system |

A temporary effect can use both `EffectSource` and `TimeBound` to record its source and expiry.
An effect that needs only an expiry can use `TimeBound` alone. An indefinite aura uses
`EffectSource`, with cleanup handled by the source's systems.

### ModifierBank

Each entity with stats has a `ModifierBank` component. It groups contributions by `ModifierType`
and identifies each contribution by its effect entity:

```text
ModifierBank on a Wisp:
    MovementSpeed:
        entity_3 (baseline)    → 60.0
    IncomingDamageMultiplier:
        entity_8 (Brittle, A)  → 1.5
        entity_12 (Brittle, B) → 1.3
```

The bank caches contributions; effect instances own their source and lifetime information.
Gameplay systems read the derived stat components.

Observers of `ModifierContributions` insertion and removal update the target's bank,
recalculate the affected totals, and queue replacements for the corresponding stat components.

### Derived Stat Components

The bank writes each total to an immutable component:

```text
MaxIntegrityPoints(f32)         MovementSpeed(f32)     AttackSpeed(f32)
AttackDamage(f32)               MinAttackRange(f32)    MaxAttackRange(f32)
IncomingDamageMultiplier(f32)   EnergySupplyRange(f32) TurnSpeed(f32)
```

These components use `#[component(immutable)]`, so updates insert replacements rather than
mutating values in place. Systems that react to stat changes observe `On<Insert<Stat>>`.

`MinAttackRange` and `MaxAttackRange` store inclusive distance limits in cells.
`AttackRangeType` selects how to measure that distance: cardinal grid steps from the nearest
imprint cell, or a circular radius from the area's world centre. `AttackAreaRange` queries
the grid area, distance metric, and limits together to construct an `AreaRange`.

## Stat Aggregation

Each `ModifierType` variant defines how multiple contributions combine:

| Stat | Rule | Identity | Semantics |
|------|------|----------|-----------|
| AttackSpeed, MinAttackRange, MaxAttackRange, AttackDamage, MaxIntegrityPoints, MovementSpeed, EnergySupplyRange, TurnSpeed | Sum | 0.0 | Flat contributions add together |
| IncomingDamageMultiplier | Max | 1.0 | Worst active debuff wins |

Aggregation starts from the identity value, which is also the result when no effects contribute.

Gameplay systems decide how to apply each total. For example, `IncomingDamageMultiplier`
takes the largest contribution, with a minimum of 1.0, and weapon systems multiply damage by it.

## Baseline Effects

An entity's starting stats come from a permanent effect instance spawned at entity creation.
For buildings, the values come from the `Almanach` (centralized metadata registry). For
wisps, they are defined in the wisp builder.

Baseline effects:

- Target the entity itself via `EffectTarget(self_entity)`
- Carry a `BaselineEffect` marker component
- Have no `TimeBound` or `EffectSource`
- Are never saved; they are reconstructed when the entity spawns or loads

## Game Clock and Expiry

Timed effects use `TimeBound` from `session` to store their lifetime in absolute `GameClock`
seconds. The clock advances only while `GameState::Running`.

`session_internal` queues `(ends_at, entity)` pairs in a min-heap and processes due entries
in `PostUpdate`. When nothing is due, checking the queue takes O(1).

Removing or replacing a `TimeBound` leaves its old queue entry in place. When that entry is
due, it despawns the entity only if the entity still has a `TimeBound` with the queued end time.

## Lifecycle Flow

**Spawning an effect:**

```text
spawn effect instance with (EffectTarget, ModifierContributions, ...)
  → On<Insert<ModifierContributions>> fires
  → observer updates ModifierBank and queues recalculated stat components
  → On<Insert<TimeBound>> fires (if timed) → pushed to the expiry queue
```

**Effect expiring:**

```text
expiry queue pops entry → despawn effect instance
  → On<Remove<ModifierContributions>> fires
  → observer removes contributions from ModifierBank and queues recalculated stat components
```

**Target leaves a source field:**

```text
FieldExited fires on the field
  → field observer uses EffectSourceOf to find and despawn FieldEffect instances on that target
  → On<Remove<ModifierContributions>> removes their values from the target's ModifierBank
  → affected stat components are recalculated
```

`EffectSource` supports lookups. Each source's systems are responsible for removing its effects.
