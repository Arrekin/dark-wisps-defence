use crate::lib_prelude::*;
use serde::{Deserialize, Serialize};
use strum::{AsRefStr, EnumIter, EnumString};


pub mod wisps_prelude {
    pub use super::WispType;
}

/// Wisp type enum - identifies different wisp variants.
#[derive(Component, Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, EnumString, EnumIter, AsRefStr)]
pub enum WispType {
    Fire,
    Water,
    Light,
    Electric,
}


#[derive(Component)]
pub struct WispFireType;
#[derive(Component)]
pub struct WispWaterType;
#[derive(Component)]
pub struct WispLightType;
#[derive(Component)]
pub struct WispElectricType;
