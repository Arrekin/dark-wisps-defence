use crate::lib_prelude::*;
use super::common::increment_db_generation;
use bevy::input::common_conditions::input_just_released;

pub struct MapSavePlugin;
impl Plugin for MapSavePlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<GameSaveExecutor>()
            .add_message::<SaveGameSignal>()
            .add_systems(Update, (
                SaveGameSignal::emit.run_if(input_just_released(KeyCode::KeyZ)),
            ))
            .add_systems(Last, (
                GameSaveExecutor::on_game_save.run_if(on_message::<SaveGameSignal>),
            ))
            ;
    }
}

pub trait Saveable: SSS {
    fn save(self, tx: &rusqlite::Transaction) -> rusqlite::Result<()>;
}
pub trait SaveableBatch: SSS {
    fn save_batch(self: Box<Self>, tx: &rusqlite::Transaction) -> rusqlite::Result<()>;
}


#[derive(Message)]
pub struct SaveGameSignal;
impl SaveGameSignal {
    fn emit(
        mut save_executor: ResMut<GameSaveExecutor>,
        mut writer: MessageWriter<SaveGameSignal>,
    ) {
        save_executor.save_name = "test_save.dwd".into();
        save_executor.objects_to_save.clear();
        writer.write(SaveGameSignal);
    }
}

pub struct SaveableBatchCommand<T: Saveable> {
    data: Vec<T>,
}
impl<T: Saveable> SaveableBatchCommand<T> {
    pub fn from_single(item: T) -> Self {
        Self { data: vec![item] }
    }
    pub fn len(&self) -> usize { self.data.len() }
}
impl<T: Saveable> FromIterator<T> for SaveableBatchCommand<T> {
    fn from_iter<I: IntoIterator<Item=T>>(iter: I) -> Self {
        let mut result = Self { data: Vec::new() };
        result.data.extend(iter);
        result
    }
}
impl<T: Saveable> SSS for SaveableBatchCommand<T> {}
impl<T: Saveable> SaveableBatch for SaveableBatchCommand<T> {
    fn save_batch(self: Box<Self>, tx: &rusqlite::Transaction) -> rusqlite::Result<()> {
        for item in self.data {
            item.save(tx)?;
        }
        Ok(())
    }
}
impl<T: Saveable> Command for SaveableBatchCommand<T> {
    fn apply(self, world: &mut World) {
        let mut buffer = world.resource_mut::<GameSaveExecutor>();
        println!("Added {} objects to GameSaveExecutor", self.data.len());
        // Box since we store dyn in GameSaveExecutor
        buffer.objects_to_save.push(Box::new(self));
    }
}
#[derive(Resource, Default)]
pub struct GameSaveExecutor {
    pub save_name: String,
    pub objects_to_save: Vec<Box<dyn SaveableBatch>>,
}
impl GameSaveExecutor {
    pub fn on_game_save(
        mut save_executor: ResMut<GameSaveExecutor>,
    ) {
        if save_executor.objects_to_save.is_empty() {
            return;
        }
        
        let objects = std::mem::take(&mut save_executor.objects_to_save);
        let save_name = save_executor.save_name.clone();

        std::thread::spawn(move || {
            fn save_process(save_name: &str, objects: Vec<Box<dyn SaveableBatch>>) -> Result<(), Box<dyn std::error::Error>> {
                if std::path::Path::new(save_name).exists() {
                    println!("Removing existing save file '{}'", save_name);
                    std::fs::remove_file(save_name)?;
                }
                
                GameDbConnection::with_db_connection(save_name, |conn| {
                    // Run migrations
                    db_migrations::migrations::runner().run(conn)?;

                    // Start transaction
                    let tx = conn.transaction()?;

                    // Save all objects
                    for batch in objects {
                        batch.save_batch(&tx)?;
                    }

                    // Commit transaction
                    tx.commit()?;
                    Ok(())
                })?;
                
                Ok(())
            }

            if let Err(e) = save_process(&save_name, objects) {
                eprintln!("Failed to save game: {}", e);
            } else {
                println!("Game saved successfully to '{}'", save_name);
                increment_db_generation();
            }
        });
    }
}

