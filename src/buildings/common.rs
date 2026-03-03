use crate::prelude::*;

// Building sub-parts markers
#[derive(Component)]
pub struct MarkerTowerRotationalTop(pub Entity);


#[derive(Component)]
pub struct TowerTopRotation {
    pub speed: f32, // in radians per second
    pub current_angle: f32,
}
#[derive(EntityEvent)]
pub struct BuildingDestroyRequest(pub Entity);
