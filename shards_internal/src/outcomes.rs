use bevy::prelude::*;
use bevy_egui::egui;
use strum::IntoEnumIterator;

use almanach::prelude::Almanach;
use game_core::prelude::{DisplayDescription, DisplayIcon, DisplayName, ShardType};
use logging::prelude::*;
use outcomes::prelude::*;
use persistence::prelude::{AppGameLoadSaveExtension, CollectSave, GameDbHelpers, LoadContext, SaveWriter};
use persistence::rusqlite;
use shards::blueprints::{ShardBlueprintAcquired, ShardBlueprints};
use shards::prelude::UnlockShardBlueprint;
use states::prelude::MapLoadingStage;

pub struct ShardOutcomesPlugin;
impl Plugin for ShardOutcomesPlugin {
    fn build(&self, app: &mut App) {
        app
            .register_outcome_kind("Unlock Shard Blueprint", spawn_unlock_shard_blueprint_outcome)
            .add_observer(on_insert_unlock_shard_blueprint_derive_display)
            .add_observer(on_fulfill_outcome_unlock_shard_blueprint)
            .add_systems(CollectSave, collect_unlock_shard_blueprint_outcomes)
            .register_loader(MapLoadingStage::SpawnMapElements, "unlock_shard_blueprint_outcomes", load_unlock_shard_blueprint_outcomes);
    }
}

/// Entry point for the editor's "Add Outcome" menu.
fn spawn_unlock_shard_blueprint_outcome(commands: &mut Commands, parent: Entity) {
    commands.spawn((
        OutcomeOf(parent),
        UnlockShardBlueprint(ShardType::default()),
    ));
}

/// Editor UI for `UnlockShardBlueprint`: a `ShardType` dropdown. Changing
/// it inserts a new `UnlockShardBlueprint` (immutable, so insert is the only
/// way), which fires the derive observer to re-derive display.
fn ui_unlock_shard_blueprint_editor(ui: &mut egui::Ui, entity: &mut EntityWorldMut) {
    let current = entity.get::<UnlockShardBlueprint>().map(|u| u.0);
    let Some(mut selected) = current else { return };
    let id = entity.id();
    let response = egui::ComboBox::from_id_salt(format!("shard_type_{id:?}"))
        .selected_text(selected.to_string())
        .show_ui(ui, |ui| {
            for shard_type in ShardType::iter() {
                ui.selectable_value(&mut selected, shard_type, shard_type.to_string());
            }
        });
    if response.response.changed() {
        entity.insert(UnlockShardBlueprint(selected));
    }
}

fn on_insert_unlock_shard_blueprint_derive_display(
    trigger: On<Insert, UnlockShardBlueprint>,
    mut commands: Commands,
    almanach: Res<Almanach>,
    outcomes: Query<&UnlockShardBlueprint>,
) {
    let entity = trigger.entity;
    let Ok(unlock) = outcomes.get(entity) else { return };
    let info = almanach.get_shard_info(unlock.0);
    commands.entity(entity).insert((
        DisplayName(format!("Unlock {} Shard Blueprint", info.name)),
        DisplayDescription(info.description.clone()),
        DisplayIcon(info.icon.clone()),
        OutcomeEditorUi(ui_unlock_shard_blueprint_editor),
    ));
}

fn on_fulfill_outcome_unlock_shard_blueprint(
    trigger: On<FulfillOutcome>,
    mut commands: Commands,
    mut blueprints: ResMut<ShardBlueprints>,
    outcomes: Query<&UnlockShardBlueprint>,
) {
    let outcome = trigger.event().outcome;
    let Ok(unlock) = outcomes.get(outcome) else { return };
    let shard_type = unlock.0;
    if blueprints.unlock(shard_type) {
        commands.trigger(ShardBlueprintAcquired(shard_type));
    }
}

// ============================================================================
// Persistence
// ============================================================================

/// Collects all `UnlockShardBlueprint` outcomes. Every outcome's parent is
/// map content (researches are `MapBound`), so all outcomes are saved.
/// Display data is not saved; it is derived from `ShardType` on load via
/// the observer above.
fn collect_unlock_shard_blueprint_outcomes(
    outcomes: Query<(Entity, &UnlockShardBlueprint, &OutcomeOf)>,
    mut save: SaveWriter,
) {
    struct Snapshot {
        id: i64,
        parent_id: i64,
        shard_type: String,
    }

    let snapshots: Vec<Snapshot> = outcomes
        .iter()
        .map(|(entity, unlock, outcome_of)| Snapshot {
            id: entity.index_u32() as i64,
            parent_id: outcome_of.0.index_u32() as i64,
            shard_type: unlock.0.to_string(),
        })
        .collect();

    if snapshots.is_empty() { return; }

    save.submit(move |tx| {
        for snap in &snapshots {
            tx.register_entity(snap.id)?;
            tx.register_entity(snap.parent_id)?;
            tx.execute(
                "INSERT OR REPLACE INTO unlock_shard_blueprint_outcomes (id, parent_id, shard_type) VALUES (?1, ?2, ?3)",
                rusqlite::params![snap.id, snap.parent_id, snap.shard_type],
            )?;
        }
        Ok(())
    });
}

fn load_unlock_shard_blueprint_outcomes(ctx: &mut LoadContext) -> rusqlite::Result<()> {
    let mut stmt = ctx.conn.prepare(
        "SELECT id, parent_id, shard_type FROM unlock_shard_blueprint_outcomes",
    )?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let old_id: i64 = row.get(0)?;
        let parent_old_id: i64 = row.get(1)?;
        let shard_type_str: String = row.get(2)?;

        let Some(entity) = ctx.entity(old_id) else {
            Log::warn().dev().tag(Tag::GameLoad).message(format!("UnlockShardBlueprint outcome with old ID {old_id} has no corresponding new entity"));
            continue;
        };
        let Some(parent) = ctx.entity(parent_old_id) else {
            Log::warn().dev().tag(Tag::GameLoad).message(format!("UnlockShardBlueprint outcome with old ID {old_id} references parent {parent_old_id} that failed entity remap"));
            continue;
        };
        let Ok(shard_type) = shard_type_str.parse::<ShardType>() else {
            Log::warn().dev().tag(Tag::GameLoad).message(format!("Unknown shard type in save: {shard_type_str}"));
            continue;
        };

        ctx.insert(entity, (
            OutcomeOf(parent),
            UnlockShardBlueprint(shard_type),
        ));
    }
    Ok(())
}
