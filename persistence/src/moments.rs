use bevy::prelude::*;

use game_core::prelude::{Moment, MomentKind, MomentOf};
use logging::prelude::*;

use crate::common::GameDbHelpers;
use crate::load::LoadContext;
use crate::rusqlite;
use crate::save::{SaveContext, SaveWriter};

// ============================================================================
// Generic moment persistence helpers
//
// Written once, called from each domain's `_internal` crate. Parameterized by
// `M: MomentKind` — the `KIND` const selects the domain's rows from the shared
// `moments` table.
// ============================================================================

/// Save all moments of kind `M` to the `moments` table. Registered via
/// `register_moment_persistence`. Scenario saves reset `fired_count` to 0;
/// quick saves preserve it.
pub fn save_moments<M: MomentKind>(
    save_ctx: Res<SaveContext>,
    mut save: SaveWriter,
    moments: Query<(Entity, &Moment, &MomentOf, &M)>,
) {
    if moments.is_empty() { return; }
    let rows: Vec<(i64, i64, &'static str, u32)> = moments
        .iter()
        .map(|(entity, moment, parent, _marker)| {
            let id = entity.index_u32() as i64;
            let parent_id = parent.0.index_u32() as i64;
            let fired_count = if save_ctx.save_as_scenario { 0 } else { moment.fired_count };
            (id, parent_id, M::KIND, fired_count)
        })
        .collect();
    save.submit(move |tx| {
        for (id, parent_id, kind, fired_count) in rows {
            tx.register_entity(id)?;
            tx.register_entity(parent_id)?;
            tx.execute(
                "INSERT OR REPLACE INTO moments (id, parent_id, kind, fired_count) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![id, parent_id, kind, fired_count],
            )?;
        }
        Ok(())
    });
}

/// Load all moments of kind `M` from the `moments` table and restore them via
/// `ctx.insert`. Registered via `register_moment_persistence`.
pub fn load_moments<M: MomentKind>(ctx: &mut LoadContext) -> rusqlite::Result<()> {
    let mut stmt = ctx.conn.prepare(
        "SELECT id, parent_id, fired_count FROM moments WHERE kind = ?1",
    )?;
    let mut rows = stmt.query(rusqlite::params![M::KIND])?;
    while let Some(row) = rows.next()? {
        let old_id: i64 = row.get(0)?;
        let old_parent_id: i64 = row.get(1)?;
        let fired_count: i64 = row.get(2)?;

        let Some(entity) = ctx.entity(old_id) else {
            Log::warn().dev().tag(Tag::GameLoad).message(format!(
                "Moment kind '{}' old ID {old_id} failed entity remap — skipping",
                M::KIND,
            ));
            continue;
        };

        let Some(parent) = ctx.entity(old_parent_id) else {
            Log::warn().dev().tag(Tag::GameLoad).message(format!(
                "Moment kind '{}' old ID {old_id} has parent_id {old_parent_id} that failed entity remap — skipping",
                M::KIND,
            ));
            continue;
        };

        ctx.insert(
            entity,
            (
                Moment { fired_count: fired_count as u32 },
                MomentOf(parent),
                M::default(),
            ),
        );
    }
    Ok(())
}
