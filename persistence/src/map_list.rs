use bevy::prelude::*;
use serde::Serialize;

use game_core::prelude::MapInfo;

use crate::{
    common::{Migrations, with_db_connection},
    map_file_name::{MapFileName, list_map_file_names},
    map_info::read_map_info,
};

/// Whether reading the catalog migrates each map file first. `Skip` keeps listing from writing
/// in-development migrations into every map; maps are brought up to date with
/// `run_migrations_on_paths`. `Apply` once migrations are final and players bring older maps.
const CATALOG_MIGRATIONS: Migrations = Migrations::Skip;

/// Map catalog: every `.dwd` in `maps/` with its header. Starts empty and reads the headers on
/// first use, so a session that never shows the list pays nothing; the game calls
/// [`GameMapList::refresh`] after it writes to `maps/`. Reading at app build would panic on an
/// unmigrated map before the migration launch action could fix it.
#[derive(Resource, Default)]
pub struct GameMapList {
    entries: Vec<MapListEntry>,
}
impl GameMapList {
    /// Cached entries; scans `maps/` first when the cache is empty.
    pub fn entries(&mut self) -> &[MapListEntry] {
        if self.entries.is_empty() {
            self.refresh();
        }
        &self.entries
    }

    /// Re-scans `maps/` and reads each file's header.
    ///
    /// Panics when a header cannot be read; bring the maps up to the current schema with
    /// `run_migrations_on_paths`.
    pub fn refresh(&mut self) {
        self.entries = list_map_file_names()
            .into_iter()
            .map(|file_name| {
                let path = file_name.path();
                let info = with_db_connection(&path, CATALOG_MIGRATIONS, |conn| Ok(read_map_info(conn)?))
                    .unwrap_or_else(|error| panic!("Failed to read the map header of '{path}': {error}"));
                MapListEntry { file_name, info }
            })
            .collect();
    }
}

#[derive(Clone, Serialize)]
pub struct MapListEntry {
    pub file_name: MapFileName,
    pub info: MapInfo,
}
