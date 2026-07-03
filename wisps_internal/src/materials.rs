use bevy::{
    prelude::*,
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
    sprite_render::{AlphaMode2d, Material2d},
};
use nanorand::Rng;

use game_core::prelude::*;
use visuals::prelude::*;

pub(crate) trait WispMaterial: Material2d {
    fn make(asset_server: &AssetServer) -> Self;
    /// Quad size multiplier over the wisp's grid footprint. Materials whose visual
    /// deforms past its resting radius pad the mesh so it never clips the quad edge.
    fn mesh_scale() -> f32 { 1.0 }
}

/// Wisp materials whose look reacts to measured motion through plain `vigor` and
/// `heading` uniforms, with no oscillator phase to keep anchored. One generic
/// system, `drive_wisp_locomotion`, feeds them all from each wisp's [`Locomotion`].
pub(crate) trait WispLocomotiveMaterial: Material2d {
    /// The locomotion currently reflected in the material, for change-gating.
    fn locomotion(&self) -> &Locomotion;
    /// Reflect freshly-measured locomotion: derive `vigor` and `heading` into the
    /// uniforms and keep the source for the next gate.
    fn set_locomotion(&mut self, locomotion: Locomotion);
}

/// Measured locomotion → motion uniforms `(vigor, heading_x, heading_y)`. `vigor`
/// is speed over a sweet-spot (unbounded; 1.0 at the sweet spot, where the look is
/// liveliest); `heading` is mapped into the quad's sample space, whose V axis points
/// down on a `Rectangle` mesh, so its Y is flipped.
fn wisp_motion(locomotion: &Locomotion) -> (f32, f32, f32) {
    /// Measured speed (world units/sec) that maps to vigor 1.0.
    const VIGOR_SWEET_SPOT: f32 = 60.0;
    let heading = locomotion.heading();
    (locomotion.speed() / VIGOR_SWEET_SPOT, heading.x, -heading.y)
}

#[derive(Asset, TypePath, Debug, Clone, AsBindGroup)]
pub(crate) struct WispFireMaterial {
    #[uniform(4)]
    pub seed: f32,
    // Motion uniforms, refreshed by `drive_wisp_locomotion` only when motion changes:
    // together `vigor` and `heading` stretch the flame licks and comb them backward
    // into a trailing fireball (vigor = strength, heading = aim).
    #[uniform(4)]
    pub vigor: f32,
    #[uniform(4)]
    pub heading_x: f32,
    #[uniform(4)]
    pub heading_y: f32,

    #[uniform(5)]
    pub effects: EffectVisualUniform,

    /// Source motion behind the uniforms above; CPU-only (never uploaded), read to
    /// gate re-uploads in `drive_wisp_locomotion`.
    locomotion: Locomotion,
}
impl WispFireMaterial {
    /// Transparent padding around the orb so its flame licks have room to reach
    /// beyond the cell; the orb keeps its on-screen size while only the margin grows.
    /// Mirrored by `QUAD_SCALE` in `assets/shaders/wisps/fire.wgsl` — keep the two equal.
    pub const QUAD_SCALE: f32 = 2.5;
}
impl Material2d for WispFireMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/wisps/fire.wgsl".into()
    }
    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}
impl WispMaterial for WispFireMaterial {
    fn make(_asset_server: &AssetServer) -> Self {
        let mut rng = nanorand::tls_rng();
        Self {
            seed: rng.generate::<f32>() * 100., // decorrelates a cluster of wisps
            vigor: 0.,
            heading_x: 0.,
            heading_y: 0.,
            effects: EffectVisualUniform::default(),
            locomotion: Locomotion::default(),
        }
    }
    fn mesh_scale() -> f32 { Self::QUAD_SCALE }
}
impl WispLocomotiveMaterial for WispFireMaterial {
    fn locomotion(&self) -> &Locomotion { &self.locomotion }
    fn set_locomotion(&mut self, locomotion: Locomotion) {
        (self.vigor, self.heading_x, self.heading_y) = wisp_motion(&locomotion);
        self.locomotion = locomotion;
    }
}
impl EffectVisualMaterial for WispFireMaterial {
    fn effects_mut(&mut self) -> &mut EffectVisualUniform {
        &mut self.effects
    }
}

#[derive(Asset, TypePath, Debug, Clone, AsBindGroup)]
pub(crate) struct WispWaterMaterial {
    #[uniform(4)]
    pub seed: f32,
    #[uniform(4)]
    pub wobble: f32,
    #[uniform(4)]
    pub flow_speed: f32,
    #[uniform(4)]
    pub tint: f32,
    // Locomotion. Written by `drive_water_material`, and only when it actually
    // changes — a cruising or idle wisp uploads nothing.
    #[uniform(4)]
    pub heading_x: f32,
    #[uniform(4)]
    pub heading_y: f32,
    // Raw drive: measured speed / sweet-spot. The shader turns this into the
    // geometry deform amount (the curve and its ceiling live in the shader).
    #[uniform(4)]
    pub vigor: f32,
    // Oscillator phase anchors. The shader extrapolates each phase as
    // `anchor_phase + (globals.time - anchor_time) * rate(vigor)`, deriving the
    // rate from `vigor` itself — so the wobble keeps moving every frame off the
    // GPU clock with no per-frame upload, and the CPU only re-anchors (folding
    // elapsed time into `anchor_phase`) when vigor or heading changes.
    #[uniform(4)]
    pub stroke_anchor_phase: f32,
    #[uniform(4)]
    pub surf_anchor_phase: f32,
    #[uniform(4)]
    pub anchor_time: f32,

    #[uniform(5)]
    pub effects: EffectVisualUniform,
}
impl WispWaterMaterial {
    /// Transparent padding around the droplet so its energetic wobble and lunge
    /// have room inside the mesh; the droplet keeps its on-screen size while only
    /// the margin grows. Mirrored by `QUAD_SCALE` in `assets/shaders/wisps/water.wgsl`
    /// (which scales UV by it) — keep the two equal, or the droplet mis-scales.
    pub const QUAD_SCALE: f32 = 2.4;
}
impl Material2d for WispWaterMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/wisps/water.wgsl".into()
    }
    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}
impl WispMaterial for WispWaterMaterial {
    fn make(_asset_server: &AssetServer) -> Self {
        let mut rng = nanorand::tls_rng();
        Self {
            seed: rng.generate::<f32>() * 100., // decorrelates a cluster of wisps
            wobble: rng.generate::<f32>() * 0.04 + 0.04, // 0.04 - 0.08
            flow_speed: rng.generate::<f32>() * 0.6 + 0.7, // 0.7 - 1.3
            tint: rng.generate::<f32>() * 2. - 1., // -1.0 - 1.0
            heading_x: 0.,
            heading_y: 0.,
            vigor: 0., // a still wisp still ripples: the shader derives rest cadence from vigor 0
            stroke_anchor_phase: 0.,
            surf_anchor_phase: 0.,
            anchor_time: 0.,
            effects: EffectVisualUniform::default(),
        }
    }
    fn mesh_scale() -> f32 { Self::QUAD_SCALE }
}
impl EffectVisualMaterial for WispWaterMaterial {
    fn effects_mut(&mut self) -> &mut EffectVisualUniform {
        &mut self.effects
    }
}


#[derive(Asset, TypePath, Debug, Clone, AsBindGroup)]
pub(crate) struct WispLightMaterial {
    #[uniform(4)]
    pub seed: f32,
    // Motion uniforms, refreshed by `drive_wisp_locomotion` only when motion changes:
    // `vigor` brightens and flares the star and sheds motes, `heading` aims the mote wake.
    #[uniform(4)]
    pub vigor: f32,
    #[uniform(4)]
    pub heading_x: f32,
    #[uniform(4)]
    pub heading_y: f32,

    #[uniform(5)]
    pub effects: EffectVisualUniform,

    /// Source motion behind the uniforms above; CPU-only (never uploaded), read to
    /// gate re-uploads in `drive_wisp_locomotion`.
    locomotion: Locomotion,
}
impl Material2d for WispLightMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/wisps/light.wgsl".into()
    }
    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}
impl WispMaterial for WispLightMaterial {
    fn make(_asset_server: &AssetServer) -> Self {
        let mut rng = nanorand::tls_rng();
        Self {
            seed: rng.generate::<f32>() * 100., // decorrelates a cluster of wisps
            vigor: 0.,
            heading_x: 0.,
            heading_y: 0.,
            effects: EffectVisualUniform::default(),
            locomotion: Locomotion::default(),
        }
    }
}
impl WispLocomotiveMaterial for WispLightMaterial {
    fn locomotion(&self) -> &Locomotion { &self.locomotion }
    fn set_locomotion(&mut self, locomotion: Locomotion) {
        (self.vigor, self.heading_x, self.heading_y) = wisp_motion(&locomotion);
        self.locomotion = locomotion;
    }
}
impl EffectVisualMaterial for WispLightMaterial {
    fn effects_mut(&mut self) -> &mut EffectVisualUniform {
        &mut self.effects
    }
}

#[derive(Asset, TypePath, Debug, Clone, AsBindGroup)]
pub(crate) struct WispElectricMaterial {
    #[uniform(4)]
    pub seed: f32,
    // Motion uniforms, refreshed by `drive_wisp_locomotion` only when motion changes:
    // `vigor` scales the crackle, `heading` aims the spark wake.
    #[uniform(4)]
    pub vigor: f32,
    #[uniform(4)]
    pub heading_x: f32,
    #[uniform(4)]
    pub heading_y: f32,

    #[uniform(5)]
    pub effects: EffectVisualUniform,

    /// Source motion behind the uniforms above; CPU-only (never uploaded), read to
    /// gate re-uploads in `drive_wisp_locomotion`.
    locomotion: Locomotion,
}
impl Material2d for WispElectricMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/wisps/electric.wgsl".into()
    }
    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}
impl WispMaterial for WispElectricMaterial {
    fn make(_asset_server: &AssetServer) -> Self {
        let mut rng = nanorand::tls_rng();
        Self {
            seed: rng.generate::<f32>() * 100., // decorrelates a cluster of wisps
            vigor: 0.,
            heading_x: 0.,
            heading_y: 0.,
            effects: EffectVisualUniform::default(),
            locomotion: Locomotion::default(),
        }
    }
}
impl WispLocomotiveMaterial for WispElectricMaterial {
    fn locomotion(&self) -> &Locomotion { &self.locomotion }
    fn set_locomotion(&mut self, locomotion: Locomotion) {
        (self.vigor, self.heading_x, self.heading_y) = wisp_motion(&locomotion);
        self.locomotion = locomotion;
    }
}
impl EffectVisualMaterial for WispElectricMaterial {
    fn effects_mut(&mut self) -> &mut EffectVisualUniform {
        &mut self.effects
    }
}
