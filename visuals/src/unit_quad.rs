use bevy::prelude::*;

/// Shared 1×1 quad mesh centered at the origin. Consumers scale it via `Transform`.
#[derive(Resource)]
pub struct UnitQuad(Handle<Mesh>);
impl UnitQuad {
    pub fn mesh(&self) -> Mesh2d {
        Mesh2d(self.0.clone())
    }
}
impl FromWorld for UnitQuad {
    fn from_world(world: &mut World) -> Self {
        Self(world.resource_mut::<Assets<Mesh>>().add(Rectangle::new(1., 1.)))
    }
}
