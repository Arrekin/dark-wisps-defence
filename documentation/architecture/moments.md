# Moments

A moment is a scenario-relevant point in time, represented as an entity: "the game started",
"this objective was satisfied", "this wave ran dry". Anything a map author might want to
observe is a moment.

As an example, a scenario can link an objective's failure moment to an entity that should react to it. When the objective fails, the moment sends `MomentHappened` to its watchers. The objective doesn't need to know who watches it, and watchers don't need to know the objective's failure event.

## How It Works

For a moment owned by another entity:

1. **Spawn the moment child** — `world.spawn((MomentOf(parent), SomeMomentKind))`.

2. **Listen to the parent's event** — an `On<Add<Kind>>` observer reads `MomentOf` and
   registers a listener on the parent for the domain event that signals the moment.

3. **Fire the moment** — the parent emits its domain event; the listener increments `fired_count`
   and triggers `MomentHappened` on the moment entity.

4. **Notify watchers** — the generic propagator walks `MomentWatchers` and triggers
   `MomentHappened` on each watcher.

5. **React in the watcher's domain** — observers handle `MomentHappened` on their own entities.

## Invariants

- Moment kind types use the `Moment*` prefix. The derive turns the type name into a persistence key, so renaming a kind changes the save format.
- Do not make a moment watch another moment. `MomentHappened` forwarded to a watching moment propagates again without incrementing its `fired_count`; a watch cycle can forward indefinitely.
- Scenario saves reset `fired_count` and preserve watch links. Playthrough saves preserve both.

## Extending the System

### Adding a moment kind

Next to the domain's components:

```rust
#[derive(Component, Default, MomentKind)]
#[require(Moment, Name = Name::new("Summoning Exhausted"))]
pub struct MomentSummoningExhausted;
```

In the domain's plugin:

```rust
.add_observer(moment_attach_self_trigger_to_parent::<MomentSummoningExhausted, SummoningExhaustedEvent>)
.register_moment_persistence::<MomentSummoningExhausted>()
```

Then give authoring a way to spawn it — for example, in the editor, a checkbox that spawns
`(MomentOf(parent), MomentSummoningExhausted)` and despawns the existing child when cleared.

### Reacting to a moment

Give the watcher a `MomentOfInterest` pointing at the moment, then attach an observer for the entity-targeted `MomentHappened` event:

```rust
commands.entity(watcher)
    .insert(MomentOfInterest(moment))
    .observe(on_moment_happened);
```

For shared behavior across a domain, a global observer is also possible. It receives `MomentHappened` for every target, so it must check that `trigger.entity` belongs to the domain before acting, as the objective handler does with `objectives.contains(trigger.entity)`.

Decide explicitly what losing the moment means and observe `On<Remove<MomentOfInterest>>` for it.
Objectives treat a lost moment as failure, because an objective that can never start is a scenario
error worth surfacing.

### Standalone moments

A moment with no natural owner is its own parent:

```rust
let entity = commands.spawn_empty().id();
commands.entity(entity).insert((MomentOf(entity), MomentGameStart));
```
