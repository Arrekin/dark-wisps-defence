use bevy::app::{App, Plugin};

pub mod common;
pub mod load;
pub mod save;

pub use rusqlite;

pub use crate::common::run_migrations_on_paths;
pub use crate::load::{GameMapList, LoadGameSignal, LoadMapConfig};

pub mod prelude {
    pub use crate::common::{AppGameLoadSaveExtension, GameDbHelpers};
    pub use crate::load::{Loadable, LoadContext, LoadResult};
    pub use crate::save::{Saveable, SaveableBatchCommand};
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
