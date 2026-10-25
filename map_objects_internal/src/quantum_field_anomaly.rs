//! # Quantum Field Anomaly
//!
//! `QuantumFieldAnomaly` redraws the scene beneath a field with refraction, superposition ghosts,
//! decoherence and a glowing lattice. It runs as an overlay before the air band, sampling the
//! ground and surface bands. The shader shapes each field with an axis-aligned box distance
//! function and reduces distortion as `QuantumFieldLayers::solve_progress` increases.
//!
//! Each field stores the active drone scan spots within its bounds. The shader suppresses
//! superposition around those spots.

use bevy::{prelude::*, reflect::TypePath, render::render_resource::{BlendState, ShaderType}};

use game_core::prelude::{GridArea, ZDepth};
use units::expedition_drone::{ExpeditionDrone, ScanSpot, ScanningBeam};
use visuals::{
    lane::{LaneParts, LanePlugin, LaneVisual},
    shader::seed_from_entity,
};

use crate::quantum_field::QuantumFieldLayers;

pub(crate) struct QuantumFieldAnomalyPlugin;
impl Plugin for QuantumFieldAnomalyPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins(LanePlugin::<QuantumFieldAnomaly>::default())
            .add_systems(Update, update_quantum_field_anomalies);
    }
}

/// Maximum simultaneous scan spots per field. Mirrors `MAX_COLLAPSE_POINTS` in
/// `assets/shaders/quantum_field/anomaly.wesl`.
const MAX_COLLAPSE_POINTS: usize = 4;
/// Quad margin past the rectangle: `BOUNDARY_OUTER_BAND` plus the edge jitter of
/// `quantum_field_masks` in `assets/shaders/quantum_field/look.wesl`.
const QUAD_MARGIN: f32 = 16.;

/// Visual data stored on the quantum field entity. Field order and types mirror
/// `Instance` in `assets/shaders/quantum_field/anomaly.wesl`.
#[derive(Component, ShaderType, TypePath, Clone, Default)]
#[require(ZDepth::AirEffect)]
pub(crate) struct QuantumFieldAnomaly {
    /// World-space center used to compare overlapping fields.
    center: Vec2,
    half_extent: Vec2,
    /// Solve progress from 0 (unsolved) to 1 (solved); reduces visual distortion.
    solve_progress: f32,
    /// Per-field noise offset.
    seed: f32,
    collapse_count: u32,
    /// Ground positions of the scan spots inside the field.
    collapse_points: [Vec2; MAX_COLLAPSE_POINTS],
}
impl LaneVisual for QuantumFieldAnomaly {
    const LABEL: &'static str = "quantum_field_anomaly";
    const LOOK: &'static str = "shaders::quantum_field::anomaly";
    const PARTS: LaneParts = LaneParts::OVERLAY;
    const READS_ALL_INSTANCES: bool = true;

    fn cut_quad_size(&self) -> Vec2 {
        2. * (self.half_extent + QUAD_MARGIN)
    }

    fn overlay_blend() -> BlendState {
        BlendState::REPLACE
    }
}

fn update_quantum_field_anomalies(
    mut fields: Query<(Entity, &QuantumFieldLayers, &GridArea, &mut QuantumFieldAnomaly)>,
    beams: Query<&ScanningBeam>,
    drones: Query<&ExpeditionDrone>,
    spots: Query<&Transform, With<ScanSpot>>,
) {
    let scan_spots: Vec<Vec2> = beams.iter()
        .filter(|beam| drones.get(beam.drone).is_ok_and(|drone| drone.is_beam_active))
        .filter_map(|beam| spots.get(beam.spot).ok())
        .map(|transform| transform.translation.xy())
        .collect();

    for (field, layers, area, mut anomaly) in fields.iter_mut() {
        let field_rect = area.world_rect();
        let (center, half_extent) = (field_rect.center(), field_rect.half_size());
        let mut collapse_points = [Vec2::ZERO; MAX_COLLAPSE_POINTS];
        let mut collapse_count = 0;
        for spot in scan_spots.iter().filter(|spot| field_rect.contains(**spot)).take(MAX_COLLAPSE_POINTS) {
            collapse_points[collapse_count] = *spot;
            collapse_count += 1;
        }
        *anomaly = QuantumFieldAnomaly {
            center,
            half_extent,
            solve_progress: layers.solve_progress(),
            seed: seed_from_entity(field),
            collapse_count: collapse_count as u32,
            collapse_points,
        };
    }
}
