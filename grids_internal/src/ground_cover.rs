//! Keeps `GroundCoverGrid` in sync with ground cover entities. Replacing `GroundCover` removes
//! the old entries before adding the new kind. Ground cover must stay on its original cells:
//! changes to `GridCoords` or `GridImprint` do not update the index.
//!
//! Shaders read one entry list indexed by per-cell offsets. Each entry contains its kind and
//! wrapped lifetime timestamps, and appears once per covered cell. Grid changes and lifetime
//! updates trigger a full upload, so each upload scans the entire map.
//! The ground cover canvas is hidden when the grid is empty.

use bevy::prelude::*;
use bytemuck::{Pod, Zeroable};

use game_core::prelude::{GridCoords, GridImprint, MapInfo};
use grids::ground_cover::{GroundCover, GroundCoverEntry, GroundCoverGrid};
use session::{GameClock, TimeBound};
use states::prelude::MapLoadingStage;
use visuals::shared_gpu::{SharedGpuBinding, SharedGpuWriter};

use crate::ground_cover_canvas::GroundCoverCanvas;

pub(crate) struct GroundCoverPlugin;
impl Plugin for GroundCoverPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<GroundCoverGrid>()
            .init_resource::<GroundCoverGpuUploadRequested>()
            .add_systems(OnExit(MapLoadingStage::LoadMapInfo), |mut commands: Commands, map_info: Res<MapInfo>| { commands.insert_resource(GroundCoverGrid::new_with_size(map_info.grid_bounds)); })
            .add_systems(PostUpdate, upload_gpu_ground_cover.run_if(resource_changed::<GroundCoverGrid>.or_else(GroundCoverGpuUploadRequested::is_requested)))
            .add_observer(on_insert_ground_cover_imprint_on_grid)
            .add_observer(on_discard_ground_cover_deprint_from_grid)
            .add_observer(on_insert_time_bound_request_ground_cover_gpu_upload);
    }
}

/// End time of an entry without a `TimeBound`. Outside the wrapped game time range.
/// Mirrored by `GROUND_COVER_PERMANENT_END` in `assets/shaders/grids/ground_cover.wesl`.
const PERMANENT_END: f32 = -1.;

/// One ground cover entry. Field order and types mirror `GroundCoverEntry` in
/// `assets/shaders/grids/ground_cover.wesl`.
#[derive(Clone, Copy, Default, Pod, Zeroable)]
#[repr(C)]
struct GpuGroundCoverEntry {
    kind: u32,
    /// Wrapped game time.
    starts_at: f32,
    /// Wrapped game time, or `PERMANENT_END`.
    ends_at: f32,
    _padding: u32,
}

/// Requests one GPU upload per frame when ground cover lifetimes change.
#[derive(Resource, Default)]
struct GroundCoverGpuUploadRequested(bool);
impl GroundCoverGpuUploadRequested {
    fn request(&mut self) {
        self.0 = true;
    }

    fn clear(&mut self) {
        self.0 = false;
    }

    fn is_requested(requested: Res<Self>) -> bool {
        requested.0
    }
}

fn on_insert_ground_cover_imprint_on_grid(
    trigger: On<Insert<GroundCover>>,
    mut ground_cover_grid: ResMut<GroundCoverGrid>,
    ground_covers: Query<(&GroundCover, &GridCoords, &GridImprint)>,
) {
    let entity = trigger.entity;
    let Ok((ground_cover, grid_coords, grid_imprint)) = ground_covers.get(entity) else { return; };
    ground_cover_grid.imprint(*grid_coords, *grid_imprint, GroundCoverEntry { entity, kind: ground_cover.0 });
}

fn on_discard_ground_cover_deprint_from_grid(
    trigger: On<Discard<GroundCover>>,
    mut ground_cover_grid: ResMut<GroundCoverGrid>,
    ground_covers: Query<(&GridCoords, &GridImprint), With<GroundCover>>,
) {
    let entity = trigger.entity;
    let Ok((grid_coords, grid_imprint)) = ground_covers.get(entity) else { return; };
    ground_cover_grid.deprint(*grid_coords, *grid_imprint, entity);
}

fn on_insert_time_bound_request_ground_cover_gpu_upload(
    trigger: On<Insert<TimeBound>>,
    mut upload_requested: ResMut<GroundCoverGpuUploadRequested>,
    ground_covers: Query<(), With<GroundCover>>,
) {
    if !ground_covers.contains(trigger.entity) { return; }
    upload_requested.request();
}

/// Uploads the full grid and updates canvas visibility.
///
/// Removing `TimeBound` from a live entity does not trigger an upload; its old lifetime remains
/// on the GPU until the next upload.
fn upload_gpu_ground_cover(
    mut shared_gpu: SharedGpuWriter,
    mut upload_requested: ResMut<GroundCoverGpuUploadRequested>,
    ground_cover_grid: Res<GroundCoverGrid>,
    time_bounds: Query<&TimeBound, With<GroundCover>>,
    canvas: Option<Single<&mut Visibility, With<GroundCoverCanvas>>>,
) {
    upload_requested.clear();

    let mut offsets = Vec::with_capacity(ground_cover_grid.grid.len() + 1);
    let mut entries = Vec::new();
    for cell in ground_cover_grid.grid.iter() {
        offsets.push(entries.len() as u32);
        entries.extend(cell.iter().map(|entry| {
            let (starts_at, ends_at) = match time_bounds.get(entry.entity) {
                Ok(time_bound) => (GameClock::wrap(time_bound.starts_at), GameClock::wrap(time_bound.ends_at)),
                Err(_) => (0., PERMANENT_END),
            };
            GpuGroundCoverEntry { kind: entry.kind as u32, starts_at, ends_at, _padding: 0 }
        }));
    }
    offsets.push(entries.len() as u32);
    if let Some(mut canvas_visibility) = canvas {
        canvas_visibility.set_if_neq(if entries.is_empty() { Visibility::Hidden } else { Visibility::Inherited });
    }
    shared_gpu.write(SharedGpuBinding::GroundCoverCellOffsets, offsets);
    shared_gpu.write_with_capacity(SharedGpuBinding::GroundCoverCellEntries, entries);
}
