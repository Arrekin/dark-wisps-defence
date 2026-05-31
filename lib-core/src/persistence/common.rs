use bevy::ecs::system::ScheduleSystem;

use crate::lib_prelude::*;

/// Run `f` against a freshly opened SQLite connection to `path`.
///
/// The connection is scoped to this call and dropped the instant `f` returns,
/// releasing the OS file handle. Do NOT cache it across calls: SQLite on Windows
/// opens without FILE_SHARE_DELETE, so any lingering handle blocks the save
/// path's `remove_file`. Loads stay parallel because each worker thread opens
/// its own short-lived connection.
pub fn with_db_connection<F>(path: &str, f: F) -> Result<(), Box<dyn std::error::Error>>
where
    F: FnOnce(&mut rusqlite::Connection) -> Result<(), Box<dyn std::error::Error>>,
{
    let mut conn = rusqlite::Connection::open(path)?;
    f(&mut conn)
}

pub mod db_migrations {
    use refinery::embed_migrations;
    embed_migrations!("./migrations");
}

pub trait GameDbHelpers {
    fn register_entity(&self, entity_id: i64) -> rusqlite::Result<usize>;
    fn save_grid_coords(&self, entity_id: i64, pos: GridCoords) -> rusqlite::Result<usize>;
    fn save_world_position(&self, entity_id: i64, pos: Vec2) -> rusqlite::Result<usize>;
    fn save_integrity_points(&self, entity_id: i64, current: f32) -> rusqlite::Result<usize>;
    fn save_grid_imprint(&self, entity_id: i64, imprint: GridImprint) -> rusqlite::Result<usize>;
    
    fn save_marker(&self, table_name: &str, entity_id: i64) -> rusqlite::Result<usize>;
    fn save_disabled_by_player(&self, entity_id: i64) -> rusqlite::Result<usize>;
    fn save_stat(&self, stat_name: &str, stat_value: f32) -> rusqlite::Result<usize>;
    fn save_stock_resource(&self, resource_name: &str, amount: i32) -> rusqlite::Result<usize>;
    
    fn get_grid_coords(&self, entity_id: i64) -> rusqlite::Result<GridCoords>;
    fn get_disabled_by_player(&self, entity_id: i64) -> rusqlite::Result<bool>;
    fn get_world_position(&self, entity_id: i64) -> rusqlite::Result<Vec2>;
    fn get_integrity_points(&self, entity_id: i64) -> rusqlite::Result<f32>;
    fn get_grid_imprint(&self, entity_id: i64) -> rusqlite::Result<GridImprint>;
    fn get_stat(&self, stat_name: &str) -> rusqlite::Result<f32>;
    fn get_stock_resource(&self, resource_name: &str) -> rusqlite::Result<i32>;
}
impl GameDbHelpers for rusqlite::Connection {
    fn register_entity(&self, entity_id: i64) -> rusqlite::Result<usize> {
        self.execute(
            "INSERT OR IGNORE INTO entities (id) VALUES (?1)",
            [entity_id],
        )
    }

    fn save_grid_coords(&self, entity_id: i64, pos: GridCoords) -> rusqlite::Result<usize> {
        self.execute(
            "INSERT INTO grid_coords (entity_id, x, y) VALUES (?1, ?2, ?3)",
            (entity_id, pos.x, pos.y),
        )
    }

    fn save_world_position(&self, entity_id: i64, pos: Vec2) -> rusqlite::Result<usize> {
        self.execute(
            "INSERT INTO world_positions (entity_id, x, y) VALUES (?1, ?2, ?3)",
            (entity_id, pos.x, pos.y),
        )
    }

    fn save_integrity_points(&self, entity_id: i64, current: f32) -> rusqlite::Result<usize> {
        self.execute(
            "INSERT OR REPLACE INTO integrity_points (entity_id, current) VALUES (?1, ?2)",
            (entity_id, current),
        )
    }

    fn save_grid_imprint(&self, entity_id: i64, imprint: GridImprint) -> rusqlite::Result<usize> {
        let (shape, width, height) = match imprint {
            GridImprint::Rectangle { width, height } => ("Rectangle", Some(width), Some(height)),
            // Stored as: shape="Plus", width=extents, height=NULL
            GridImprint::Plus { extents } => ("Plus", Some(extents), None),
        };

        self.execute(
            "INSERT OR REPLACE INTO grid_imprints (id, shape, width, height) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![entity_id, shape, width, height],
        )
    }


    /// Save entity of the object in its dedicated table. Calls register_entity()
    fn save_marker(&self, table_name: &str, entity_id: i64) -> rusqlite::Result<usize> {
        self.register_entity(entity_id)?;
        let query = format!("INSERT OR REPLACE INTO {} (id) VALUES (?1)", table_name);
        self.execute(&query, [entity_id])
    }

    fn save_disabled_by_player(&self, entity_id: i64) -> rusqlite::Result<usize> {
        self.execute(
            "INSERT INTO disabled_by_player (entity_id) VALUES (?1)",
            [entity_id],
        )
    }

    fn save_stat(&self, stat_name: &str, stat_value: f32) -> rusqlite::Result<usize> {
        self.execute(
            "INSERT OR REPLACE INTO stats (stat_name, stat_value) VALUES (?1, ?2)",
            (stat_name, stat_value),
        )
    }

    fn save_stock_resource(&self, resource_name: &str, amount: i32) -> rusqlite::Result<usize> {
        self.execute(
            "INSERT OR REPLACE INTO stock (resource_name, amount) VALUES (?1, ?2)",
            (resource_name, amount),
        )
    }


    fn get_disabled_by_player(&self, entity_id: i64) -> rusqlite::Result<bool> {
        let mut stmt = self.prepare("SELECT 1 FROM disabled_by_player WHERE entity_id = ?1")?;
        let exists = stmt.exists([entity_id])?;
        Ok(exists)
    }

    fn get_stat(&self, stat_name: &str) -> rusqlite::Result<f32> {
        let mut stmt = self.prepare("SELECT stat_value FROM stats WHERE stat_name = ?1")?;
        let mut rows = stmt.query([stat_name])?;
        let row = rows.next()?.ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        Ok(row.get(0)?)
    }

    fn get_stock_resource(&self, resource_name: &str) -> rusqlite::Result<i32> {
        let mut stmt = self.prepare("SELECT amount FROM stock WHERE resource_name = ?1")?;
        let mut rows = stmt.query([resource_name])?;
        let row = rows.next()?.ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        Ok(row.get(0)?)
    }
    
    fn get_grid_coords(&self, entity_id: i64) -> rusqlite::Result<GridCoords> {
        let mut stmt = self.prepare("SELECT x, y FROM grid_coords WHERE entity_id = ?1")?;
        let mut rows = stmt.query([entity_id])?;
        let row = rows.next()?.ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        Ok(GridCoords { x: row.get(0)?, y: row.get(1)? })
    }

    fn get_world_position(&self, entity_id: i64) -> rusqlite::Result<Vec2> {
        let mut stmt = self.prepare("SELECT x, y FROM world_positions WHERE entity_id = ?1")?;
        let mut rows = stmt.query([entity_id])?;
        let row = rows.next()?.ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        Ok(Vec2::new(row.get(0)?, row.get(1)?))
    }
    
    fn get_integrity_points(&self, entity_id: i64) -> rusqlite::Result<f32> {
        let mut stmt = self.prepare("SELECT current FROM integrity_points WHERE entity_id = ?1")?;
        let mut rows = stmt.query([entity_id])?;
        let row = rows.next()?.ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        Ok(row.get(0)?)
    }

    fn get_grid_imprint(&self, entity_id: i64) -> rusqlite::Result<GridImprint> {
        let mut stmt = self.prepare("SELECT shape, width, height FROM grid_imprints WHERE id = ?1")?;
        let mut rows = stmt.query([entity_id])?;
        let row = rows.next()?.ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        
        let shape: String = row.get(0)?;
        match shape.as_str() {
            "Rectangle" => {
                let width: i32 = row.get(1)?;
                let height: i32 = row.get(2)?;
                Ok(GridImprint::Rectangle { width, height })
            }
            "Plus" => {
                let extents: i32 = row.get(1)?;
                Ok(GridImprint::Plus { extents })
            }
            _ => Err(rusqlite::Error::InvalidColumnType(0, "Unknown shape type".into(), rusqlite::types::Type::Text)),
        }
    }

}

pub trait AppGameLoadSaveExtension {
    fn register_db_loader<T: Loadable>(&mut self, stage: MapLoadingStage) -> &mut Self;
    fn register_db_saver<M>(&mut self, save_system: impl IntoScheduleConfigs<ScheduleSystem, M>) -> &mut Self;
}
impl AppGameLoadSaveExtension for App {
    fn register_db_loader<T: Loadable>(&mut self, stage: MapLoadingStage) -> &mut Self {
        if !self.world().contains_resource::<GameLoadRegistry>() {
            self.init_resource::<GameLoadRegistry>();
        }
        let mut registry = self.world_mut().resource_mut::<GameLoadRegistry>();
        registry.register::<T>(stage);

        self
    }
    fn register_db_saver<M>(&mut self, save_system: impl IntoScheduleConfigs<ScheduleSystem, M>) -> &mut Self {
        self.add_systems(PostUpdate, save_system.run_if(on_message::<SaveGameSignal>));
        
        self
    }
}