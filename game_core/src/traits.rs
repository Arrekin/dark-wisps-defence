use bevy::{
    ecs::relationship::Relationship,
    prelude::*,
};

pub trait SSS: Send + Sync + 'static {}

// Simple property trait for single value objects. Useful in generic contexts.
pub trait Property {
    fn get(&self) -> f32;
    fn set(&mut self, value: f32);
    fn new(value: f32) -> Self;
}

/// Entity commands that do nothing on `None`. Keeps builder chains free of `if let`.
pub trait OptionalCommands {
    fn insert_some(&mut self, bundle: Option<impl Bundle>) -> &mut Self;
    /// Spawns a clone of each bundle as an entity related to this one through `R`.
    fn with_related_some<'a, R: Relationship>(&mut self, bundles: Option<impl IntoIterator<Item = &'a (impl Bundle + Clone)>>) -> &mut Self;
}
impl OptionalCommands for EntityCommands<'_> {
    fn insert_some(&mut self, bundle: Option<impl Bundle>) -> &mut Self {
        match bundle {
            Some(bundle) => self.insert(bundle),
            None => self,
        }
    }

    fn with_related_some<'a, R: Relationship>(&mut self, bundles: Option<impl IntoIterator<Item = &'a (impl Bundle + Clone)>>) -> &mut Self {
        let Some(bundles) = bundles else { return self };
        self.with_related_entities::<R>(|related| {
            for bundle in bundles {
                related.spawn(bundle.clone());
            }
        })
    }
}
