use bevy::prelude::*;
use bevy_egui::egui;
use strum::IntoEnumIterator;

use almanach::prelude::Almanach;
use game_core::prelude::{DisplayDescription, DisplayIcon, DisplayName, Shard, ShardTier, ShardType};
use logging::prelude::*;
use outcomes::prelude::*;
use persistence::{prelude::*, rusqlite};
use shards::{
    blueprints::{ShardBlueprintAcquired, ShardBlueprints},
    prelude::UnlockShardBlueprint,
};
use states::prelude::MapLoadingStage;

pub(crate) struct ShardOutcomesPlugin;
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
    let Some(&UnlockShardBlueprint(mut selected)) = entity.get::<UnlockShardBlueprint>() else { return };
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
    // A blueprint covers every tier of its type; the T1 entry represents the type.
    let info = almanach.get_resource_info(Shard::new(unlock.0, ShardTier::T1));
    commands.entity(entity).insert((
        DisplayName(format!("Unlock {} Shard Blueprint", unlock.0)),
        DisplayDescription(info.description.clone()),
        DisplayIcon(info.icon.clone()),
        OutcomeEditorUi(ui_unlock_shard_blueprint_editor),
    ));
}

#[log_tags(Tag::Shards)]
fn on_fulfill_outcome_unlock_shard_blueprint(
    trigger: On<FulfillOutcome>,
    mut commands: Commands,
    mut blueprints: ResMut<ShardBlueprints>,
    outcomes: Query<&UnlockShardBlueprint>,
) {
    let outcome = trigger.event().outcome;
    let Ok(unlock) = outcomes.get(outcome) else { return };
    let shard_type = unlock.0;
    #[info_player("{shard_type} shard blueprint unlocked")]
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
#[log_tags(Tag::GameSave)]
fn collect_unlock_shard_blueprint_outcomes(
    outcomes: Query<(Entity, &UnlockShardBlueprint, &OutcomeOf)>,
    mut save: SaveWriter,
) {
    struct Snapshot {
        id: u32,
        parent_id: u32,
        shard_type: ShardType,
    }

    #[debug_dev("Saving {} unlock shard blueprint outcomes", snapshots.len())]
    let snapshots: Vec<Snapshot> = outcomes
        .iter()
        .map(|(entity, unlock, outcome_of)| Snapshot {
            id: entity.index_u32(),
            parent_id: outcome_of.0.index_u32(),
            shard_type: unlock.0,
        })
        .collect();

    if snapshots.is_empty() { return; }

    save.submit(move |ctx| {
        for snap in &snapshots {
            ctx.register_entity(snap.id)?;
            ctx.register_entity(snap.parent_id)?;
            ctx.tx.execute(
                "INSERT OR REPLACE INTO unlock_shard_blueprint_outcomes (id, parent_id, shard_type) VALUES (?1, ?2, ?3)",
                rusqlite::params![snap.id, snap.parent_id, snap.shard_type.as_ref()],
            )?;
        }
        Ok(())
    });
}

fn load_unlock_shard_blueprint_outcomes(ctx: &mut LoadContext) -> LoadResult {
    ctx.for_each_entity("SELECT id, parent_id, shard_type FROM unlock_shard_blueprint_outcomes", |ctx, _, entity, row| {
        let parent_old_id: u32 = row.get(1)?;
        let shard_type = row.get_parsed::<ShardType>(2)?;
        let parent = ctx.entity(parent_old_id)?;
        ctx.insert(entity, (
            OutcomeOf(parent),
            UnlockShardBlueprint(shard_type),
        ));
        Ok(())
    })
}
