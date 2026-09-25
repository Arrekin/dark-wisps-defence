use bevy::app::{App, Plugin};

pub mod common;
pub mod load;
mod map_file_name;
mod map_info;
mod map_list;
pub mod moments;
pub mod save;

pub use rusqlite;

pub use crate::common::run_migrations_on_paths;
pub use crate::load::{LoadGameReport, LoadGameResult, LoadGameSignal, LoadMapConfig, MapSource, creating_new_map};
pub use crate::map_file_name::{MapFileName, list_map_file_names};
pub use crate::map_list::{GameMapList, MapListEntry};
pub use crate::save::{SaveContext, SaveGameSignal, SaveTarget};

pub mod prelude {
    pub use crate::common::{AppGameLoadSaveExtension, GameDbHelpers};
    pub use crate::load::{EntityIdMap, LoadContext, LoadProgress, LoaderFn};
    pub use crate::save::{CollectSave, SaveContext, SaveWriter};
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
