use crate::prelude::*;

#[derive(Component, Default)]
pub struct ExpeditionZone {
    pub expeditions_arrived: u32, // How many expeditions have arrived but was not yet processed
}