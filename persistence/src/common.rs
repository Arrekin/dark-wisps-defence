use bevy::prelude::*;

use game_core::prelude::MomentKind;
use logging::prelude::*;
use states::MapLoadingStage;

use crate::{
    load::GameLoadRegistry,
    moments::{load_moments, save_moments},
    save::CollectSave,
};

/// Whether [`with_db_connection`] brings the file up to the current schema before running `f`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Migrations {
    Apply,
    Skip,
}

/// Run `f` against a freshly opened SQLite connection to `path`, after applying schema
/// migrations when `migrations` is [`Migrations::Apply`].
///
/// The connection is scoped to this call and dropped the instant `f` returns,
/// releasing the OS file handle. Do NOT cache it across calls: SQLite on Windows
/// opens without FILE_SHARE_DELETE, so any lingering handle blocks the save
/// path's `remove_file`. Loads stay parallel because each worker thread opens
/// its own short-lived connection.
pub(crate) fn with_db_connection<T, F>(path: &str, migrations: Migrations, f: F) -> Result<T, Box<dyn std::error::Error>>
where
    F: FnOnce(&mut rusqlite::Connection) -> Result<T, Box<dyn std::error::Error>>,
{
    let mut conn = rusqlite::Connection::open(path)?;
    if migrations == Migrations::Apply {
        db_migrations::migrations::runner().run(&mut conn)?;
    }
    f(&mut conn)
}

pub(crate) mod db_migrations {
    use refinery::embed_migrations;
    embed_migrations!("./migrations");
}

/// Apply schema migrations on every `.dwd` file in `paths`.
///
/// When `rebuild_metadata` is true, the refinery schema history is cleared first
/// so V1 re-runs from scratch. Use this only when consolidating migrations.
#[log_tags(Tag::GameLoad)]
pub fn run_migrations_on_paths(paths: &[String], rebuild_metadata: bool) {
    for path in paths {
        let _ = with_db_connection(path, Migrations::Skip, |conn| {
            if rebuild_metadata {
                #[info_dev("Cleared migration history of '{path}'")]
                conn.execute("DELETE FROM refinery_schema_history;", [])?;
            }
            #[info_dev("Applied migrations to '{path}'")]
            db_migrations::migrations::runner().run(conn)?;
            Ok(())
        })
        .inspect_err(|error| error_dev!("Migrations failed for '{path}': {error}"));
    }
    info_dev!("Migrations complete");
}

pub trait AppGameLoadSaveExtension {
    fn register_loader(
        &mut self,
        stage: MapLoadingStage,
        table: &'static str,
        loader: crate::load::LoaderFn,
    ) -> &mut Self;

    /// Register save collector + loader for a moment kind. Combines
    /// `save_moments::<M>` and `load_moments::<M>` into one call. Loads at
    /// `SpawnEffectInstances` — late enough that all parent entities exist.
    fn register_moment_persistence<M: MomentKind>(&mut self) -> &mut Self;
}
impl AppGameLoadSaveExtension for App {
    fn register_loader(
        &mut self,
        stage: MapLoadingStage,
        table: &'static str,
        loader: crate::load::LoaderFn,
    ) -> &mut Self {
        if !self.world().contains_resource::<GameLoadRegistry>() {
            self.init_resource::<GameLoadRegistry>();
        }
        let mut registry = self
            .world_mut()
            .resource_mut::<GameLoadRegistry>();
        registry
            .loaders
            .entry(stage)
            .or_default()
            .push(crate::load::LoaderDescriptor {
                table,
                run: loader,
            });

        self
    }

    fn register_moment_persistence<M: MomentKind>(&mut self) -> &mut Self {
        // Known issue: all moment kinds share the `moments` table, so the
        // progress bar row counter (`SELECT COUNT(*) FROM moments`) runs once
        // per kind, inflating the total. The actual load is correct (each
        // loader filters by `WHERE kind = ?`). Accepted as a cosmetic
        // imperfection.
        self.add_systems(CollectSave, save_moments::<M>)
            .register_loader(MapLoadingStage::SpawnEffectInstances, "moments", load_moments::<M>)
    }
}
