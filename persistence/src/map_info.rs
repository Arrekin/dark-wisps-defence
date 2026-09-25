use bevy::prelude::*;

use game_core::prelude::MapInfo;
use states::prelude::MapLoadingStage;

use crate::{
    common::AppGameLoadSaveExtension,
    load::{LoadContext, LoadMapConfig, MapSource, creating_new_map},
    rusqlite,
    save::{CollectSave, SaveWriter},
};

pub(crate) struct MapInfoPlugin;
impl Plugin for MapInfoPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<MapInfo>()
            .add_systems(CollectSave, collect_map_info)
            .register_loader(MapLoadingStage::LoadMapInfo, "map_info", load_map_info)
            .add_systems(OnEnter(MapLoadingStage::LoadMapInfo), insert_new_map_info.run_if(creating_new_map))
            ;
    }
}

/// Reads the map header — the single `map_info` row every `.dwd` carries.
pub(crate) fn read_map_info(conn: &rusqlite::Connection) -> rusqlite::Result<MapInfo> {
    conn.query_row("SELECT name, width, height FROM map_info WHERE id = 1", [], |row| {
        let name: String = row.get(0)?;
        let width: i32 = row.get(1)?;
        let height: i32 = row.get(2)?;
        Ok(MapInfo::new(name, (width, height)))
    })
}

fn collect_map_info(
    map_info: Res<MapInfo>,
    mut save: SaveWriter,
) {
    let width = map_info.grid_bounds.width;
    let height = map_info.grid_bounds.height;
    let name = map_info.name.clone();
    save.submit(move |tx| {
        tx.execute(
            "INSERT OR REPLACE INTO map_info (id, width, height, name) VALUES (1, ?1, ?2, ?3)",
            (width, height, &name),
        )?;
        Ok(())
    });
}

fn load_map_info(ctx: &mut LoadContext) -> rusqlite::Result<()> {
    let map_info = read_map_info(ctx.conn)?;
    ctx.insert_resource(map_info);
    Ok(())
}

/// Insert `MapInfo` from the load config on the new-map path. Mirrors the
/// `load_map_info` loader — same stage, same guarantee that `MapInfo` is in
/// place by `OnExit(LoadMapInfo)`.
fn insert_new_map_info(mut commands: Commands, config: Res<LoadMapConfig>) {
    if let MapSource::New(map_info) = &config.source {
        commands.insert_resource(map_info.clone());
    }
}
