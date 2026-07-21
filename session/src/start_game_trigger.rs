use bevy::prelude::*;

use game_core::prelude::{MapBound, TriggerFired, TriggerSource};
use logging::prelude::*;
use persistence::prelude::*;
use persistence::rusqlite;
use states::prelude::{GameState, MapLoadingStage};

pub(crate) struct StartGameTriggerPlugin;
impl Plugin for StartGameTriggerPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(CollectSave, collect_start_game_trigger)
            .register_loader(MapLoadingStage::LoadResources, "trigger_start_game", load_start_game_trigger)
            .add_systems(OnEnter(GameState::Running), fire_start_game_once)
            ;
    }
}

/// StartGame trigger — data half. Firing logic in `fire_start_game_once`.
#[derive(Component)]
#[require(TriggerSource, MapBound)]
pub struct TriggerStartGame {
    pub fired: bool,
}

fn collect_start_game_trigger(
    save_ctx: Res<SaveContext>,
    mut save: SaveWriter,
    trigger: Single<(Entity, &TriggerStartGame)>,
) {
    let (entity, trigger) = *trigger;
    let id = entity.index_u32() as i64;
    let fired = if save_ctx.save_as_scenario { false } else { trigger.fired };
    save.submit(move |tx| {
        tx.register_entity(id)?;
        tx.execute(
            "INSERT OR REPLACE INTO trigger_start_game (id, fired) VALUES (?1, ?2)",
            rusqlite::params![id, if fired { 1 } else { 0 }],
        )?;
        Ok(())
    });
}

fn load_start_game_trigger(ctx: &mut LoadContext) -> rusqlite::Result<()> {
    // Singleton row: read whatever id it carries and remap — no shared id constant.
    let mut stmt = ctx.conn.prepare("SELECT id, fired FROM trigger_start_game")?;
    let mut rows = stmt.query([])?;
    let Some(row) = rows.next()? else {
        Log::warn().dev().tag(Tag::GameLoad).message("No StartGame trigger row in save");
        return Ok(());
    };
    let old_id: i64 = row.get(0)?;
    let fired: i64 = row.get(1)?;
    let Some(entity) = ctx.entity(old_id) else {
        Log::error().dev().tag(Tag::GameLoad).message(format!("StartGame trigger old ID {old_id} failed entity remap"));
        return Ok(());
    };
    ctx.insert(entity, TriggerStartGame { fired: fired != 0 });
    Ok(())
}

/// Fire `TriggerFired` on the StartGame trigger entity exactly once, when
/// `fired == false` and the game enters `Running` state. Sets `fired = true`
/// after firing. On mid-game reload, `fired == true` (restored from save) →
/// early return, no re-fire. Fires on `OnEnter(GameState::Running)` — strictly
/// after the `Running` state is applied, so goal activation observers (gated
/// on `Running`) don't skip.
fn fire_start_game_once(
    mut commands: Commands,
    mut trigger: Single<(Entity, &mut TriggerStartGame)>,
) {
    let (entity, trigger) = &mut *trigger;
    if trigger.fired { return; }
    commands.trigger(TriggerFired { entity: *entity });
    trigger.fired = true;
}
