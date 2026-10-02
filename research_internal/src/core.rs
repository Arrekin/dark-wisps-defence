use bevy::{platform::collections::HashSet, prelude::*};

use almanach::prelude::Almanach;
use game_core::prelude::{ContentId, DisplayIcon};
use research::prelude::*;

use crate::ui::tile::ResearchTileOf;

/// Spawns a tile entity bound to the research when it's enabled (first
/// `ResearchState` insert). The `On<Add, ResearchTileOf>` observer builds
/// the UI.
pub(crate) fn on_add_research_state_spawn_tile(
    trigger: On<Add, ResearchState>,
    mut commands: Commands,
) {
    let research = trigger.entity;
    commands.spawn(ResearchTileOf(research));
    commands.trigger(ResearchDisplayDataUpdated { research });
}

/// The single entry point for marker components — they are never inserted
/// directly. Swaps them to match the newly inserted `ResearchState`.
pub(crate) fn on_insert_research_state_sync_markers(
    trigger: On<Insert, ResearchState>,
    mut commands: Commands,
    states: Query<&ResearchState>,
) {
    let entity = trigger.entity;
    let Ok(new_state) = states.get(entity) else { return };
    let mut entity_commands = commands.entity(entity);
    entity_commands.remove::<(ResearchAvailable, ResearchActive, ResearchCompleted)>();
    match new_state {
        ResearchState::Available => { entity_commands.insert(ResearchAvailable); }
        ResearchState::Active    => { entity_commands.insert(ResearchActive); }
        ResearchState::Completed => { entity_commands.insert(ResearchCompleted); }
    }
}

pub(crate) fn on_insert_display_icon_fire_research_display_data_updated(
    trigger: On<Insert, DisplayIcon>,
    mut commands: Commands,
    researches: Query<(), With<Research>>,
) {
    let entity = trigger.entity;
    if researches.contains(entity) {
        commands.trigger(ResearchDisplayDataUpdated { research: entity });
    }
}

/// Spawn every catalog research not yet on the map, out of scenario (no
/// `ResearchState`). Triggered by the `SeedResearches` event.
pub(crate) fn on_seed_researches_spawn_missing(
    _trigger: On<SeedResearches>,
    mut commands: Commands,
    almanach: Res<Almanach>,
    researches: Query<&ContentId, With<Research>>,
) {
    let existing: HashSet<&ContentId> = researches.iter().collect();
    for (id, spawn_fn) in almanach.researches.iter() {
        if !existing.contains(id) {
            spawn_fn(&mut commands, id);
        }
    }
}
