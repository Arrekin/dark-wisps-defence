//! # Flame Stream
//!
//! Fuel advancement and cone hit detection for `FlameStream` entities.
//!
//! - **Fuel:** Advances `head` (and `tail` while venting) outward from `nozzle` at `FUEL_SPEED`.
//!   Emits `FlameStreamBurntOut` on the generator entity once a venting stream's `tail` passes
//!   `length + BURNOUT_DISTANCE`.
//! - **Hit detection:** Steps grid columns along the stream's dominant axis from `nozzle` to
//!   `length` and finds the smallest axial distance within the cone and active `fuel` range.
//!
//! Visuals live in `flame_stream_fire`. Not persisted; recreated by the owning tower on load.

use std::ops::RangeInclusive;

use bevy::prelude::*;

use game_core::prelude::{CELL_SIZE, GridArea, GridCoords, GridImprint};
use grids::wisps::WispsGrid;
use states::prelude::GameState;
use weaponry::{
    flame_stream::{FlameHit, FlameStreamBurntOut, FlameStreamGeneratedBy, FlameStreamSystems},
    prelude::*,
};
use wisps::prelude::Wisp;

pub(crate) struct FlameStreamPlugin;
impl Plugin for FlameStreamPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Update, (
                advance_flame_streams,
                update_flame_stream_hits,
            ).chain().in_set(FlameStreamSystems).run_if(in_state(GameState::Running)));
    }
}

/// Fuel speed in world units per second.
const FUEL_SPEED: f32 = 1200.;
const WISP_HIT_RADIUS: f32 = 8.;

fn advance_flame_streams(
    mut commands: Commands,
    time: Res<Time>,
    mut streams: Query<(&mut FlameStream, &FlameStreamGeneratedBy)>,
) {
    let travel = FUEL_SPEED * time.delta_secs();
    for (mut stream, generated_by) in streams.iter_mut() {
        if stream.advance(travel) {
            commands.trigger(FlameStreamBurntOut { tower: generated_by.0 });
        }
    }
}

fn update_flame_stream_hits(
    wisps_grid: Res<WispsGrid>,
    mut streams: Query<&mut FlameStream>,
    wisps: Query<&Transform, With<Wisp>>,
) {
    for mut stream in streams.iter_mut() {
        let hit = stream.fuel().and_then(|fuel| nearest_hit(&stream, fuel, &wisps_grid, &wisps));
        stream.hit = hit;
    }
}

/// Axial distance from `nozzle` to `position` if `position` lies within the stream cone.
fn hit_distance(stream: &FlameStream, position: Vec2) -> Option<f32> {
    let offset = position - stream.nozzle;
    let distance = offset.dot(stream.direction);
    let lateral = offset.perp_dot(stream.direction).abs();
    ((0.0..=stream.length).contains(&distance) && lateral <= stream.half_width_at(distance) + WISP_HIT_RADIUS)
        .then_some(distance)
}

/// Finds the wisp with the smallest axial distance covered by `fuel` inside the stream cone.
fn nearest_hit(stream: &FlameStream, fuel: RangeInclusive<f32>, wisps_grid: &WispsGrid, wisps: &Query<&Transform, With<Wisp>>) -> Option<FlameHit> {
    let mut nearest: Option<FlameHit> = None;
    for (column_distance, slice) in columns(stream) {
        if nearest.as_ref().is_some_and(|hit| column_distance > hit.distance) { break; }
        for coords in slice.iter_in_bounds(wisps_grid.bounds) {
            for &wisp in &wisps_grid[coords] {
                let Ok(wisp_transform) = wisps.get(wisp) else { continue; };
                let Some(hit_distance) = hit_distance(stream, wisp_transform.translation.xy()) else { continue; };
                if !fuel.contains(&hit_distance) { continue; }
                if nearest.as_ref().is_none_or(|hit| hit_distance < hit.distance) {
                    nearest = Some(FlameHit { coords, distance: hit_distance });
                }
            }
        }
    }
    nearest
}

/// Grid slices perpendicular to the stream's dominant axis, ordered from nozzle to tip.
/// Yields `(min_axial_distance, slice)` so callers can stop once
/// `min_axial_distance` exceeds the best hit found so far.
fn columns(stream: &FlameStream) -> impl Iterator<Item = (f32, GridArea)> {
    let nozzle = stream.nozzle;
    let direction = stream.direction;
    let length = stream.length;
    let along = if direction.x.abs() >= direction.y.abs() { 0 } else { 1 };
    let across = 1 - along;
    let max_reach = stream.half_width_at(length) + WISP_HIT_RADIUS;
    // Max axial distance difference between an off-center wisp and the centerline at the same `along` coordinate.
    let max_lead = max_reach * (direction[across] / direction[along]).abs();
    let cell_of = move |world: f32| (world / CELL_SIZE).floor() as i32;
    let distance_of = move |world: f32| (world - nozzle[along]) / direction[along];
    let step = direction[along].signum() as i32;
    let first = cell_of(nozzle[along] - max_reach * step as f32);
    let last = cell_of(nozzle[along] + direction[along] * length + max_reach * step as f32);
    (0..=(last - first) * step).map(move |index| {
        let column = first + index * step;
        let edges = [distance_of(column as f32 * CELL_SIZE), distance_of((column + 1) as f32 * CELL_SIZE)];
        let near = (edges[0].min(edges[1]) - max_lead).clamp(0., length);
        let far = (edges[0].max(edges[1]) + max_lead).clamp(0., length);
        let reach = stream.half_width_at(far) + WISP_HIT_RADIUS;
        let ends = [near, far].map(|distance| nozzle[across] + direction[across] * distance);
        let low = cell_of(ends[0].min(ends[1]) - reach);
        let high = cell_of(ends[0].max(ends[1]) + reach);
        let slice = if along == 0 {
            GridArea::new(GridCoords { x: column, y: low }, GridImprint::Rectangle { width: 1, height: high - low + 1 })
        } else {
            GridArea::new(GridCoords { x: low, y: column }, GridImprint::Rectangle { width: high - low + 1, height: 1 })
        };
        (near, slice)
    })
}
