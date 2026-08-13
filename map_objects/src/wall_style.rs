//! A wall style is the parameter set the wall shader draws with.

use bevy::{prelude::*, render::render_resource::ShaderType};

/// Silhouette geometry shared by every cell drawn with a given style.
#[derive(ShaderType, Clone, Copy, Debug)]
pub struct WallStyleGeometry {
    pub bevel_width: f32,
    pub contour_width: f32,
    pub hairline_width: f32,
    pub erosion_amount: f32,
}

/// Surface shading shared by every cell drawn with a given style.
#[derive(ShaderType, Clone, Copy, Debug)]
pub struct WallStyleSurface {
    pub plate_noise_scale: f32,
    pub shadow_length: f32,
    pub light_direction: Vec2,
}

/// GPU-side parameter set for the wall canvas shader. Field order and types mirror the
/// `WallStyle` struct in `assets/shaders/wall_canvas.wgsl` exactly.
///
/// Every member is exactly 16 bytes and must stay that way: a member narrower than 16
/// bytes shifts every member after it and panics when the buffer is prepared.
#[derive(ShaderType, Clone, Copy, Debug)]
pub struct WallStyle {
    pub body_low: LinearRgba,
    pub body_high: LinearRgba,
    pub bevel_color: LinearRgba,
    pub hairline_color: LinearRgba,
    pub contour_color: LinearRgba,
    pub geometry: WallStyleGeometry,
    pub surface: WallStyleSurface,
}

/// A named entry in the style table. `WallStyle` is the GPU-uploaded payload; `name` is
/// the stable identity that survives save/load.
#[derive(Clone, Debug)]
pub struct WallStyleEntry {
    pub name: String,
    pub style: WallStyle,
}

/// Index into [`WallStyles`], identifying which style a wall is drawn with.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[component(immutable)]
pub struct WallStyleKey(pub u32);

/// The table of wall styles available on the map, in a fixed order shared with the GPU
/// styles buffer.
#[derive(Resource)]
pub struct WallStyles {
    pub entries: Vec<WallStyleEntry>,
}

impl WallStyles {
    /// The styles a map starts with. Colours are authored as sRGB; widths are world pixels.
    pub fn presets() -> Self {
        let light_direction = Vec2::new(-0.4472, 0.8944);
        let plate_noise_scale = 26.0;
        let shadow_length = 5.0;

        Self {
            entries: vec![
                WallStyleEntry {
                    name: "rime".to_string(),
                    style: WallStyle {
                        body_low: Color::srgb(0.30, 0.42, 0.55).to_linear(),
                        body_high: Color::srgb(0.52, 0.68, 0.80).to_linear(),
                        bevel_color: Color::srgb(0.11, 0.13, 0.22).to_linear(),
                        hairline_color: Color::srgb(0.60, 0.78, 0.88).to_linear(),
                        contour_color: Color::srgb(0.06, 0.07, 0.12).to_linear(),
                        geometry: WallStyleGeometry {
                            bevel_width: 4.0,
                            contour_width: 1.0,
                            hairline_width: 1.0,
                            erosion_amount: 1.6,
                        },
                        surface: WallStyleSurface { plate_noise_scale, shadow_length, light_direction },
                    },
                },
                WallStyleEntry {
                    name: "basalt".to_string(),
                    style: WallStyle {
                        body_low: Color::srgb(0.16, 0.18, 0.24).to_linear(),
                        body_high: Color::srgb(0.34, 0.38, 0.46).to_linear(),
                        bevel_color: Color::srgb(0.07, 0.08, 0.12).to_linear(),
                        hairline_color: Color::srgb(0.30, 0.36, 0.44).to_linear(),
                        contour_color: Color::srgb(0.06, 0.07, 0.12).to_linear(),
                        geometry: WallStyleGeometry {
                            bevel_width: 7.0,
                            contour_width: 1.5,
                            hairline_width: 1.0,
                            erosion_amount: 4.0,
                        },
                        surface: WallStyleSurface { plate_noise_scale, shadow_length, light_direction },
                    },
                },
                WallStyleEntry {
                    name: "alloy".to_string(),
                    style: WallStyle {
                        body_low: Color::srgb(0.10, 0.14, 0.26).to_linear(),
                        body_high: Color::srgb(0.18, 0.26, 0.44).to_linear(),
                        bevel_color: Color::srgb(0.05, 0.07, 0.16).to_linear(),
                        hairline_color: Color::srgb(0.16, 0.78, 1.00).to_linear(),
                        contour_color: Color::srgb(0.06, 0.07, 0.12).to_linear(),
                        geometry: WallStyleGeometry {
                            bevel_width: 2.5,
                            contour_width: 1.0,
                            hairline_width: 1.0,
                            erosion_amount: 0.0,
                        },
                        surface: WallStyleSurface { plate_noise_scale, shadow_length, light_direction },
                    },
                },
            ],
        }
    }

    pub fn default_index(&self) -> u32 {
        0
    }

}

/// Which term the wall shader draws instead of the finished wall. Discriminants are the
/// `DEBUG_*` constants in `assets/shaders/wall_canvas.wgsl`; the two must stay in step.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u32)]
pub enum WallCanvasDebug {
    #[default]
    Off = 0,
    /// Signed distance, banded every 4 world pixels. A kink in the field shows as a bent band.
    Distance = 1,
    /// Style being drawn with, against the cell's own region.
    Style = 2,
    /// Which way the surface faces: green where lit, red where shaded.
    Facing = 3,
    /// Plate noise alone.
    Noise = 4,
}
impl WallCanvasDebug {
    pub fn shader_index(self) -> u32 {
        self as u32
    }
}
