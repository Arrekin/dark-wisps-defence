# Shard System

Shards are the entity customization mechanic. Players collect shards and socket them into
slots, granting stat bonuses or new behaviors. The same shard type can produce entirely
different effects on different entity types — a Fury shard might grant flat damage on a
Blaster but splash damage on a Cannon.

## Core Concepts

**Shards are labels, not entities.** A shard has no mutable state of its own. `ShardType`
is a simple enum. The inventory tracks counts per type; slot holders track which types are
socketed.

**Targets own the reaction.** When a shard is socketed, the target decides what happens.
Each entity type registers a per-entity observer that receives the shard type and spawns
the appropriate effect. This gives each entity full type-system freedom: it can modify
stats, insert behavioral components, spawn child entities, or do nothing at all.

**Effects are ordinary effect entities.** Shard effects use the same `EffectTarget` +
`ModifierContributions` pattern as baseline effects, brittle effects, etc. A `ShardEffect`
marker distinguishes them for removal queries, save/load filtering, and UI. No new stat
pipeline or aggregation — shards plug into the existing modifier system.

## How It Works

1. **Player sockets a shard** — the UI validates preconditions (inventory has the shard,
   the slot is free), then commits both the inventory deduction and the slot insertion
   atomically. No rollbacks.

2. **Slot insertion triggers an event** — `ShardSlots::insert_at` writes the slot and
   fires `ShardApplyEvent` on the target entity in one operation, ensuring the two are
   never separated.

3. **Target observer reacts** — the entity's per-entity observer matches on the shard type
   and spawns the right effect entity (stat-only via `ShardEffect::from_modifiers`, or
   custom inline code for behavioral effects).

4. **Stat pipeline picks it up** — the existing modifier aggregation re-derives the
   entity's stats from all effect entities, including the new shard effect. No
   shard-specific code in the stat pipeline.

## Extending the System

### Adding a shard type

Add a variant to `ShardType`, then add match arms in each entity's `on_shard_apply_do_so` that
should accept it. Entities without a match arm are automatically incompatible — no extra
code.

### Adding a behavioral effect

Define a marker or data component (e.g., `SplashDamage { radius: f32 }`), have the
entity's observer insert it on the effect entity or the entity itself, and query it in the
relevant gameplay system. New behavior = new component + one system. No central dispatch.

### Stacking

Duplicate stat shards stack naturally through the modifier system (each effect entity
contributes independently). Behavioral components follow Bevy's insert semantics
(last-write-wins by default). If merging or prevention is needed, the observer or UI can
handle it per-case.

## Shard Catalog

### `Range` — +AttackRange

| Tower | Effect |
|-------|--------|
| Cannon | +2.0 range (15 → 17) |
| Blaster | +2.0 range (15 → 17) |
| Rocket Launcher | +2.0 range (30 → 32) |
| Emitter | +2.0 range (4 → 6) |
| Field | +2.0 range (7 → 9), force field radius grows |

### `Damage` — +AttackDamage

| Tower | Effect |
|-------|--------|
| Cannon | +15.0 damage (50 → 65) |
| Blaster | +2.0 damage (1 → 3) |
| Rocket Launcher | +15.0 damage (50 → 65) |
| Emitter | +2.0 damage (1 → 3) |
| Field | *(incompatible — no damage stat)* |

### `Speed` — +AttackSpeed

| Tower | Effect |
|-------|--------|
| Cannon | +0.15 speed (0.5 → 0.65) |
| Blaster | +1.0 speed (5 → 6) |
| Rocket Launcher | +0.1 speed (0.33 → 0.43) |
| Emitter | +0.05 speed (0.2 → 0.25) |
| Field | *(incompatible — no attack speed)* |
