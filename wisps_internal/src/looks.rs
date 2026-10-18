//! Paint-lane components for the four wisp types, containing appearance parameters, active effect
//! visuals and opacity for placement ghosts. Each type has a dedicated layer to keep its
//! instances adjacent for batching.

use bevy::{ecs::component::Mutable, prelude::*, reflect::TypePath, render::render_resource::ShaderType};
use nanorand::Rng;

use game_core::{motion::Locomotion, prelude::ZDepth};
use visuals::{effect_visuals::{EffectVisualTarget, EffectVisuals}, lane::LaneVisual};
use wisps::prelude::WISP_GRID_IMPRINT;

/// Wisp visuals driven by measured speed and heading through `WispMotionLook`.
pub(crate) trait WispMotionVisual: Component<Mutability = Mutable> {
    fn motion_look(&self) -> &WispMotionLook;
    fn motion_look_mut(&mut self) -> &mut WispMotionLook;
}

/// Converts locomotion into shader inputs `(vigor, heading)`.
/// Vigor is speed divided by the reference speed and can exceed 1.0.
/// Heading flips world Y to match the downward V axis of the rectangle's UV coordinates.
pub(crate) fn wisp_motion(locomotion: &Locomotion) -> (f32, Vec2) {
    /// Measured speed (world units/sec) that maps to vigor 1.0.
    const VIGOR_SWEET_SPOT: f32 = 60.0;
    let heading = locomotion.heading();
    (locomotion.speed() / VIGOR_SWEET_SPOT, Vec2::new(heading.x, -heading.y))
}

/// Look parameters of the motion-reactive wisps. Field order and types mirror `WispMotionLook` in
/// `assets/shaders/wisps/motion_look.wesl`.
#[derive(ShaderType, Clone, Copy, Debug, Default)]
pub(crate) struct WispMotionLook {
    pub seed: f32,
    pub vigor: f32,
    pub heading: Vec2,
}
impl WispMotionLook {
    fn new() -> Self {
        Self { seed: nanorand::tls_rng().generate::<f32>() * 100., ..default() } // decorrelates a cluster of wisps
    }
}

/// A fire wisp. Field order and types mirror `Instance` in `assets/shaders/wisps/fire_quad.wesl`.
#[derive(Component, ShaderType, TypePath, Clone)]
#[require(ZDepth::WispFire)]
pub(crate) struct WispFireVisual {
    /// `vigor` and `heading` stretch the flame licks and comb them backward into a trailing
    /// fireball (vigor = strength, heading = aim).
    pub look: WispMotionLook,
    pub effects: EffectVisuals,
    pub alpha: f32,
}
impl WispFireVisual {
    /// Transparent padding around the orb so its flame licks have room to reach beyond the cell;
    /// the orb keeps its on-screen size while only the margin grows. Mirrored by `QUAD_SCALE` in
    /// `assets/shaders/wisps/fire_look.wesl`.
    pub const QUAD_SCALE: f32 = 2.5;

    /// A visual with fresh per-wisp look parameters, drawn at `alpha` (below 1.0 for a ghost).
    pub fn new(alpha: f32) -> Self {
        Self { look: WispMotionLook::new(), effects: default(), alpha }
    }
}
impl LaneVisual for WispFireVisual {
    const LABEL: &'static str = "wisp_fire";
    const LOOK: &'static str = "shaders::wisps::fire_quad";

    fn quad_size() -> Vec2 {
        WISP_GRID_IMPRINT.world_size() * Self::QUAD_SCALE
    }
}
impl WispMotionVisual for WispFireVisual {
    fn motion_look(&self) -> &WispMotionLook { &self.look }
    fn motion_look_mut(&mut self) -> &mut WispMotionLook { &mut self.look }
}
impl EffectVisualTarget for WispFireVisual {
    fn effects_mut(&mut self) -> &mut EffectVisuals { &mut self.effects }
}

/// Look parameters of the water wisp. Field order and types mirror `WispWaterLook` in
/// `assets/shaders/wisps/water_look.wesl`.
#[derive(ShaderType, Clone, Copy, Debug, Default)]
pub(crate) struct WispWaterLook {
    pub seed: f32,
    pub wobble: f32,
    pub flow_speed: f32,
    pub tint: f32,
    /// Direction of travel, in the quad's sample space.
    pub heading: Vec2,
    /// Measured speed over the sweet spot (see `wisp_motion`). The shader derives the deform amount
    /// and cadence from it.
    pub vigor: f32,
    /// Oscillator phase anchors. The shader extrapolates each phase as
    /// `anchor_phase + (globals.time - anchor_time) * rate(vigor)`, deriving the rate from `vigor`
    /// itself, so the wobble keeps moving every frame off the GPU clock with no per-frame upload.
    /// The CPU only re-anchors (folding elapsed time into `anchor_phase`) when vigor or heading
    /// changes.
    pub stroke_anchor_phase: f32,
    pub surf_anchor_phase: f32,
    pub anchor_time: f32,
}

/// A water wisp. Field order and types mirror `Instance` in `assets/shaders/wisps/water_quad.wesl`.
#[derive(Component, ShaderType, TypePath, Clone)]
#[require(ZDepth::WispWater)]
pub(crate) struct WispWaterVisual {
    pub look: WispWaterLook,
    pub effects: EffectVisuals,
    pub alpha: f32,
}
impl WispWaterVisual {
    /// Transparent padding around the droplet so its energetic wobble and lunge have room inside
    /// the mesh; the droplet keeps its on-screen size while only the margin grows. Mirrored by
    /// `QUAD_SCALE` in `assets/shaders/wisps/water_look.wesl` (which scales UV by it).
    pub const QUAD_SCALE: f32 = 2.4;

    /// A visual with fresh per-wisp look parameters, drawn at `alpha` (below 1.0 for a ghost).
    pub fn new(alpha: f32) -> Self {
        let mut rng = nanorand::tls_rng();
        let look = WispWaterLook {
            seed: rng.generate::<f32>() * 100., // decorrelates a cluster of wisps
            wobble: rng.generate::<f32>() * 0.04 + 0.04, // 0.04 - 0.08
            flow_speed: rng.generate::<f32>() * 0.6 + 0.7, // 0.7 - 1.3
            tint: rng.generate::<f32>() * 2. - 1., // -1.0 - 1.0
            ..default() // a still wisp still ripples: the shader derives rest cadence from vigor 0
        };
        Self { look, effects: default(), alpha }
    }
}
impl LaneVisual for WispWaterVisual {
    const LABEL: &'static str = "wisp_water";
    const LOOK: &'static str = "shaders::wisps::water_quad";

    fn quad_size() -> Vec2 {
        WISP_GRID_IMPRINT.world_size() * Self::QUAD_SCALE
    }
}
impl EffectVisualTarget for WispWaterVisual {
    fn effects_mut(&mut self) -> &mut EffectVisuals { &mut self.effects }
}

/// A light wisp. Field order and types mirror `Instance` in `assets/shaders/wisps/light_quad.wesl`.
#[derive(Component, ShaderType, TypePath, Clone)]
#[require(ZDepth::WispLight)]
pub(crate) struct WispLightVisual {
    /// `vigor` brightens and flares the star and sheds motes, `heading` aims the mote wake.
    pub look: WispMotionLook,
    pub effects: EffectVisuals,
    pub alpha: f32,
}
impl WispLightVisual {
    /// A visual with fresh per-wisp look parameters, drawn at `alpha` (below 1.0 for a ghost).
    pub fn new(alpha: f32) -> Self {
        Self { look: WispMotionLook::new(), effects: default(), alpha }
    }
}
impl LaneVisual for WispLightVisual {
    const LABEL: &'static str = "wisp_light";
    const LOOK: &'static str = "shaders::wisps::light_quad";

    fn quad_size() -> Vec2 {
        WISP_GRID_IMPRINT.world_size()
    }
}
impl WispMotionVisual for WispLightVisual {
    fn motion_look(&self) -> &WispMotionLook { &self.look }
    fn motion_look_mut(&mut self) -> &mut WispMotionLook { &mut self.look }
}
impl EffectVisualTarget for WispLightVisual {
    fn effects_mut(&mut self) -> &mut EffectVisuals { &mut self.effects }
}

/// An electric wisp. Field order and types mirror `Instance` in
/// `assets/shaders/wisps/electric_quad.wesl`.
#[derive(Component, ShaderType, TypePath, Clone)]
#[require(ZDepth::WispElectric)]
pub(crate) struct WispElectricVisual {
    /// `vigor` scales the crackle, `heading` aims the spark wake.
    pub look: WispMotionLook,
    pub effects: EffectVisuals,
    pub alpha: f32,
}
impl WispElectricVisual {
    /// A visual with fresh per-wisp look parameters, drawn at `alpha` (below 1.0 for a ghost).
    pub fn new(alpha: f32) -> Self {
        Self { look: WispMotionLook::new(), effects: default(), alpha }
    }
}
impl LaneVisual for WispElectricVisual {
    const LABEL: &'static str = "wisp_electric";
    const LOOK: &'static str = "shaders::wisps::electric_quad";

    fn quad_size() -> Vec2 {
        WISP_GRID_IMPRINT.world_size()
    }
}
impl WispMotionVisual for WispElectricVisual {
    fn motion_look(&self) -> &WispMotionLook { &self.look }
    fn motion_look_mut(&mut self) -> &mut WispMotionLook { &mut self.look }
}
impl EffectVisualTarget for WispElectricVisual {
    fn effects_mut(&mut self) -> &mut EffectVisuals { &mut self.effects }
}
