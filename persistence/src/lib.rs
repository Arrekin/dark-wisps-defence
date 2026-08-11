use bevy::app::{App, Plugin};

pub mod common;
pub mod load;
pub mod moments;
pub mod save;

pub use rusqlite;

pub use crate::common::run_migrations_on_paths;
pub use crate::load::{GameMapList, LoadGameSignal, LoadMapConfig, MapSource, creating_new_map};
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
            save::MapSavePlugin,
        ));
    }
}
