# Research System

Research is how a scenario lets the player permanently change what the game allows. A research is a
piece of map content — a name, an icon, a price and a duration — that the player works on over time
and which grants something when it finishes.

It is not a building. It has no position, no power draw and no physical presence; it exists as an
entity the player interacts with through the research panel.

## Core Concepts

**A research is map content, not a global fact.** Every research lives on the map it belongs to and
is saved with it, so two scenarios can carry the same research with different prices, different
descriptions, or different rewards. Nothing about a research is shared between maps at runtime.

**The catalog creates researches; it does not describe them.** Code holds a set of *definitions* —
one per research, registered in the `Almanach` under a `ContentId`. A definition is a function that
spawns a complete research into a map. Once spawned, the research is on its own: editing a definition
changes what future maps get, never what an existing map already has. There is no reconciliation pass,
because a map's copy is the authority on itself.

**Being in the scenario is `ResearchState` being present.** A research the author has not enabled
carries no state and no runtime — it is on the map, but not part of the game. Every query that reads
state therefore skips it automatically, without anyone remembering to filter.

**At most one research is active.** Starting one parks whichever was running, keeping its progress.
There is no cancel and no refund: progress is destroyed only by completion.

**Pay-as-you-go.** Cost is consumed continuously as progress advances, not paid up front. A research
stalls when stock runs dry and resumes on its own when resources return, with no error and no state
change. How much has been paid is a pure function of how far the research has come, so stalling and
resuming need no bookkeeping.

**A research does not know what it grants.** It owns a collection of *outcome* entities, each a typed
leaf. On completion the research triggers fulfilment on every outcome and each one acts on itself.
Adding a new kind of reward means adding an outcome kind, never touching research.

## How It Works

### The entity

| Component | Meaning |
|---|---|
| `Research { cost, duration }` | The domain data. Present on every research, in the scenario or not. |
| `ContentId` | Authored identity, matching a catalog definition. Nothing branches on its value. |
| `DisplayName` / `DisplayDescription` / `DisplayIconSwitcher` | What the player sees. Generic vocabulary from `game_core`. |
| `ResearchState` | `Available`, `Active` or `Completed`. Present only when the research is part of the scenario. |
| `ResearchRuntime { progress }` | Progress data, carried by states that can progress. A completed research has none. |

`ResearchState` is immutable, so every transition is an insert, and an observer swaps the marker
components — `ResearchAvailable`, `ResearchActive`, `ResearchCompleted` — to match. Those markers are
the query surface and are never inserted by hand.

### Running one

`SetActiveResearch` starts a research or switches to it, parking the incumbent back to `Available`.
`StopResearch` parks the active one. Both are entity events, and both no-op on a research that cannot
accept them, so callers need no preconditions.

Each frame the active research advances by `delta / duration`, then:

1. Nothing moves unless every cost with units outstanding has at least one whole unit in stock.
2. The advance is clamped so it never crosses a cost threshold the stock cannot cover, and never
   below the current progress — earned progress is never lost.
3. Whole units crossed between the old and new progress are deducted.

Payment is `floor(progress × amount)` per cost, so a research pays exactly its price by the time it
completes, and *remaining* is `amount - floor(progress × amount)` — the number the panel shows.

At `1.0` the research becomes `Completed`, drops its runtime, and fires `ResearchFinished`. That event,
not the state insert, is what triggers outcomes — a restored save inserts `Completed` too, and
fulfilment must not run again on load.

### Outcomes

Outcomes are a generic mechanism living in the `outcomes` crate, not a research concept. An outcome is
a satellite entity attached to a parent through `OutcomeOf` / `HasOutcomes`; when the parent decides
its condition is met, it fires `FulfillOutcome` on each one and the outcome acts on itself. The
relationship uses `linked_spawn`, so outcomes die with their parent.

Each outcome *kind* lives in the domain that owns what it releases — `UnlockShardBlueprint` belongs to
shards, because shards own blueprint possession. A kind supplies its own spawn contract, its own
fulfilment observer, its own persistence, and its own editor UI.

### Saving

A research is saved whether or not it is in the scenario. `progress` and `state` are nullable and
travel together: both present for a research in the scenario, both null for one that is not. Costs use
the shared `costs` table; each outcome kind has a table of its own.

Saving as a scenario resets progress but keeps state, because an author may legitimately want a
research already completed at the start of a map, while a half-finished one carries no authoring
intent.

## The panel

The research panel is a full-screen view with three regions: a header, a band of two detail views, and
a grid of tiles.

A **tile** is the compact entry — icon, name, progress and an action button whose label states what
pressing it does (`Start`, `Switch`, `Resume`, `Stop`). A tile exists for as long as its research is in
the scenario: it is spawned when the research gains its state and dies with the research through a
relationship cascade. Nothing rebuilds the list.

A **detail view** shows one research in full. Each view is bound to a *marker component* rather than to
an entity: the active view shows whatever carries `ResearchActive`, the inspected one shows whatever
carries `ResearchUISelected`. Both markers sit on at most one research, so a view resolves its subject
without searching and empties itself when the marker moves. Adding a third view costs a marker, a
spawn and a registration.

The panel is spawned once and hidden when closed, and its live systems are gated to the open state —
nothing has to be correct while the panel is not visible.

## Extending the System

### Adding a research

Write a definition module in `research_internal/src/definitions/` that spawns the research as a scene
and register it:

```rust
pub fn spawn_fire_shard_recipe_research(commands: &mut Commands, id: &ContentId) {
    commands.spawn_scene(bsn! {
        Research {
            cost: {vec![Cost { resource_type: ResourceType::Essence(EssenceType::Fire), amount: 100 }]},
            duration: {Duration::from_secs(30)},
        }
        ContentId({id.0.clone()})
        DisplayName("Fire Shard Recipe")
        DisplayDescription("Unlocks the blueprint to forge Fire shards.")
        DisplayIconSwitcher("ui/shards/shard_fire.png")
        HasOutcomes [
            UnlockShardBlueprint({ShardType::Fire})
        ]
    });
}
```

Then register it with the `Almanach` under a stable `ContentId`. Existing maps are unaffected until an
author seeds the new entry from the editor.

A definition's outcomes are written in the same scene as its data, so nothing anywhere maps a research
to the outcomes it ought to have.

### Adding an outcome kind

Create the component that describes what is released, in the crate that owns the thing being released.
Give it a spawn contract for the editor, an observer that acts on `FulfillOutcome`, a collector and
loader for its own table, and an editor UI function for configuring it. Register the kind so the
editor's "add outcome" menu can offer it. Research needs no changes.

Configuration components should be `#[component(immutable)]`, so changing one is always an insert and
anything derived from it — display, in particular — can be rebuilt by an observer that cannot be
bypassed.

### Authoring a scenario

The editor's research tab lists every research on the map. From there an author can seed the entries a
map is missing from the catalog, add a research to the scenario or remove it, edit its name,
description, icon, cost and duration, and compose its outcomes. `ContentId` is fixed at creation:
it is what matches a research to its catalog entry, and never changes afterwards.

Removing a research from a scenario resets it to its catalog default rather than parking it — a
research outside the scenario carries no authored intent, so there is nothing to preserve.

## Invariants

- At most one research carries `ResearchActive`, maintained by the handler that sets it.
- `ResearchState` markers are only ever written by the observer that watches state inserts.
- A research with no outcomes still ticks and completes.
- Zero cost is legal and ticks freely; zero duration completes in one step, still paying its price.
- Fulfilment runs exactly once per completion, and never on load.
