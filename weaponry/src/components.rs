use bevy::prelude::*;
use game_core::prelude::MapBound;

#[derive(Component, Default)]
#[require(MapBound)]
pub struct Projectile;
