use bevy::prelude::*;

use game_core::prelude::{GridCoords, GridImprint, MomentKind};
use logging::prelude::*;
use resources::prelude::{Cost, EssenceType, ResourceType};
use states::MapLoadingStage;

use crate::load::GameLoadRegistry;
use crate::moments::{load_moments, save_moments};
use crate::save::CollectSave;

/// Run `f` against a freshly opened SQLite connection to `path`.
///
/// The connection is scoped to this call and dropped the instant `f` returns,
/// releasing the OS file handle. Do NOT cache it across calls: SQLite on Windows
/// opens without FILE_SHARE_DELETE, so any lingering handle blocks the save
/// path's `remove_file`. Loads stay parallel because each worker thread opens
/// its own short-lived connection.
pub(crate) fn with_db_connection<F>(path: &str, f: F) -> Result<(), Box<dyn std::error::Error>>
where
    F: FnOnce(&mut rusqlite::Connection) -> Result<(), Box<dyn std::error::Error>>,
{
    let mut conn = rusqlite::Connection::open(path)?;
    f(&mut conn)
}

pub(crate) mod db_migrations {
    use refinery::embed_migrations;
    embed_migrations!("./migrations");
}

/// Apply (or rebuild) schema migrations on every `.dwd` file in `paths`.
///
/// When `rebuild_metadata` is true, the refinery schema history is cleared first
/// so V1 re-runs from scratch. Use this only when consolidating migrations.
pub fn run_migrations_on_paths(paths: &[String], rebuild_metadata: bool) {
    for path in paths {
        if rebuild_metadata {
            Log::info().dev().tag(Tag::GameLoad).message(format!("Rebuilding migration metadata for '{path}'"));
        } else {
            Log::info().dev().tag(Tag::GameLoad).message(format!("Applying migrations to '{path}'"));
        }
        if let Err(e) = with_db_connection(path, |conn| {
            if rebuild_metadata {
                conn.execute("DELETE FROM refinery_schema_history;", [])?;
            }
            db_migrations::migrations::runner().run(conn)?;
            Ok(())
        }) {
            let action = if rebuild_metadata { "Rebuild" } else { "Migration" };
            Log::error().dev().tag(Tag::GameLoad).message(format!("{action} failed for '{path}': {e}"));
        }
    }
    let done_msg = if rebuild_metadata { "Migration metadata rebuild complete" } else { "Migrations complete" };
    Log::info().dev().tag(Tag::GameLoad).message(done_msg);
}

pub trait GameDbHelpers {
    fn register_entity(&self, entity_id: i64) -> rusqlite::Result<usize>;
    fn save_marker(&self, table_name: &str, entity_id: i64) -> rusqlite::Result<usize>;
    fn save_world_position(&self, entity_id: i64, pos: Vec2) -> rusqlite::Result<usize>;
    fn save_integrity_points(&self, entity_id: i64, current: f32) -> rusqlite::Result<usize>;
    fn save_disabled_by_player(&self, entity_id: i64) -> rusqlite::Result<usize>;
    fn save_stat(&self, stat_name: &str, stat_value: f32) -> rusqlite::Result<usize>;
    fn save_stock_resource(&self, resource_name: &str, amount: i32) -> rusqlite::Result<usize>;
    fn save_grid_coords(&self, entity_id: i64, pos: GridCoords) -> rusqlite::Result<usize>;
    fn save_grid_imprint(&self, entity_id: i64, imprint: GridImprint) -> rusqlite::Result<usize>;
    fn save_costs(&self, entity_id: i64, costs: &[Cost]) -> rusqlite::Result<()>;

    fn get_world_position(&self, entity_id: i64) -> rusqlite::Result<Vec2>;
    fn get_integrity_points(&self, entity_id: i64) -> rusqlite::Result<f32>;
    fn get_disabled_by_player(&self, entity_id: i64) -> rusqlite::Result<bool>;
    fn get_stat(&self, stat_name: &str) -> rusqlite::Result<f32>;
    fn get_stock_resource(&self, resource_name: &str) -> rusqlite::Result<i32>;
    fn get_grid_coords(&self, entity_id: i64) -> rusqlite::Result<GridCoords>;
    fn get_grid_imprint(&self, entity_id: i64) -> rusqlite::Result<GridImprint>;
    fn get_costs(&self, entity_id: i64) -> rusqlite::Result<Vec<Cost>>;
}
impl GameDbHelpers for rusqlite::Connection {
    fn register_entity(&self, entity_id: i64) -> rusqlite::Result<usize> {
        self.execute(
            "INSERT OR IGNORE INTO entities (id) VALUES (?1)",
            [entity_id],
        )
    }

    /// Save entity of the object in its dedicated table. Calls register_entity()
    fn save_marker(&self, table_name: &str, entity_id: i64) -> rusqlite::Result<usize> {
        self.register_entity(entity_id)?;
        let query = format!("INSERT OR REPLACE INTO {} (id) VALUES (?1)", table_name);
        self.execute(&query, [entity_id])
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

    fn save_grid_coords(&self, entity_id: i64, pos: GridCoords) -> rusqlite::Result<usize> {
        self.execute(
            "INSERT INTO grid_coords (entity_id, x, y) VALUES (?1, ?2, ?3)",
            (entity_id, pos.x, pos.y),
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

    fn save_costs(&self, entity_id: i64, costs: &[Cost]) -> rusqlite::Result<()> {
        for (position, cost) in costs.iter().enumerate() {
            let (resource_kind, essence_type): (&str, Option<String>) = match cost.resource_type {
                ResourceType::DarkOre => ("DarkOre", None),
                ResourceType::Essence(essence) => ("Essence", Some(essence.as_ref().to_string())),
            };
            self.execute(
                "INSERT OR REPLACE INTO costs (entity_id, position, resource_kind, essence_type, amount) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![entity_id, position as i64, resource_kind, essence_type, cost.amount],
            )?;
        }
        Ok(())
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
        row.get(0)
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
        row.get(0)
    }

    fn get_stock_resource(&self, resource_name: &str) -> rusqlite::Result<i32> {
        let mut stmt = self.prepare("SELECT amount FROM stock WHERE resource_name = ?1")?;
        let mut rows = stmt.query([resource_name])?;
        let row = rows.next()?.ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        row.get(0)
    }

    fn get_grid_coords(&self, entity_id: i64) -> rusqlite::Result<GridCoords> {
        let mut stmt = self.prepare("SELECT x, y FROM grid_coords WHERE entity_id = ?1")?;
        let mut rows = stmt.query([entity_id])?;
        let row = rows.next()?.ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        Ok(GridCoords { x: row.get(0)?, y: row.get(1)? })
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

    fn get_costs(&self, entity_id: i64) -> rusqlite::Result<Vec<Cost>> {
        let mut stmt = self.prepare(
            "SELECT resource_kind, essence_type, amount FROM costs WHERE entity_id = ?1 AND custom_key = 0 ORDER BY position",
        )?;
        let mut rows = stmt.query([entity_id])?;
        let mut costs = Vec::new();
        while let Some(row) = rows.next()? {
            let resource_kind: String = row.get(0)?;
            let essence_type: Option<String> = row.get(1)?;
            let amount: i32 = row.get(2)?;
            let resource_type = match resource_kind.as_str() {
                "DarkOre" => ResourceType::DarkOre,
                "Essence" => {
                    let Some(essence_str) = essence_type else {
                        Log::warn().dev().tag(Tag::GameLoad).message(format!("Essence cost for entity {entity_id} has no essence_type — skipping cost"));
                        continue;
                    };
                    match essence_str.parse::<EssenceType>() {
                        Ok(essence) => ResourceType::Essence(essence),
                        Err(_) => {
                            Log::warn().dev().tag(Tag::GameLoad).message(format!("Unknown essence type in save: {essence_str}"));
                            continue;
                        }
                    }
                }
                other => {
                    Log::warn().dev().tag(Tag::GameLoad).message(format!("Unknown resource kind in save: {other}"));
                    continue;
                }
            };
            costs.push(Cost { resource_type, amount });
        }
        Ok(costs)
    }
}

pub trait AppGameLoadSaveExtension {
    fn register_loader(
        &mut self,
        stage: MapLoadingStage,
        table: &'static str,
        loader: crate::load::LoaderFn,
    ) -> &mut Self;

    /// Register save collector + loader for a moment kind. Combines
    /// `save_moments::<M>` and `load_moments::<M>` into one call. Loads at
    /// `SpawnEffectInstances` — late enough that all parent entities exist.
    fn register_moment_persistence<M: MomentKind>(&mut self) -> &mut Self;
}
impl AppGameLoadSaveExtension for App {
    fn register_loader(
        &mut self,
        stage: MapLoadingStage,
        table: &'static str,
        loader: crate::load::LoaderFn,
    ) -> &mut Self {
        if !self.world().contains_resource::<GameLoadRegistry>() {
            self.init_resource::<GameLoadRegistry>();
        }
        let mut registry = self
            .world_mut()
            .resource_mut::<GameLoadRegistry>();
        registry
            .loaders
            .entry(stage)
            .or_default()
            .push(crate::load::LoaderDescriptor {
                table,
                run: loader,
            });

        self
    }

    fn register_moment_persistence<M: MomentKind>(&mut self) -> &mut Self {
        // Known issue: all moment kinds share the `moments` table, so the
        // progress bar row counter (`SELECT COUNT(*) FROM moments`) runs once
        // per kind, inflating the total. The actual load is correct (each
        // loader filters by `WHERE kind = ?`). Accepted as a cosmetic
        // imperfection.
        self.add_systems(CollectSave, save_moments::<M>)
            .register_loader(MapLoadingStage::SpawnEffectInstances, "moments", load_moments::<M>)
    }
}