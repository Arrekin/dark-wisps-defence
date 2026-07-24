# Moments

A moment is a scenario-relevant point in time, represented as an entity: "the game started",
"this objective was satisfied", "this wave ran dry". Anything a map author might want to
observe is a moment.

Moments exist so that these facts are data, not code. A scenario can hang something off an
objective's failure without either system knowing the other exists, and without a shared
vocabulary of event names that both sides must spell identically.

## Core Concepts

**A moment informs; it does not command.** A moment states that something happened. It carries no
opinion about what should follow, and a moment with no watchers is not a mistake. Objectives and
summonings happen to use moments as their activation criterion, but that is those systems' choice,
not part of what a moment is. A watcher is free to end something, count something, log something,
or ignore the fact entirely.

**Moments are entities, not events.** An entity is referenceable from data: the editor can
enumerate the moments that exist on this map and offer them in a picker, saves store the link
as a plain entity id riding the standard remap, and adding a moment is spawning an entity
rather than declaring a type or extending an enum. The boundary — anything map-authored or
referenceable from a scenario is a moment; a truly global engine event with nothing pointing
at it from data stays an ordinary in-code event.

**Every moment has a parent.** `MomentOf` / `HasMoments`, with linked despawn: moments die
with the thing whose moments they are. Standalone moments are self-parented (`MomentOf(self)`),
so there is no parentless case and no special path through persistence or the despawn cascade.

**Watching is a reference, not ownership.** `MomentOfInterest` / `MomentWatchers`, with no
linked despawn. When a moment goes away, the relationship is removed from its watchers — firing
`On<Remove>` — but the watchers survive. What losing your moment means is each domain's own
policy, not the moment system's.

**The parent decides when.** There is no shared notion of what counts as "happening". The
owning domain fires its moment at whatever point it considers the fact true. In practice the
domain does not mention moments at all: it fires its own terminal event, and the moment child
registers a listener for that event on its parent when it is spawned.

**Firing state is universal.** Every moment carries `fired_count`. One-shot moments refuse to
re-fire once it reaches 1; repeatable moments keep incrementing. Because it is the one piece of
state every moment has, it lives on the shared moments table rather than in per-kind storage.

**Restoring is not firing.** Loading a save inserts `fired_count` directly and produces no side
effects. Firing is always an explicit act by the owning domain. This is the same
restoration-vs-transition discipline the objective and summoning state machines follow.

## How It Works

1. **Authoring spawns a moment child** — `world.spawn((MomentOf(parent), SomeMomentKind))`.
   The kind marker requires `Moment` and a `Name`, so the entity is complete on spawn.

2. **The child wires itself to its parent** — an `On<Add, Kind>` observer reads `MomentOf` and
   registers a listener on the parent for the domain event that means "this moment happened".
   The listener self-despawns lazily — only the next time the domain event fires after the moment
   is gone. If a moment is toggled off and on, the old listener lingers on the parent until the
   domain event fires again, at which point it discovers the moment is gone and removes itself.
   This is eventual cleanup, not immediate.

3. **The domain fires its own event** — the listener increments `fired_count` and triggers
   `MomentHappened` on the moment entity.

4. **The generic propagator fans out** — it walks `MomentWatchers` and triggers `MomentHappened`
   on each watcher.

5. **Watcher domains react** — each observes `MomentHappened` filtered to its own components and
   does whatever the fact means for it. Today both watchers treat it as a start signal: an
   objective routes it to its activate event, a summoning flips to `Active`. Nothing in the
   mechanism requires that reading.

The event target carries the addressing: `MomentHappened` on a moment means "I happened", and on
a watcher means "the moment you watch happened". Steps 3–5 are generic; steps 1–2 and 5 are the
only places a domain appears.

Persistence is handled generically — see `documentation/persistence.md` for the moments table and
the scenario-mode reset rules.

## Invariants

- **Kind markers are named `Moment*`.** The derive infers the persistence key from the type name
  and rejects names without the prefix. The key string is never written by hand, and renaming the
  type changes the key — a rename is a save format change.
- **Authoring spawns moment children; domains do not.** A moment exists because someone chose to
  expose it, not because its parent was created. Load restores children the same way, so nothing
  needs a duplicate guard.
- **A watcher holds at most one `MomentOfInterest`.** The propagated event carries no indication
  of *which* moment happened, so a second watch link would be ambiguous.
- **A moment should not itself be a watcher.** The propagator will deliver `MomentHappened` to it,
  but that path does not increment `fired_count` and does not guard against re-entry. Chaining
  belongs between a moment and a domain entity, not between two moments.
- **`fired_count` is playthrough state; the watch link is authoring.** Scenario saves reset the
  former and preserve the latter.

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

Then give authoring a way to spawn it — in the editor, a checkbox that spawns
`(MomentOf(parent), MomentSummoningExhausted)` and despawns the existing child when cleared.

The domain event in the observer registration is whatever the domain already fires when the fact
becomes true. If it does not fire one yet, add it — a terminal event is worth having on its own
terms, and it keeps the domain free of moment vocabulary.

### Reacting to a moment

Give the entity a `MomentOfInterest` pointing at any moment, then observe `MomentHappened`
filtered to your own components. Activation is the worked example below, but the shape is the
same whatever the reaction is:

```rust
fn on_moment_happened_activate(
    trigger: On<MomentHappened>,
    candidates: Query<&Thing, With<ThingInactive>>,
    mut commands: Commands,
) { /* ... */ }
```

The query filter is what makes the observer safe to register globally: it ignores every
`MomentHappened` that is not addressed to one of your entities in a state you care about. Pick the
filter that expresses your own precondition — `With<Inactive>` if the reaction is starting,
something else if it is not.

Decide explicitly what losing the moment means and observe `On<Remove, MomentOfInterest>` for it.
Objectives treat a lost moment as failure, because an objective that can never start is a scenario
error worth surfacing. That is a domain policy, not a rule — but a domain that chooses to do
nothing should at minimum log, so the situation is not silent.

### Standalone moments

A moment with no natural owner is its own parent:

```rust
let entity = commands.spawn_empty().id();
commands.entity(entity).insert((MomentOf(entity), MomentGameStart));
```

Its firing logic lives wherever the condition is observable — a state transition, a timer, a
region sensor. Something must guarantee exactly one exists per map before anything can watch it:
spawn it on map load when absent, and let the loader's restore take precedence when a save
already contains one.

One-shot standalone moments use `fire_if_not_yet_fired` rather than `fire`, so that reloading
mid-playthrough does not replay them.
