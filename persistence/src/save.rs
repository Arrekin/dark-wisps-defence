use bevy::{
    input::common_conditions::input_just_released,
    prelude::*,
};

use game_core::prelude::*;
use logging::prelude::*;

use crate::common::{db_migrations, with_db_connection};

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
pub(crate) trait SaveableBatch: SSS {
    fn save_batch(self: Box<Self>, tx: &rusqlite::Transaction) -> rusqlite::Result<()>;
}


#[derive(Message)]
pub(crate) struct SaveGameSignal;
impl SaveGameSignal {
    fn emit(
        mut writer: MessageWriter<SaveGameSignal>,
        mut save_executor: ResMut<GameSaveExecutor>,
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
    type Out = ();
    fn apply(self, world: &mut World) {
        let mut buffer = world.resource_mut::<GameSaveExecutor>();
        Log::debug().dev().tag(Tag::GameSave).message(format!("Queued {} objects for saving", self.data.len()));
        // Box since we store dyn in GameSaveExecutor
        buffer.objects_to_save.push(Box::new(self));
    }
}
#[derive(Resource, Default)]
pub(crate) struct GameSaveExecutor {
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
        Log::info().dev().tag(Tag::GameSave).message(format!("Saving game to '{save_name}'"));

        std::thread::spawn(move || {
            fn save_process(save_name: &str, objects: Vec<Box<dyn SaveableBatch>>) -> Result<(), Box<dyn std::error::Error>> {
                if std::path::Path::new(save_name).exists() {
                    Log::debug().dev().tag(Tag::GameSave).message(format!("Replacing existing save file '{save_name}'"));
                    std::fs::remove_file(save_name)?;
                }
                
                with_db_connection(save_name, |conn| {
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
                Log::error().dev().tag(Tag::GameSave).message(format!("Save failed: {e}"));
            } else {
                Log::info().player().tag(Tag::GameSave).message(format!("Game saved to '{save_name}'"));
            }
        });
    }
}

