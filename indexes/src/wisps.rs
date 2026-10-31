use std::fmt::Debug;

use bevy::prelude::*;

use game_core::prelude::{Bounds, GridPosition, VolumeLevel};
use grids::{GridVersion, base::BaseGrid};

/// Wisp membership per cell and volume, with a shared version and a version for each volume.
///
/// Each volume has a separate grid so scans read consecutive cell lists without stepping over
/// data for other volumes.
#[derive(Resource, Default)]
pub struct WispsGrid {
    underground: BaseGrid<Vec<Entity>, GridVersion>,
    elevation: BaseGrid<Vec<Entity>, GridVersion>,
    on_elevation: BaseGrid<Vec<Entity>, GridVersion>,
    pub bounds: Bounds,
    /// Advances when membership changes in any volume.
    pub version: GridVersion,
}

impl WispsGrid {
    pub fn new_with_size(bounds: impl Into<Bounds>) -> Self {
        let bounds = bounds.into();
        Self {
            underground: BaseGrid::new_with_size(bounds),
            elevation: BaseGrid::new_with_size(bounds),
            on_elevation: BaseGrid::new_with_size(bounds),
            bounds,
            version: GridVersion::default(),
        }
    }

    /// Version of the membership lists at `level`; changes also include moves within that volume.
    pub fn level_version(&self, level: VolumeLevel) -> GridVersion {
        self.volume(level).version
    }

    /// Returns the wisps in one cell volume.
    ///
    /// Coordinates must be in bounds. Panics if the position's level is not a volume.
    pub fn wisps_at(&self, position: impl TryInto<GridPosition<VolumeLevel>, Error: Debug>) -> &[Entity] {
        let position = position.try_into().expect("wisps are in volumes");
        &self.volume(position.level)[position.coords]
    }

    pub fn has_wisps(&self, position: impl TryInto<GridPosition<VolumeLevel>, Error: Debug>) -> bool {
        !self.wisps_at(position).is_empty()
    }

    pub fn wisp_add(&mut self, position: impl TryInto<GridPosition<VolumeLevel>, Error: Debug>, wisp: Entity) {
        let position = position.try_into().expect("wisps are in volumes");
        let volume = self.volume_mut(position.level);
        volume[position.coords].push(wisp);
        volume.advance_version();
        self.version = self.version.wrapping_add(1);
    }

    pub fn wisp_remove(&mut self, position: impl TryInto<GridPosition<VolumeLevel>, Error: Debug>, wisp: Entity) {
        let position = position.try_into().expect("wisps are in volumes");
        let volume = self.volume_mut(position.level);
        let cell = &mut volume[position.coords];
        let Some(index) = cell.iter().position(|&listed| listed == wisp) else { return; };
        cell.swap_remove(index);
        volume.advance_version();
        self.version = self.version.wrapping_add(1);
    }

    pub fn wisp_move(
        &mut self,
        from: impl TryInto<GridPosition<VolumeLevel>, Error: Debug>,
        to: impl TryInto<GridPosition<VolumeLevel>, Error: Debug>,
        wisp: Entity,
    ) {
        self.wisp_remove(from, wisp);
        self.wisp_add(to, wisp);
    }

    fn volume(&self, level: VolumeLevel) -> &BaseGrid<Vec<Entity>, GridVersion> {
        match level {
            VolumeLevel::Underground => &self.underground,
            VolumeLevel::Elevation => &self.elevation,
            VolumeLevel::OnElevation => &self.on_elevation,
        }
    }

    fn volume_mut(&mut self, level: VolumeLevel) -> &mut BaseGrid<Vec<Entity>, GridVersion> {
        match level {
            VolumeLevel::Underground => &mut self.underground,
            VolumeLevel::Elevation => &mut self.elevation,
            VolumeLevel::OnElevation => &mut self.on_elevation,
        }
    }
}
