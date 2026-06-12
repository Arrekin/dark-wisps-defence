# Research System

Research is the mechanism for permanently changing what the game allows. The player spends
resources and time to unlock blueprints, buildings, or other capabilities.

## Core Concepts

**Research is not a building.** It has no position, power requirement, or physical presence.
It exists as a data entry the player interacts with through a panel.

**At most one research is active at a time.** Starting a research parks any currently active
one, preserving its progress. There is no cancel and no refund; progress is only ever
destroyed by completion.

**Pay-as-you-go.** Cost is consumed continuously as progress advances. A research may start
before its full cost is affordable; it stalls when stock runs dry and resumes automatically
when resources return. The amount paid is a pure function of progress.

**Outcomes are child entities.** A research does not know what it grants. It owns a collection
of outcome child entities, each a typed leaf. On completion, the research triggers fulfillment
on each child, and the child handles its own grant. This keeps the research generic: adding a
new grant type means adding a new outcome leaf, not modifying the completion code.

**Obsolescence is not completion.** A research becomes obsolete when all its outputs are already
owned through some other means. An obsolete research is hidden and cannot be started, but it
remains completed if it was ever finished. Completion is a permanent historical fact; obsolescence
is a reversible derived state.

**The panel renders display projections, never domain types.** Each research carries a card
projection and each outcome carries a display projection. The panel reads these and lifecycle
markers only. It never imports outcome kinds or possession lanes.

## Lifecycle

### Fresh Map

1. **Definitions spawn** — one entity per research type, carrying the spec (cost, duration) and
   its default outcome children.
2. **Modifiers compose** — optional systems observe the spawn event and attach extra outcomes or
   components to authored researches (map-specific rewards, difficulty adjustments).
3. **Obsolescence derives** — each outcome checks whether its output is already owned and marks
   itself satisfied if so. When all outcomes of a research are satisfied, the research becomes
   obsolete.

### Runtime

1. **Player opens the panel** — visible researches are those that are not completed and not obsolete.
2. **Start** — the selected research becomes active and gains an in-flight progress component.
3. **Tick** — the active research drains resources from stock in proportion to elapsed time.
   Progress stalls at the first resource unit the stock cannot cover.
4. **Completion** — at progress `1.0`, the research removes its active and progress markers,
   inserts a permanent completed marker, and triggers fulfillment on every outcome child.
   Each outcome grants into its own possession lane.
5. **Obsolescence** — the granted blueprint triggers the satisfaction reaction on matching
   outcomes, which in turn marks the research obsolete if all its outcomes are satisfied.
   The research card disappears from the panel.

### Switching and Stopping

- **Switch** — starting research B while A is active parks A (progress retained) and activates B.
- **Stop** — removes the active marker; progress is retained. The research can be resumed later.
- **No cancel** — there is no operation that destroys progress or refunds spent resources.

### Save and Load

Research instances and their outcomes are saved as entities. Possession of granted blueprints is
saved by the possession lanes, not by the research system.

On load, the definition entities are restored first, then in-flight state (progress fraction and
active flag) is overlaid onto the matching research entities. Outcomes are restored from their own
table and re-linked to their research. Fulfillment is never re-triggered on load; the saved
`Completed` marker is authoritative.

## Extending the System

### Adding a New Research

Define a research type, register a static definition (name, icon, cost, duration, outcomes),
and add it to the taxonomy. On a fresh map, `seed_research` instantiates any research not
already present, so new types appear automatically.

```rust
// Example: activating a research programmatically
commands.trigger(SetActiveResearch(ResearchType::WaterShardRecipe));
```

### Adding a New Outcome Kind

Create a marker component for the new grant type and attach it to outcome child entities.
Implement an `on_add` observer that derives an `OutcomeDisplay` projection and wires a
fulfillment observer. On fulfillment, grant into the appropriate possession lane and announce
the acquisition so matching outcomes can mark themselves satisfied.

Register a saver and loader for the new outcome table. No changes to the research tick or
completion code are needed.

### Gating (Future)

Research visibility and startability are governed by two independent gates:

1. **Gate A (possession)** — the player must hold the research's own blueprint. This is handled
   by a possession lane (mirroring shard blueprints).
2. **Gate B (conditions)** — authored preconditions such as wave count, existing buildings, or
   other research completion.

A research is visible when gate A holds, and startable when both gates hold. The panel renders
conditions as a checklist with met/unmet styling.
