use bevy::prelude::*;

pub trait SSS: Send + Sync + 'static {}

// Simple property trait for single value objects. Useful in generic contexts.
pub trait Property {
    fn get(&self) -> f32;
    fn set(&mut self, value: f32);
    fn new(value: f32) -> Self;
}

/// Inserts an optional bundle, so builder observers chain optional components without `if let` blocks.
pub trait InsertSome {
    fn insert_some(&mut self, bundle: Option<impl Bundle>) -> &mut Self;
}
impl InsertSome for EntityCommands<'_> {
    fn insert_some(&mut self, bundle: Option<impl Bundle>) -> &mut Self {
        match bundle {
            Some(bundle) => self.insert(bundle),
            None => self,
        }
    }
}
