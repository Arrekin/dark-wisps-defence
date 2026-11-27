use bevy::prelude::*;
use serde::Serialize;

use game_core::prelude::MapInfo;
use logging::prelude::*;

use crate::{
    common::{Migrations, with_db_connection},
    map_file::MapFile,
    map_info::read_map_info,
};

/// Whether reading the scenario list migrates each scenario first. `Skip` keeps listing from writing
/// in-development migrations into every map; maps are brought up to date with
/// `run_migrations_on_files`. `Apply` once migrations are final and players bring older maps.
const SCENARIO_LIST_MIGRATIONS: Migrations = Migrations::Skip;

/// Every scenario in `maps/` with its header. Starts empty and reads the headers on
/// first use, so a session that never shows the list pays nothing; the game calls
/// [`ScenarioList::refresh`] after it writes to `maps/`.
#[derive(Resource, Default)]
pub struct ScenarioList {
    entries: Vec<MapListEntry>,
}
impl ScenarioList {
    /// Cached entries; scans `maps/` first when the cache is empty.
    pub fn entries(&mut self) -> &[MapListEntry] {
        if self.entries.is_empty() {
            self.refresh();
        }
        &self.entries
    }

    /// Re-scans `maps/` and reads each file's header. A scenario whose header cannot be read, such
    /// as one not yet brought to the current schema with `run_migrations_on_files`, is logged and
    /// left out.
    #[log_tags(Tag::GameLoad)]
    pub fn refresh(&mut self) {
        self.entries = MapFile::list_scenarios()
            .into_iter()
            .filter_map(|file| {
                let path = file.path();
                let info = with_db_connection(&path, SCENARIO_LIST_MIGRATIONS, |conn| Ok(read_map_info(conn)?))
                    .inspect_err(|error| warn_dev!("Scenario '{path}' left out of the list: header unreadable ({error})"))
                    .ok()?;
                Some(MapListEntry { file, info })
            })
            .collect();
    }
}

#[derive(Clone, Serialize)]
pub struct MapListEntry {
    pub file: MapFile,
    pub info: MapInfo,
}
