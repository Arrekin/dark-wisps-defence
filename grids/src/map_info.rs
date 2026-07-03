use bevy::prelude::*;

use game_core::prelude::{CELL_SIZE, SSS};
use persistence::{prelude::{LoadContext, LoadResult, Loadable, Saveable}, rusqlite};

#[derive(Resource, Default, Clone, SSS)]
pub struct MapInfo {
    pub grid_width: i32,
    pub grid_height: i32,
    pub world_width: f32,
    pub world_height: f32,
    pub name: String,
}
impl Saveable for MapInfo {
    fn save(self, tx: &rusqlite::Transaction) -> rusqlite::Result<()> {
        tx.execute(
            "INSERT OR REPLACE INTO map_info (id, width, height, name) VALUES (1, ?1, ?2, ?3)",
            (self.grid_width, self.grid_height, &self.name),
        )?;
        Ok(())
    }
}
impl Loadable for MapInfo {
    fn load(ctx: &mut LoadContext) -> rusqlite::Result<LoadResult> {
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

        ctx.commands.insert_resource(map_info);
        Ok(LoadResult::Finished)
    }
}
