//! A progress bar drawn as a raked track carrying three positions along its length.
//!
//! Set [`ProgressBar::set_fraction`] for how far the work has got, and
//! [`ProgressBar::set_reachable`] for how far it could get with the resources currently
//! held. Between them the bar shows a runway; when the two meet, the runway has closed and
//! the work cannot advance.
//!
//! Reachable drives two positions at different speeds — a marker that tracks it closely and
//! a filled band that follows more slowly — so resources arriving open a gap that then
//! flows shut. The layers are described in `assets/shaders/progress_bar.wgsl`.
//!
//! Values are stamped as fades and interpolated by the shader, so a bar costs no CPU
//! between changes and none at all while nothing is moving.

use bevy::{
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
    ui_render::ui_material::UiMaterial,
};

use crate::common::fade::{Fade, FadeState};

const SHADER_ASSET_PATH: &str = "shaders/progress_bar.wgsl";

/// Exponential ease rates per second. The marker follows affordability promptly while the
/// slower band makes changes in reachable progress visible.
const EASE_PROGRESS: f32 = 5.0;
const EASE_MARKER: f32 = 8.0;
const EASE_BAND: f32 = 2.0;

// ============================================================================
// MATERIAL — the GPU uniform
// ============================================================================

/// Shape of the track and the lines drawn on it.
#[derive(ShaderType, Clone, Copy, Debug)]
pub struct ProgressBarGeometry {
    /// Horizontal shift of the top edge relative to the bottom, as a fraction of the track's
    /// height. 0 is a plain rectangle. The track's ends carry this too, which is what keeps
    /// a full bar flush with its end instead of leaving a wedge.
    pub rake: f32,
    pub rail_thickness: f32,
    pub edge_width: f32,
    pub marker_width: f32,
}

/// Relative brightnesses. See the shader for what each one weights.
#[derive(ShaderType, Clone, Copy, Debug)]
pub struct ProgressBarShading {
    pub edge_falloff: f32,
    pub fill_gradient: f32,
    pub runway_gain: f32,
    pub inert_gain: f32,
}

/// Graduation marks, and how close the progress edge must be to the marker to count as
/// stalled.
#[derive(ShaderType, Clone, Copy, Debug)]
pub struct ProgressBarDetail {
    pub tick_length: f32,
    pub tick_width: f32,
    pub tick_gain: f32,
    /// Runway width in pixels at or below which the progress edge counts as stalled.
    pub stall_gap: f32,
}

/// GPU-side uniform. Field order and types mirror `ProgressBarMaterial` in
/// `assets/shaders/progress_bar.wgsl`.
///
/// Every member is 16 bytes. A narrower one shifts everything after it and panics when the
/// buffer is prepared.
#[derive(AsBindGroup, Asset, TypePath, Clone, Copy, Debug)]
pub struct ProgressBarMaterial {
    #[uniform(0)]
    pub fill_color: LinearRgba,
    #[uniform(0)]
    pub edge_color: LinearRgba,
    #[uniform(0)]
    pub track_color: LinearRgba,
    #[uniform(0)]
    pub rail_color: LinearRgba,
    #[uniform(0)]
    pub marker_color: LinearRgba,
    #[uniform(0)]
    pub stall_color: LinearRgba,
    #[uniform(0)]
    pub geometry: ProgressBarGeometry,
    #[uniform(0)]
    pub shading: ProgressBarShading,
    #[uniform(0)]
    pub detail: ProgressBarDetail,
    #[uniform(0)]
    pub progress_fade: Fade,
    #[uniform(0)]
    pub marker_fade: Fade,
    #[uniform(0)]
    pub band_fade: Fade,
}

impl UiMaterial for ProgressBarMaterial {
    fn fragment_shader() -> ShaderRef {
        SHADER_ASSET_PATH.into()
    }
}

// ============================================================================
// RUNTIME COMPONENT
// ============================================================================

/// A progress bar surface. The sync system in `widgets_internal` stamps fades into the
/// material on the frames a value actually moves.
#[derive(Component, Clone, Copy, Debug)]
pub struct ProgressBar {
    fill_color: LinearRgba,
    edge_color: LinearRgba,
    track_color: LinearRgba,
    rail_color: LinearRgba,
    marker_color: LinearRgba,
    stall_color: LinearRgba,
    geometry: ProgressBarGeometry,
    shading: ProgressBarShading,
    detail: ProgressBarDetail,
    progress: FadeState,
    marker: FadeState,
    band: FadeState,
}

impl ProgressBar {
    /// How far the work has got, 0..=1. Returns whether it moved, so a caller writing every
    /// frame can skip marking the component changed when nothing did.
    pub fn set_fraction(&mut self, fraction: f32) -> bool {
        self.progress.set_target(fraction.clamp(0., 1.))
    }

    /// How far the work could get with what is currently held, 0..=1. Equal to the fraction
    /// means no runway is left.
    pub fn set_reachable(&mut self, reachable: f32) -> bool {
        let reachable = reachable.clamp(0., 1.);
        // Bitwise OR, not `||`: both calls must execute for their side effects.
        self.marker.set_target(reachable) | self.band.set_target(reachable)
    }

    pub fn begin_fades(&mut self, now: f32) {
        self.progress.begin_fade(now);
        self.marker.begin_fade(now);
        self.band.begin_fade(now);
    }

    pub fn to_material(&self) -> ProgressBarMaterial {
        ProgressBarMaterial {
            fill_color: self.fill_color,
            edge_color: self.edge_color,
            track_color: self.track_color,
            rail_color: self.rail_color,
            marker_color: self.marker_color,
            stall_color: self.stall_color,
            geometry: self.geometry,
            shading: self.shading,
            detail: self.detail,
            progress_fade: self.progress.fade(),
            marker_fade: self.marker.fade(),
            band_fade: self.band.fade(),
        }
    }
}

// ============================================================================
// SPAWN CONTRACT
// ============================================================================

/// Spawn contract for a progress bar. The builder observer in `widgets_internal` creates the
/// material asset and inserts the carried [`ProgressBar`].
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct BuilderProgressBar {
    pub progress_bar: ProgressBar,
}

impl BuilderProgressBar {
    /// Sets the initial fraction. The bar fades in from 0 to this value on its
    /// first sync — a deliberate fill-in effect when a panel opens or switches
    /// subject, not a bug.
    pub fn with_fraction(mut self, fraction: f32) -> Self {
        self.progress_bar.set_fraction(fraction);
        self
    }

    pub fn with_fill_color(mut self, color: impl Into<Color>) -> Self {
        self.progress_bar.fill_color = color.into().to_linear();
        self
    }

    pub fn with_edge_color(mut self, color: impl Into<Color>) -> Self {
        self.progress_bar.edge_color = color.into().to_linear();
        self
    }

    pub fn with_track_color(mut self, color: impl Into<Color>) -> Self {
        self.progress_bar.track_color = color.into().to_linear();
        self
    }

    pub fn with_rail_color(mut self, color: impl Into<Color>) -> Self {
        self.progress_bar.rail_color = color.into().to_linear();
        self
    }

    pub fn with_stall_color(mut self, color: impl Into<Color>) -> Self {
        self.progress_bar.stall_color = color.into().to_linear();
        self
    }

    pub fn with_detail(mut self, detail: ProgressBarDetail) -> Self {
        self.progress_bar.detail = detail;
        self
    }

    pub fn with_marker_color(mut self, color: impl Into<Color>) -> Self {
        self.progress_bar.marker_color = color.into().to_linear();
        self
    }

    pub fn with_geometry(mut self, geometry: ProgressBarGeometry) -> Self {
        self.progress_bar.geometry = geometry;
        self
    }

    pub fn with_shading(mut self, shading: ProgressBarShading) -> Self {
        self.progress_bar.shading = shading;
        self
    }
}

impl Default for ProgressBar {
    fn default() -> Self {
        Self {
            // #28C7FF ice blue, the interactive accent.
            fill_color: Srgba::rgb_u8(0x28, 0xC7, 0xFF).into(),
            // #EAF4FF primary text, for the two lines that mark a position.
            edge_color: Srgba::rgb_u8(0xEA, 0xF4, 0xFF).into(),
            // #080D1A panel background: the track is a slot cut into the surface.
            track_color: Srgba::rgb_u8(0x08, 0x0D, 0x1A).into(),
            // #233A68 structural border, for the rails and their graduations.
            rail_color: Srgba::rgb_u8(0x23, 0x3A, 0x68).into(),
            // The runway's own hue at full strength, so the marker reads as a bolder edge of
            // what it bounds.
            marker_color: Srgba::rgb_u8(0x28, 0xC7, 0xFF).into(),
            // Worn by the progress edge once it reaches the marker.
            stall_color: Srgba::rgb_u8(0xFF, 0x3B, 0x30).into(),
            geometry: ProgressBarGeometry {
                rake: 0.4,
                rail_thickness: 1.0,
                edge_width: 2.0,
                marker_width: 1.5,
            },
            shading: ProgressBarShading {
                edge_falloff: 20.0,
                fill_gradient: 1.18,
                runway_gain: 0.16,
                inert_gain: 0.45,
            },
            detail: ProgressBarDetail {
                tick_length: 3.0,
                tick_width: 1.0,
                tick_gain: 0.7,
                stall_gap: 2.0,
            },
            progress: FadeState::new(EASE_PROGRESS),
            marker: FadeState::new(EASE_MARKER),
            band: FadeState::new(EASE_BAND),
        }
    }
}
