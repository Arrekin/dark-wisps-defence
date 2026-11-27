use bevy::app::{App, Plugin};

mod common;
mod load;
mod map_file;
mod map_info;
mod moments;
mod resource_lists;
mod save;
mod scenario_list;

pub use rusqlite;

pub use crate::common::run_migrations_on_files;
pub use crate::load::{LoadGameReport, LoadGameResult, LoadGameSignal, LoadMapConfig, MapSource, creating_new_map};
pub use crate::map_file::{InvalidFileName, MapFile, MapFileName};
pub use crate::save::{SaveGameReport, SaveGameResult, SaveGameSignal};
pub use crate::scenario_list::{MapListEntry, ScenarioList};

pub mod prelude {
    pub use crate::common::AppGameLoadSaveExtension;
    pub use crate::load::{EntityIdMap, LoadContext, LoadError, LoadProgress, LoadResult, LoadRowExtension, LoaderFn};
    pub use crate::save::{CollectSave, SaveContext, SaveRunner, SaveWriter};
}

pub struct PersistencePlugin;
impl Plugin for PersistencePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            load::MapLoadPlugin,
            map_info::MapInfoPlugin,
            save::MapSavePlugin,
        ));
    }
}
