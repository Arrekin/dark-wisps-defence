use std::iter;

use bevy::{ecs::system::SystemParam, prelude::*};

use game_core::prelude::{GridCoords, VolumeLevel};
use indexes::{floor_cover::FloorCoverGrid, wisps::WispsGrid};

/// Clears wisps and floor covers from a chosen volume upward within one cell.
#[derive(SystemParam)]
pub(crate) struct CellClearing<'w, 's> {
    commands: Commands<'w, 's>,
    wisps_grid: ResMut<'w, WispsGrid>,
    floor_cover_grid: Res<'w, FloorCoverGrid>,
}
impl CellClearing<'_, '_> {
    /// Removes wisps from the spatial index and queues their despawn from `lowest` upward.
    /// Also queues despawn of floor covers on each affected volume's floor.
    /// `coords` must be in bounds.
    pub(crate) fn clear_from(&mut self, coords: GridCoords, lowest: VolumeLevel) {
        for level in iter::successors(Some(lowest), |level| level.above()) {
            let position = coords.at(level);
            for wisp in self.wisps_grid.wisps_at(position).to_vec() {
                self.wisps_grid.wisp_remove(position, wisp);
                self.commands.entity(wisp).despawn();
            }
            for entry in self.floor_cover_grid.entries_at(coords).iter().filter(|entry| entry.level == level.floor()) {
                self.commands.entity(entry.entity).despawn();
            }
        }
    }
}
