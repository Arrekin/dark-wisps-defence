//! A wall style is the parameter set the wall shader draws with.

use bevy::{prelude::*, render::render_resource::ShaderType};
use strum::{AsRefStr, EnumIter};

use grids::placement::PlacementStyle;

/// World-pixel silhouette geometry shared by every cell drawn with a style.
#[derive(ShaderType, Clone, Copy, Debug)]
pub struct WallStyleGeometry {
    pub bevel_width: f32,
    pub contour_width: f32,
    pub hairline_width: f32,
    pub erosion_amount: f32,
}

/// World-pixel surface-noise scale and contact-shadow length for a style.
#[derive(ShaderType, Clone, Copy, Debug)]
pub struct WallStyleSurface {
    pub plate_noise_scale: f32,
    pub shadow_length: f32,
}

/// GPU-side parameter set for wall shaders. Field order and types mirror the
/// `WallStyle` struct in `assets/shaders/wall_style.wgsl` exactly; [`ShaderType`] supplies
/// the matching storage-buffer layout.
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
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[component(immutable)]
pub struct WallStyleKey(pub u32);

/// Converts the placer's opaque style index to a wall style key.
impl From<PlacementStyle> for WallStyleKey {
    fn from(style: PlacementStyle) -> Self {
        Self(style.0)
    }
}

/// The table of wall styles available on the map, in a fixed order shared with the GPU
/// styles buffer.
#[derive(Resource)]
pub struct WallStyles {
    pub entries: Vec<WallStyleEntry>,
}

impl WallStyles {
    /// The styles a map starts with. Colours are authored as sRGB; widths are world pixels.
    pub fn presets() -> Self {
        let plate_noise_scale = 26.0;
        let shadow_length = 5.0;

        Self {
            entries: vec![
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
                        surface: WallStyleSurface { plate_noise_scale, shadow_length },
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
                        surface: WallStyleSurface { plate_noise_scale, shadow_length },
                    },
                },
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
                        surface: WallStyleSurface { plate_noise_scale, shadow_length },
                    },
                },
            ],
        }
    }

    /// The key of the entry called `name`, or `None` when this map's table has no such entry.
    pub fn key_of(&self, name: &str) -> Option<WallStyleKey> {
        self.entries
            .iter()
            .position(|entry| entry.name == name)
            .map(|index| WallStyleKey(index as u32))
    }

    /// The name the entry at `key` is saved under.
    pub fn name_of(&self, key: WallStyleKey) -> Option<&str> {
        self.entries.get(key.0 as usize).map(|entry| entry.name.as_str())
    }

}

/// Which term the wall shader draws instead of the finished wall. Discriminants are the
/// `DEBUG_*` constants in `assets/shaders/wall_canvas.wgsl`; the two must stay in step.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq, EnumIter, AsRefStr)]
#[repr(u32)]
pub enum WallCanvasDebug {
    #[default]
    Off = 0,
    /// Signed distance, banded every 4 world pixels. A kink in the field shows as a bent band.
    Distance = 1,
    /// Which way the surface faces: green where lit, red where shaded.
    Facing = 2,
    /// Plate noise alone.
    Noise = 3,
}
impl WallCanvasDebug {
    pub fn shader_index(self) -> u32 {
        self as u32
    }
}
