use bevy::prelude::*;

use game_core::prelude::CELL_SIZE;
use grids::prelude::MapInfo;
use persistence::{
    prelude::{AppGameLoadSaveExtension, CollectSave, LoadContext, SaveWriter},
    rusqlite,
};
use states::prelude::MapLoadingStage;

fn collect_map_info(
    map_info: Res<MapInfo>,
    mut save: SaveWriter,
) {
    let width = map_info.grid_width;
    let height = map_info.grid_height;
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
    let mut stmt = ctx.conn.prepare("SELECT width, height, name FROM map_info WHERE id = 1")?;
    let result = stmt.query_row([], |row| {
        let width: i32 = row.get(0)?;
        let height: i32 = row.get(1)?;
        let name: String = row.get(2)?;
        Ok((width, height, name))
    });

    let (width, height, name) = result?;
    let map_info = MapInfo {
        grid_width: width,
        grid_height: height,
        world_width: width as f32 * CELL_SIZE,
        world_height: height as f32 * CELL_SIZE,
        name,
    };

    ctx.insert_resource(map_info);
    Ok(())
}

pub struct MapInfoPlugin;
impl Plugin for MapInfoPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(CollectSave, collect_map_info)
            .register_loader(MapLoadingStage::LoadMapInfo, "map_info", load_map_info)
            ;
    }
}
