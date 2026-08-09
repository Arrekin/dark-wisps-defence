//! A close mark for panels.
//!
//! Its hover surround distinguishes it from action buttons without drawing a persistent box.
//!
//! The strokes are drawn in `assets/shaders/close_button.wgsl`. Clicking is the consumer's
//! business — observe `Pointer<Click>` on the entity.

use bevy::{
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
    ui_render::ui_material::UiMaterial,
};

use crate::common::fade::{Fade, FadeState};

const SHADER_ASSET_PATH: &str = "shaders/close_button.wgsl";

/// Matches the panel's hover rate, so chrome and surfaces answer the pointer together.
const EASE_HOVER: f32 = 10.0;

// ============================================================================
// MATERIAL
// ============================================================================

/// The mark, in pixels.
#[derive(ShaderType, Clone, Copy, Debug)]
pub struct CloseButtonGeometry {
    /// Half-length of a stroke, measured along its diagonal. The mark's width on screen is
    /// `mark_size * sqrt(2)`.
    pub mark_size: f32,
    /// The stroke along the chamfer diagonal, and the one crossing it. Their difference is
    /// what ties the mark to a silhouette that cuts two corners and not four.
    pub heavy_width: f32,
    pub light_width: f32,
    /// Half-width of the gap where the strokes cross.
    pub gap: f32,
}

/// What the pointer brings.
#[derive(ShaderType, Clone, Copy, Debug)]
pub struct CloseButtonHover {
    /// Radians the mark turns through at full hover.
    pub rotation: f32,
    pub corner_cut: f32,
    pub surround_edge: f32,
    pub surround_fill: f32,
}

/// GPU-side uniform. Field order and types mirror `CloseButtonMaterial` in
/// `assets/shaders/close_button.wgsl`.
///
/// Every member is 16 bytes. A narrower one shifts everything after it and panics when the
/// buffer is prepared.
#[derive(AsBindGroup, Asset, TypePath, Clone, Copy, Debug)]
pub struct CloseButtonMaterial {
    #[uniform(0)]
    pub mark_color: LinearRgba,
    #[uniform(0)]
    pub hover_color: LinearRgba,
    #[uniform(0)]
    pub surround_color: LinearRgba,
    #[uniform(0)]
    pub geometry: CloseButtonGeometry,
    #[uniform(0)]
    pub hover: CloseButtonHover,
    #[uniform(0)]
    pub hover_fade: Fade,
}

impl UiMaterial for CloseButtonMaterial {
    fn fragment_shader() -> ShaderRef {
        SHADER_ASSET_PATH.into()
    }
}

// ============================================================================
// RUNTIME COMPONENT
// ============================================================================

/// A built close mark. The widget drives hover itself; consumers only handle the click.
#[derive(Component, Clone, Copy, Debug)]
pub struct CloseButton {
    mark_color: LinearRgba,
    hover_color: LinearRgba,
    surround_color: LinearRgba,
    geometry: CloseButtonGeometry,
    hover_look: CloseButtonHover,
    hover: FadeState,
}

impl CloseButton {
    pub fn set_hover(&mut self, hovering: bool) {
        self.hover.set_target(if hovering { 1.0 } else { 0.0 });
    }

    pub fn begin_fades(&mut self, now: f32) {
        self.hover.begin_fade(now);
    }

    pub fn to_material(&self) -> CloseButtonMaterial {
        CloseButtonMaterial {
            mark_color: self.mark_color,
            hover_color: self.hover_color,
            surround_color: self.surround_color,
            geometry: self.geometry,
            hover: self.hover_look,
            hover_fade: self.hover.fade(),
        }
    }
}

// ============================================================================
// SPAWN CONTRACT
// ============================================================================

/// Spawn contract for a close mark. Give it a node of the size you want clickable; the mark
/// is drawn centred at `mark_size` and stays comfortable to hit in a larger box.
#[derive(Component, Clone, Copy, Debug, Default)]
#[require(Button, Pickable)]
pub struct BuilderCloseButton {
    pub close_button: CloseButton,
}

impl BuilderCloseButton {
    pub fn with_mark_color(mut self, color: impl Into<Color>) -> Self {
        self.close_button.mark_color = color.into().to_linear();
        self
    }

    pub fn with_hover_color(mut self, color: impl Into<Color>) -> Self {
        self.close_button.hover_color = color.into().to_linear();
        self
    }

    pub fn with_geometry(mut self, geometry: CloseButtonGeometry) -> Self {
        self.close_button.geometry = geometry;
        self
    }
}

impl Default for CloseButton {
    fn default() -> Self {
        Self {
            // #8BA8CC secondary text: chrome sits below the panel's title, not beside it.
            mark_color: Srgba::rgb_u8(0x8B, 0xA8, 0xCC).into(),
            // #28C7FF ice blue, the interactive accent.
            hover_color: Srgba::rgb_u8(0x28, 0xC7, 0xFF).into(),
            // #233A68 structural border.
            surround_color: Srgba::rgb_u8(0x23, 0x3A, 0x68).into(),
            // `mark_size` is measured along the diagonal, so the mark's width on
            // screen is `mark_size * sqrt(2)`: 14 here draws an X about 20px across,
            // which sits level with the cap height of a panel heading beside it.
            geometry: CloseButtonGeometry {
                mark_size: 14.0,
                heavy_width: 3.0,
                light_width: 2.0,
                gap: 1.5,
            },
            hover_look: CloseButtonHover {
                // About eight degrees.
                rotation: 0.14,
                corner_cut: 5.0,
                surround_edge: 1.0,
                surround_fill: 0.25,
            },
            hover: FadeState::new(EASE_HOVER),
        }
    }
}
