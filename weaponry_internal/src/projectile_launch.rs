//! # Projectile Launch
//!
//! Deferred launch scheduling for projectiles with [`ProjectileLaunchCounter`].
//!
//! When [`GameClock`] reaches `launch_at`, [`launch_due_projectiles`] removes
//! [`ProjectileLaunchCounter`] and triggers [`ProjectileLaunched`] on the entity. Each projectile
//! type handles [`ProjectileLaunched`] to begin flight and reveal its visuals.
//!
//! [`SaveProjectileLaunchCounterExtension`] and [`LoadProjectileLaunchCounterExtension`] persist
//! pending launch times in the `projectile_launch_counters` table.

use bevy::prelude::*;

use persistence::{prelude::*, rusqlite::{self, OptionalExtension}};
use session::GameClock;
use states::prelude::GameState;
use weaponry::prelude::{ProjectileLaunchCounter, ProjectileLaunched};

pub(crate) struct ProjectileLaunchPlugin;
impl Plugin for ProjectileLaunchPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Update, launch_due_projectiles.in_set(ProjectileLaunchSystems).run_if(in_state(GameState::Running)));
    }
}

/// Removes expired [`ProjectileLaunchCounter`] components and triggers [`ProjectileLaunched`].
/// Flight movement systems run after this set so newly launched projectiles advance on the same frame.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ProjectileLaunchSystems;

fn launch_due_projectiles(
    mut commands: Commands,
    clock: Res<GameClock>,
    projectiles: Query<(Entity, &ProjectileLaunchCounter)>,
) {
    for (entity, launch_counter) in projectiles.iter() {
        if launch_counter.launch_at > clock.now() { continue; }
        commands.entity(entity).remove::<ProjectileLaunchCounter>();
        commands.trigger(ProjectileLaunched(entity));
    }
}

pub(crate) trait SaveProjectileLaunchCounterExtension {
    /// Persists `launch_counter` into the `projectile_launch_counters` table for an already-registered `entity_id`.
    fn save_projectile_launch_counter(&self, entity_id: u32, launch_counter: ProjectileLaunchCounter) -> rusqlite::Result<()>;
}
impl SaveProjectileLaunchCounterExtension for SaveContext<'_> {
    fn save_projectile_launch_counter(&self, entity_id: u32, launch_counter: ProjectileLaunchCounter) -> rusqlite::Result<()> {
        self.tx.prepare_cached("INSERT OR REPLACE INTO projectile_launch_counters (entity_id, launch_at) VALUES (?1, ?2)")?
            .execute(rusqlite::params![entity_id, launch_counter.launch_at])?;
        Ok(())
    }
}

pub(crate) trait LoadProjectileLaunchCounterExtension {
    /// Loads the saved counter, or returns `None` if no `projectile_launch_counters` row exists.
    fn projectile_launch_counter(&self, old_id: u32) -> LoadResult<Option<ProjectileLaunchCounter>>;
}
impl LoadProjectileLaunchCounterExtension for LoadContext<'_> {
    fn projectile_launch_counter(&self, old_id: u32) -> LoadResult<Option<ProjectileLaunchCounter>> {
        self.conn.prepare_cached("SELECT launch_at FROM projectile_launch_counters WHERE entity_id = ?1")
            .and_then(|mut statement| statement.query_row([old_id], |row| Ok(ProjectileLaunchCounter::new(row.get(0)?))).optional())
            .map_err(LoadError::table_read("projectile_launch_counters"))
    }
}
