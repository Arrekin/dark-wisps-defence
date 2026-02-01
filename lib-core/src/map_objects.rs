use crate::lib_prelude::*;


#[derive(Component)]
#[require(MapBound, ObstacleGridObject = ObstacleGridObject::DarkOre)]
pub struct DarkOre {
    pub amount: i32,
}


#[derive(Component)]
#[require(MapBound, ObstacleGridObject = ObstacleGridObject::Wall, EmissionsGridSpreadAffector)]
pub struct Wall;

/// Progressive obstacle requiring drone scanning to solve.
/// Layers are defined at spawn time; current_layer indexes into the layers vec.
#[derive(Component, Default)]
#[require(MapBound, ObstacleGridObject = ObstacleGridObject::QuantumField)]
pub struct QuantumField;