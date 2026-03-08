# Shard System

Shards are the tower customization mechanic. Players collect shards and socket them into
tower slots, granting stat bonuses or new behaviors. The same shard type can produce
entirely different effects on different tower types — a Fury shard might grant flat damage
on a Blaster but splash damage on a Cannon.

## Core Concepts

**Shards are labels, not entities.** A shard has no mutable state of its own. `ShardType`
is a simple enum. The inventory tracks counts per type; towers track which types are
socketed.

**Targets own the reaction.** When a shard is socketed, the target decides
what happens. Each tower type registers a per-entity observer that receives the shard type
and spawns the appropriate effect. This gives each tower full type-system freedom: it can
modify stats, insert behavioral components, spawn child entities, or do nothing at all.

**Effects are ordinary effect entities.** Shard effects use the same `EffectTarget` +
`ModifierContributions` pattern as baseline effects, brittle effects, etc. A `ShardEffect`
marker distinguishes them for removal queries and save/load filtering. No new stat pipeline
or aggregation — shards plug into the existing modifier system.

## How It Works

1. **Player sockets a shard** — the UI validates preconditions (inventory has the shard,
   tower has a free slot), then commits both the inventory deduction and the slot insertion
   atomically. No rollbacks.

2. **Slot insertion triggers an event** — `ShardSlots::insert` pushes the shard type and
   fires `ShardApplyEvent` on the tower entity in one operation, ensuring the two are never
   separated.

3. **Tower observer reacts** — the tower's per-entity observer matches on the shard type
   and spawns the right effect entity (stat-only via `ShardEffect::from_modifiers`, or
   custom inline code for behavioral effects).

4. **Stat pipeline picks it up** — the existing modifier aggregation re-derives the tower's
   stats from all effect entities, including the new shard effect. No shard-specific code
   in the stat pipeline.

## Extending the System

### Adding a shard type

Add a variant to `ShardType`, then add match arms in each tower's `on_shard_apply` that
should accept it. Towers without a match arm are automatically incompatible — no extra code.

### Adding a behavioral effect

Define a marker or data component (e.g., `SplashDamage { radius: f32 }`), have the
tower's observer insert it on the effect entity or the tower itself, and query it in the
relevant gameplay system. New behavior = new component + one system. No central dispatch.

### Stacking

Duplicate stat shards stack naturally through the modifier system (each effect entity
contributes independently). Behavioral components follow Bevy's insert semantics
(last-write-wins by default). If merging or prevention is needed, the observer or UI can
handle it per-case.
