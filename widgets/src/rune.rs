//! A seeded glyph that flies within its parent and despawns on arrival.
//!
//! Each glyph has a fixed spine and twelve seed-selected branches. Stable seeds give a subject a
//! consistent set of glyphs. Spawn [`BuilderRune`] under the node containing the flight; positions
//! are pixels relative to that parent's top-left.

use bevy::{
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
    ui_render::ui_material::UiMaterial,
};

const SHADER_ASSET_PATH: &str = "shaders/rune.wgsl";

// ============================================================================
// MATERIAL
// ============================================================================

#[derive(ShaderType, Clone, Copy, Debug)]
pub struct RuneParams {
    /// Low twelve bits select which branches the glyph carries.
    pub seed: u32,
    pub stroke_width: f32,
    pub tilt: f32,
    /// Seconds on the same clock the shader reads from `globals.time`.
    pub start_time: f32,
}

/// The glyph's fade, evaluated in the shader from its age. Keeping the whole curve on the
/// GPU is what lets the material be written once.
#[derive(ShaderType, Clone, Copy, Debug)]
pub struct RuneLife {
    pub duration: f32,
    /// Fractions of the flight spent fading in and out. The glyph resolves out of nothing
    /// and is gone by the time it lands, so arrival reads as absorption.
    pub fade_in: f32,
    pub fade_out: f32,
    pub brightness: f32,
}

#[derive(AsBindGroup, Asset, TypePath, Clone, Copy, Debug)]
pub struct RuneMaterial {
    #[uniform(0)]
    pub color: LinearRgba,
    #[uniform(0)]
    pub params: RuneParams,
    #[uniform(0)]
    pub life: RuneLife,
}

impl UiMaterial for RuneMaterial {
    fn fragment_shader() -> ShaderRef {
        SHADER_ASSET_PATH.into()
    }
}

// ============================================================================
// FLIGHT
// ============================================================================

/// Where a rune travels, in pixels relative to its parent's top-left.
#[derive(Clone, Copy, Debug, Default)]
pub struct RuneFlight {
    pub from: Vec2,
    pub to: Vec2,
    pub duration: f32,
    /// Sideways offset of the path's midpoint, in pixels. Bends the flight so a stream of
    /// runes converging on one point does not collapse into a single line.
    pub curve: f32,
}

impl RuneFlight {
    /// Position at `progress`, 0..=1. Accelerating, along a quadratic bend.
    fn position(&self, progress: f32) -> Vec2 {
        let travelled = progress * progress * progress;
        let direction = (self.to - self.from).normalize_or_zero();
        let sideways = Vec2::new(-direction.y, direction.x) * self.curve;
        let control = self.from.lerp(self.to, 0.5) + sideways;

        let inverse = 1.0 - travelled;
        self.from * (inverse * inverse)
            + control * (2.0 * inverse * travelled)
            + self.to * (travelled * travelled)
    }
}

/// A rune in flight. The flight system moves it and despawns it on arrival.
///
/// [`RuneFlight`] describes where the glyph's centre travels; `size` is what converts that
/// into the corner offset a [`Node`] is positioned by.
#[derive(Component, Clone, Copy, Debug)]
pub struct Rune {
    pub flight: RuneFlight,
    pub started_at: f32,
    pub size: f32,
}

impl Rune {
    /// Progress along the flight, 0..=1. At 1 the rune has arrived.
    pub fn progress(&self, now: f32) -> f32 {
        if self.flight.duration <= 0. { return 1.0 }
        ((now - self.started_at) / self.flight.duration).clamp(0., 1.)
    }

    pub fn position(&self, now: f32) -> Vec2 {
        self.flight.position(self.progress(now))
    }
}

// ============================================================================
// SPAWN CONTRACT
// ============================================================================

/// Spawn contract for a rune. Spawn it as a child of the node the flight happens in.
#[derive(Component, Clone, Copy, Debug)]
pub struct BuilderRune {
    pub seed: u32,
    pub color: Color,
    pub size: f32,
    pub stroke_width: f32,
    pub tilt: f32,
    /// Fractions of the flight spent fading in and out.
    pub fade_in: f32,
    pub fade_out: f32,
    /// Ceiling the fade reaches at mid-flight. Below 1 for a quieter stream.
    pub brightness: f32,
    pub flight: RuneFlight,
}

impl BuilderRune {
    pub fn new(seed: u32, flight: RuneFlight) -> Self {
        Self { seed, flight, ..default() }
    }

    pub fn with_color(mut self, color: impl Into<Color>) -> Self {
        self.color = color.into();
        self
    }

    pub fn with_size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    pub fn with_tilt(mut self, tilt: f32) -> Self {
        self.tilt = tilt;
        self
    }

    pub fn params(&self, start_time: f32) -> RuneParams {
        RuneParams {
            // Only the low twelve bits pick branches, and the spine keeps a zero seed from
            // drawing nothing at all.
            seed: self.seed & 0xFFF,
            stroke_width: self.stroke_width,
            tilt: self.tilt,
            start_time,
        }
    }

    pub fn life(&self) -> RuneLife {
        RuneLife {
            duration: self.flight.duration,
            fade_in: self.fade_in,
            fade_out: self.fade_out,
            brightness: self.brightness,
        }
    }
}

impl Default for BuilderRune {
    fn default() -> Self {
        Self {
            seed: 0,
            // #28C7FF ice blue.
            color: Srgba::rgb_u8(0x28, 0xC7, 0xFF).into(),
            size: 14.0,
            // In glyph-box fractions, so a rune keeps its weight at any size.
            stroke_width: 0.09,
            tilt: 0.,
            // The glyph resolves out of nothing and is gone by the time it lands, so arrival
            // reads as absorption.
            fade_in: 0.18,
            fade_out: 0.22,
            brightness: 1.0,
            flight: RuneFlight::default(),
        }
    }
}
