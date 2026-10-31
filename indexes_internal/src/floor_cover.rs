//! Maintains `FloorCoverGrid` through `FloorCover` insertion and removal observers.
//! Replacing the component removes its old entries before indexing the new kind.
//!
//! Shaders read a flat entry list with offsets for each cell. Entries contain the cover kind,
//! floor, and wrapped lifetime timestamps, repeated for each covered cell. Each upload scans
//! the entire grid and hides the canvas if there are no entries.

use bevy::prelude::*;
use bytemuck::{Pod, Zeroable};

use game_core::{prelude::{GridArea, MapInfo}, request_flag::RequestFlag};
use indexes::floor_cover::{FloorCover, FloorCoverGrid};
use logging::prelude::*;
use session::{GameClock, TimeBound};
use states::prelude::MapLoadingStage;
use visuals::shared_gpu::{SharedGpuBinding, SharedGpuWriter};

use crate::floor_cover_canvas::FloorCoverCanvas;

pub(crate) struct FloorCoverPlugin;
impl Plugin for FloorCoverPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<FloorCoverGrid>()
            .init_resource::<RequestFlag<FloorCoverGpuUpload>>()
            .add_systems(OnExit(MapLoadingStage::LoadMapInfo), create_floor_cover_grid)
            .add_systems(PostUpdate, upload_gpu_floor_cover.run_if(RequestFlag::<FloorCoverGpuUpload>::is_requested))
            .add_observer(on_insert_floor_cover_imprint_on_grid)
            .add_observer(on_discard_floor_cover_deprint_from_grid)
            .add_observer(on_insert_time_bound_request_floor_cover_gpu_upload);
    }
}

/// End-time sentinel for entries without `TimeBound`, outside the wrapped game time range.
/// Mirrored by `FLOOR_COVER_PERMANENT_END` in `assets/shaders/grids/floor_cover.wesl`.
const PERMANENT_END: f32 = -1.;

/// GPU representation of a floor cover entry. Field order and types match `FloorCoverEntry` in
/// `assets/shaders/grids/floor_cover.wesl`.
#[derive(Clone, Copy, Default, Pod, Zeroable)]
#[repr(C)]
struct GpuFloorCoverEntry {
    kind: u32,
    /// Wrapped game time.
    starts_at: f32,
    /// Wrapped game time, or `PERMANENT_END`.
    ends_at: f32,
    /// Floor code from `FloorLevel::gpu_index`.
    floor: u32,
}

/// Request marker for a full floor cover upload.
struct FloorCoverGpuUpload;

fn create_floor_cover_grid(
    mut commands: Commands,
    map_info: Res<MapInfo>,
    mut upload_requested: ResMut<RequestFlag<FloorCoverGpuUpload>>,
) {
    commands.insert_resource(FloorCoverGrid::new_with_size(map_info.grid_bounds));
    upload_requested.request();
}

#[log_tags(Tag::MapObjects)]
fn on_insert_floor_cover_imprint_on_grid(
    trigger: On<Insert<FloorCover>>,
    mut floor_cover_grid: ResMut<FloorCoverGrid>,
    mut upload_requested: ResMut<RequestFlag<FloorCoverGpuUpload>>,
    floor_covers: Query<(&FloorCover, &GridArea)>,
) {
    let entity = trigger.entity;
    #[warn_dev("Floor cover {entity} has no area; not imprinted on the floor cover grid")]
    let Ok((floor_cover, area)) = floor_covers.get(entity) else { return; };
    floor_cover_grid.imprint(*area, entity, floor_cover.0);
    upload_requested.request();
}

fn on_discard_floor_cover_deprint_from_grid(
    trigger: On<Discard<FloorCover>>,
    mut floor_cover_grid: ResMut<FloorCoverGrid>,
    mut upload_requested: ResMut<RequestFlag<FloorCoverGpuUpload>>,
    floor_covers: Query<&GridArea, With<FloorCover>>,
) {
    let entity = trigger.entity;
    let Ok(area) = floor_covers.get(entity) else { return; };
    floor_cover_grid.deprint(*area, entity);
    upload_requested.request();
}

/// Refreshes GPU lifetimes when `TimeBound` is inserted or replaced.
/// Removing `TimeBound` does not request an upload, so its previous values persist until the next one.
fn on_insert_time_bound_request_floor_cover_gpu_upload(
    trigger: On<Insert<TimeBound>>,
    mut upload_requested: ResMut<RequestFlag<FloorCoverGpuUpload>>,
    floor_covers: Query<(), With<FloorCover>>,
) {
    if !floor_covers.contains(trigger.entity) { return; }
    upload_requested.request();
}

/// Uploads the full grid and updates canvas visibility.
fn upload_gpu_floor_cover(
    mut shared_gpu: SharedGpuWriter,
    mut upload_requested: ResMut<RequestFlag<FloorCoverGpuUpload>>,
    floor_cover_grid: Res<FloorCoverGrid>,
    time_bounds: Query<&TimeBound, With<FloorCover>>,
    canvas: Option<Single<&mut Visibility, With<FloorCoverCanvas>>>,
) {
    upload_requested.clear();

    let mut offsets = Vec::with_capacity(floor_cover_grid.grid.len() + 1);
    let mut entries = Vec::new();
    for cell in floor_cover_grid.grid.iter() {
        offsets.push(entries.len() as u32);
        entries.extend(cell.iter().map(|entry| {
            let (starts_at, ends_at) = match time_bounds.get(entry.entity) {
                Ok(time_bound) => (GameClock::wrap(time_bound.starts_at), GameClock::wrap(time_bound.ends_at)),
                Err(_) => (0., PERMANENT_END),
            };
            GpuFloorCoverEntry { kind: entry.kind.gpu_index(), starts_at, ends_at, floor: entry.level.gpu_index() }
        }));
    }
    offsets.push(entries.len() as u32);
    if let Some(mut canvas_visibility) = canvas {
        canvas_visibility.set_if_neq(if entries.is_empty() { Visibility::Hidden } else { Visibility::Inherited });
    }
    shared_gpu.write(SharedGpuBinding::FloorCoverCellOffsets, offsets);
    shared_gpu.write_with_capacity(SharedGpuBinding::FloorCoverCellEntries, entries);
}
