# Research

Research lets players unlock lasting changes during a scenario.

Core concepts:
- Research is enabled/disabled per scenario
- Research is customizable per scenario. Each scenario can have its own version of a given research.
- Research grants its effects via Outcomes
- At most one research is active
- Player can switch between researches without losing progress
- Resources spent on research are non-refundable.
- **Pay-as-you-go.** Cost is consumed continuously; progress stalls when stock runs dry and automatically resumes when resources appear.
- Creators can add new researches beyond the built-in ones. They are saved in the map file and remain scenario-specific.

## How It Works

### Entity Composition

| Component                                                    | Meaning                                                                                       |
| ------------------------------------------------------------ | --------------------------------------------------------------------------------------------- |
| `Research { cost, duration }`                                | The domain data. Present on every research, in the scenario or not.                           |
| `ContentId`                                                  | Authored identity.                                                                            |
| `DisplayName` / `DisplayDescription` / `DisplayIconSwitcher` | What the player sees.                                                                         |
| `ResearchState`                                              | `Available`, `Active` or `Completed`. Present only when the research is part of the scenario. |
| `ResearchRuntime { progress }`                               | Progress data, carried by states that can progress. A completed research has none.            |

`ResearchState` is immutable. Inserting it automatically manages dedicated marker components: `ResearchAvailable`, `ResearchActive`, `ResearchCompleted`.

### Lifecycle

`SetActiveResearch` request event: 
- changes an `Available` research to `Active`. 
- If another research was active at that time, it returns to `Available` with its progress intact. 

`StopResearch` returns the active research to `Available`.

While `Active`, a research advances over its configured duration. Stock is charged in whole units as progress advances. When the next payment cannot be made, progress stops at that boundary until stock is replenished. On completion, the research becomes `Completed`, its progress runtime is removed, and `ResearchFinished` triggers its outcomes.

### Saving

Every research entity is saved, including those without `ResearchState` that are outside the scenario. Playthrough saves keep state and progress. Scenario saves keep the authored state but discard progress; an `Available` or `Active` research starts at zero progress when that scenario loads.

## Extending the System

### Adding a research

Write a definition module in `research_internal/src/definitions/` that spawns the research as a scene:

```rust
pub fn spawn_fire_shard_recipe_research(commands: &mut Commands, id: &ContentId) {
    commands.spawn_scene(bsn! {
        Research {
            cost: {vec![ResourceAmount::new(EssenceType::Fire, 100)]},
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

Then register it with the `Almanach` under a stable `ContentId`. Existing maps are unaffected until an author seeds the new entry from the editor.
