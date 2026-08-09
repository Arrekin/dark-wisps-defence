//! Procedural, shader-drawn UI surfaces for panels, tiles, and buttons.
//!
//! The shader draws its signed-distance-field geometry in pixel space, so edges retain their
//! dimensions across node sizes. It defines the visual layers; this module defines their
//! parameters and state transitions.
//!
//! Panels expose selected, hover, and consumer-defined style states. State changes update fade
//! endpoints once; the shader interpolates from `globals.time` without per-frame CPU updates.

use bevy::{
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
    ui_render::ui_material::UiMaterial,
};

use crate::common::fade::{Fade, FadeState};

// ============================================================================
// MATERIAL — the GPU uniform
// ============================================================================

const SHADER_ASSET_PATH: &str = "shaders/void_panel.wgsl";

/// Silhouette dimensions in pixels and the resting intensity of the two edge layers.
#[derive(ShaderType, Clone, Copy, Debug)]
pub struct VoidPanelGeometry {
    pub border_width: f32,
    pub corner_cut: f32,
    pub edge_brightness: f32,
    pub rim_intensity: f32,
}

/// How a panel answers a fully raised style state. See [`VoidPanelStyle`], which is the
/// consumer-facing form of the same four numbers.
#[derive(ShaderType, Clone, Copy, Debug)]
pub struct VoidPanelStyleResponse {
    pub field_scale: f32,
    pub contour_scale: f32,
    pub tint: f32,
    pub corner_mark: f32,
}

/// Shape and travel of two surges that move continuously around the panel border.
#[derive(ShaderType, Clone, Copy, Debug)]
pub struct VoidPanelBorderSurge {
    /// Laps of the perimeter per second.
    pub rate: f32,
    /// Length of one surge along the border, in pixels.
    pub span: f32,
    /// Pixels added to the contour's width at the centre of a surge. Bounded by
    /// `HAIRLINE_INSET` in the shader: a contour that reaches the hairline fills the dark
    /// channel between them, and a surge then reads as a gap being papered over.
    pub width: f32,
    /// How hard the surge drives the contour's brightness.
    pub intensity: f32,
}

/// GPU-side uniform for the void-panel shader. Field order and types mirror the
/// `VoidPanelMaterial` struct in `assets/shaders/void_panel.wgsl` exactly.
///
/// Every member must start at a multiple of 16 bytes, and nothing here pads automatically.
/// A member narrower than 16 bytes shifts every member after it and panics when the buffer
/// is prepared. All members are currently exactly 16 bytes; keep it that way.
#[derive(AsBindGroup, Asset, TypePath, Clone, Copy, Debug)]
pub struct VoidPanelMaterial {
    /// Field color at the panel center.
    #[uniform(0)]
    pub background_center: LinearRgba,
    /// Field color at the panel border.
    #[uniform(0)]
    pub background_edge: LinearRgba,
    /// Structural border color.
    #[uniform(0)]
    pub border_color: LinearRgba,
    /// Accent color the contour and rim move toward on hover and selection.
    #[uniform(0)]
    pub accent_color: LinearRgba,
    /// Tint the style state carries. Alpha is unused.
    #[uniform(0)]
    pub style_color: LinearRgba,
    #[uniform(0)]
    pub geometry: VoidPanelGeometry,
    #[uniform(0)]
    pub style_response: VoidPanelStyleResponse,
    #[uniform(0)]
    pub selected_fade: Fade,
    #[uniform(0)]
    pub hover_fade: Fade,
    #[uniform(0)]
    pub style_fade: Fade,
    #[uniform(0)]
    pub border_surge_fade: Fade,
    #[uniform(0)]
    pub border_surge: VoidPanelBorderSurge,
}

impl UiMaterial for VoidPanelMaterial {
    fn fragment_shader() -> ShaderRef {
        SHADER_ASSET_PATH.into()
    }
}

// ============================================================================
// RUNTIME COMPONENT — holds targets + in-flight fade bookkeeping
// ============================================================================

/// Easing rates, as per-second exponential constants for the curve
/// `1 - exp(-rate * elapsed)`. A rate of 10.0 is a 100 ms time constant.
///
/// Hover is fastest, selection slightly slower, style gentler still, so that states
/// arriving together do not resolve in lockstep.
const EASE_HOVER: f32 = 10.0;
const EASE_SELECTED: f32 = 6.0;
const EASE_STYLE: f32 = 4.0;
/// Slowest of the four: surges arriving on a border should feel like something spinning up.
const EASE_BORDER_SURGE: f32 = 3.0;

/// How a panel looks while its style state is raised. A consumer declares one of these
/// per condition it wants to show and passes it to [`VoidPanel::set_style`].
///
/// `field_scale` and `contour_scale` multiply the background field and the contour
/// intensity at full style: below 1 the surface recedes, above 1 it asserts itself.
/// `tint` is how far the contour moves toward `color`.
///
/// `corner_mark` is the width in pixels of a wedge of `color` filling the bottom-right
/// corner, or 0 for none. It is the only part of the style that adds something rather than
/// adjusting what is already drawn, which is why it is the part that reads across a grid:
/// scaling the field and contour can only ever move values that sit near black already.
#[derive(Clone, Copy, Debug)]
pub struct VoidPanelStyle {
    pub color: Color,
    pub field_scale: f32,
    pub contour_scale: f32,
    pub tint: f32,
    pub corner_mark: f32,
}

/// A void-panel surface: its appearance, plus the three states that can be raised on it.
/// Spawned through [`BuilderVoidPanel`]; the sync system in `widgets_internal` writes it
/// into the [`VoidPanelMaterial`] asset whenever it changes.
#[derive(Component, Clone, Copy, Debug)]
pub struct VoidPanel {
    // Appearance, set at spawn and rarely changed afterwards.
    background_center: LinearRgba,
    background_edge: LinearRgba,
    border_color: LinearRgba,
    accent_color: LinearRgba,
    style_color: LinearRgba,
    style_response: VoidPanelStyleResponse,
    geometry: VoidPanelGeometry,
    selected: FadeState,
    hover: FadeState,
    style_amount: FadeState,
    border_surge_amount: FadeState,
    border_surge: VoidPanelBorderSurge,
}

impl VoidPanel {
    pub fn set_selected(&mut self, selected: bool) {
        self.selected.set_target(if selected { 1.0 } else { 0.0 });
    }

    pub fn set_hover(&mut self, hovering: bool) {
        self.hover.set_target(if hovering { 1.0 } else { 0.0 });
    }

    /// Adopts `style` and raises the style state to full.
    ///
    /// The appearance is supplied per call rather than fixed at spawn, so a panel with
    /// several mutually exclusive conditions passes a different [`VoidPanelStyle`] as it
    /// moves between them. Only one can be shown at a time; the most recent call wins.
    ///
    /// The tint changes at once while the amount continues to ease. That is unnoticeable
    /// from a lowered state and a visible hue cut from a raised one.
    pub fn set_style(&mut self, style: VoidPanelStyle) {
        self.style_color = style.color.to_linear();
        self.style_response = VoidPanelStyleResponse {
            field_scale: style.field_scale,
            contour_scale: style.contour_scale,
            tint: style.tint,
            corner_mark: style.corner_mark,
        };
        self.style_amount.set_target(1.0);
    }

    /// Lowers the style state. The tint is left in place so the fade out holds its hue.
    pub fn clear_style(&mut self) {
        self.style_amount.set_target(0.0);
    }

    /// Raises or lowers the two surges that travel the border. Anything above zero means
    /// something is happening here; it says nothing about how well it is going.
    pub fn set_border_surge(&mut self, surging: bool) {
        self.border_surge_amount.set_target(if surging { 1.0 } else { 0.0 });
    }

    /// Starts a fade on any state whose target has moved. Called by the sync system.
    pub fn begin_fades(&mut self, now: f32) {
        self.selected.begin_fade(now);
        self.hover.begin_fade(now);
        self.style_amount.begin_fade(now);
        self.border_surge_amount.begin_fade(now);
    }

    /// Builds the material uniform from the appearance and the in-flight fades.
    pub fn to_material(&self) -> VoidPanelMaterial {
        VoidPanelMaterial {
            background_center: self.background_center,
            background_edge: self.background_edge,
            border_color: self.border_color,
            accent_color: self.accent_color,
            style_color: self.style_color,
            geometry: self.geometry,
            style_response: self.style_response,
            selected_fade: self.selected.fade(),
            hover_fade: self.hover.fade(),
            style_fade: self.style_amount.fade(),
            border_surge_fade: self.border_surge_amount.fade(),
            border_surge: self.border_surge,
        }
    }
}

// ============================================================================
// SPAWN CONTRACT
// ============================================================================

/// Spawn contract for a void-panel surface. The builder observer in `widgets_internal`
/// creates the material asset, inserts [`MaterialNode<VoidPanelMaterial>`] and the
/// carried [`VoidPanel`], then removes this builder.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct BuilderVoidPanel {
    pub void_panel: VoidPanel,
}

impl BuilderVoidPanel {
    pub fn with_background_center(mut self, color: impl Into<Color>) -> Self {
        self.void_panel.background_center = color.into().to_linear();
        self
    }

    pub fn with_background_edge(mut self, color: impl Into<Color>) -> Self {
        self.void_panel.background_edge = color.into().to_linear();
        self
    }

    pub fn with_border_color(mut self, color: impl Into<Color>) -> Self {
        self.void_panel.border_color = color.into().to_linear();
        self
    }

    pub fn with_accent_color(mut self, color: impl Into<Color>) -> Self {
        self.void_panel.accent_color = color.into().to_linear();
        self
    }

    pub fn with_border_width(mut self, width: f32) -> Self {
        self.void_panel.geometry.border_width = width;
        self
    }

    /// Depth of the chamfer, in pixels. Only the top-left and bottom-right corners are
    /// cut. Small values are indistinguishable from a rounded corner, so a cut meant to
    /// be seen needs a committed size.
    pub fn with_corner_cut(mut self, cut: f32) -> Self {
        self.void_panel.geometry.corner_cut = cut;
        self
    }

    /// Resting brightness of the contour, which hover and selection multiply. Low values
    /// keep a screen of panels from reading as a field of outlines.
    pub fn with_edge_brightness(mut self, brightness: f32) -> Self {
        self.void_panel.geometry.edge_brightness = brightness;
        self
    }

    /// Resting intensity of the inner rim, which hover and selection add to. Low values
    /// keep panels from glowing at rest.
    pub fn with_rim_intensity(mut self, intensity: f32) -> Self {
        self.void_panel.geometry.rim_intensity = intensity;
        self
    }

    /// Speed, length and strength of the surges that travel the border once the panel is
    /// raised. A surface much smaller or larger than a card wants its own values —
    /// span is in pixels, so the default reads as a short arc on a card and most of the
    /// border on a tile.
    pub fn with_border_surge(mut self, border_surge: VoidPanelBorderSurge) -> Self {
        self.void_panel.border_surge = border_surge;
        self
    }
}

impl Default for VoidPanel {
    fn default() -> Self {
        // Colors are the art direction's palette, by hex. The contour and rim rest low so
        // a grid of panels separates by value rather than by outline, leaving brightness
        // at the border free to mean hover or selection.
        Self {
            // #0D1630 elevated surface
            background_center: Srgba::rgb_u8(0x0D, 0x16, 0x30).into(),
            // #070C18, a step under the panel background
            background_edge: Srgba::rgb_u8(0x07, 0x0C, 0x18).into(),
            // #233A68 structural border
            border_color: Srgba::rgb_u8(0x23, 0x3A, 0x68).into(),
            // #28C7FF ice blue
            accent_color: Srgba::rgb_u8(0x28, 0xC7, 0xFF).into(),
            // Inert until a consumer calls `set_style`.
            style_color: LinearRgba::WHITE,
            style_response: VoidPanelStyleResponse {
                field_scale: 1.0,
                contour_scale: 1.0,
                tint: 0.0,
                corner_mark: 0.0,
            },
            geometry: VoidPanelGeometry {
                border_width: 1.0,
                corner_cut: 12.0,
                edge_brightness: 0.35,
                rim_intensity: 0.06,
            },
            selected: FadeState::new(EASE_SELECTED),
            hover: FadeState::new(EASE_HOVER),
            style_amount: FadeState::new(EASE_STYLE),
            // At zero no surges are drawn, so every panel that never asks for them pays a
            // few arithmetic operations and nothing else.
            border_surge_amount: FadeState::new(EASE_BORDER_SURGE),
            border_surge: VoidPanelBorderSurge {
                // One lap every five seconds.
                rate: 1.0 / 5.0,
                span: 56.0,
                width: 3.0,
                intensity: 5.0,
            },
        }
    }
}
