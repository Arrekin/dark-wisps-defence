use bevy::prelude::*;

use game_core::prelude::*;
use logging::prelude::*;
use persistence::{prelude::*, rusqlite};
use research::prelude::*;
use resources::prelude::ResourceAmount;

/// Saves every research on the map, enabled or not. `state` is nullable
/// (NULL = disabled); `progress` is nullable (NULL = no runtime, i.e. not-yet-
/// started or completed). Scenario saves drop runtime entirely: an in-progress
/// research saved as a scenario becomes Available with no progress — "reset
/// to not started." A completed one stays Completed with no runtime.
#[log_tags(Tag::GameSave)]
pub(crate) fn collect_researches(
    save_runner: Res<SaveRunner>,
    researches: Query<(
        Entity,
        &Research,
        &ContentId,
        &DisplayName,
        &DisplayDescription,
        &DisplayIconSwitcher,
        Option<&ResearchState>,
        Option<&ResearchRuntime>,
    )>,
    mut save: SaveWriter,
) {
    if researches.is_empty() { return; }

    struct Snapshot {
        id: u32,
        content_id: String,
        name: String,
        description: String,
        icon_path: String,
        duration_secs: f32,
        progress: Option<f32>,
        state: Option<ResearchState>,
        costs: Vec<ResourceAmount>,
    }

    #[debug_dev("Saving {} researches", snapshots.len())]
    let snapshots: Vec<Snapshot> = researches
        .iter()
        .map(|(entity, research, content_id, name, description, icon, state, runtime)| {
            // Scenario saves drop runtime: progress becomes NULL regardless of
            // current progress. Non-scenario saves keep it when present.
            let progress = if save_runner.save_as_scenario {
                None
            } else {
                runtime.map(|runtime| runtime.progress)
            };
            Snapshot {
                id: entity.index_u32(),
                content_id: content_id.0.clone(),
                name: name.0.clone(),
                description: description.0.clone(),
                icon_path: icon.0.clone(),
                duration_secs: research.duration.as_secs_f32(),
                progress,
                state: state.copied(),
                costs: research.cost.clone(),
            }
        })
        .collect();

    save.submit(move |ctx| {
        for snap in &snapshots {
            ctx.register_entity(snap.id)?;
            let cost_list_id = ctx.save_resource_list(&snap.costs)?;
            ctx.tx.execute(
                "INSERT OR REPLACE INTO researches (id, content_id, name, description, icon_path, duration_secs, progress, state, cost_list_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                rusqlite::params![
                    snap.id,
                    snap.content_id,
                    snap.name,
                    snap.description,
                    snap.icon_path,
                    snap.duration_secs,
                    snap.progress,
                    snap.state.as_ref().map(|state| state.as_ref()),
                    cost_list_id,
                ],
            )?;
        }
        Ok(())
    });
}

#[log_tags(Tag::GameLoad)]
pub(crate) fn load_researches(ctx: &mut LoadContext) -> LoadResult {
    ctx.for_each_entity("SELECT id, content_id, name, description, icon_path, duration_secs, progress, state, cost_list_id FROM researches", |ctx, old_id, entity, row| {
        let content_id: String = row.get(1)?;
        let name: String = row.get(2)?;
        let description: String = row.get(3)?;
        let icon_path: String = row.get(4)?;
        let duration_secs: f32 = row.get(5)?;
        let progress: Option<f32> = row.get(6)?;
        let state_str: Option<String> = row.get(7)?;
        let cost_list_id: i64 = row.get(8)?;
        let costs = ctx.resource_list(cost_list_id)?;

        // Insert core components (always present, enabled or not).
        ctx.insert(entity, (
            Research {
                cost: costs,
                duration: std::time::Duration::from_secs_f32(duration_secs),
            },
            ContentId(content_id),
            DisplayName(name),
            DisplayDescription(description),
            DisplayIconSwitcher(icon_path),
        ));

        // State is the enablement signal: present = enabled, NULL = disabled
        // (nothing more to insert).
        // Progress is optional even when state is present (not-yet-started or
        // completed). Insert state first; if progress is present, insert
        // runtime after — the require on ResearchAvailable would auto-insert
        // a default runtime, but we insert the saved one explicitly to
        // overwrite it.
        let Some(state_str) = state_str else { return Ok(()) };
        #[warn_dev("Research with old ID {old_id} has unknown state '{state_str}' — treating as disabled")]
        let Ok(state) = state_str.parse::<ResearchState>() else { return Ok(()) };
        ctx.insert(entity, state);
        if let Some(progress) = progress {
            ctx.insert(entity, ResearchRuntime { progress });
        }
        Ok(())
    })
}
