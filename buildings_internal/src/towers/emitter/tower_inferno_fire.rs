//! # Inferno Fire
//!
//! Charging visuals driven by `TowerShootingTimer`, spawned as children of `TowerInferno`.
//! Each child uses a lane visual:
//!
//! - `InfernoCore` — core and seam glow masked by the tower sprite in its shared material.
//! - `InfernoEmberRing` — ground ember ring along the outer edge of `AttackRange`.
//! - `InfernoHaze` — distortion of the ground and surface bands, applied before the air band.
//!   Shimmer rings start one tile beyond the imprint and reach the core at full charge.

use bevy::{
    prelude::*,
    reflect::TypePath,
    render::render_resource::{AsBindGroup, BlendState, ShaderType},
    sprite_render::MaterialExtension2d,
};

use almanach::AlmanachRegistrations;
use alteration::modifiers::prelude::*;
use buildings::prelude::*;
use game_core::prelude::*;
use visuals::{
    lane::{LaneMaterial, LaneMaterialHandle, LaneParts, LanePlugin, LaneVisual},
    shader::ADDITIVE_BLENDING,
};

pub(crate) struct TowerInfernoFirePlugin;
impl Plugin for TowerInfernoFirePlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                LanePlugin::<InfernoCore>::default(),
                LanePlugin::<InfernoEmberRing>::default(),
                LanePlugin::<InfernoHaze>::default(),
            ))
            .add_systems(PostStartup, set_inferno_core_sprite)
            .add_systems(Update, (update_cores, update_ember_rings, update_hazes));
    }
}

// Ember ring
/// Extra mesh half-size beyond `range` to fit the ring falloff.
const EMBER_RING_MESH_MARGIN: f32 = 24.;

// Haze
/// Haze extent beyond the tower imprint (1 tile).
const HAZE_MARGIN: f32 = CELL_SIZE;

/// Core and seam glow overlay. Child of `TowerInferno`. Field order and types mirror `Instance` in
/// `assets/shaders/towers/inferno/core.wesl`.
#[derive(Component, ShaderType, TypePath, Clone, Default, PartialEq)]
#[require(ZDepth::BuildingEffect)]
pub(crate) struct InfernoCore {
    /// Normalized reload progress.
    charge: f32,
}
impl LaneVisual for InfernoCore {
    const LABEL: &'static str = "inferno_core";
    const LOOK: &'static str = "shaders::towers::inferno::core";
    type Material = InfernoCoreMaterial;

    /// Premultiplied alpha: the core occludes the sprite (`alpha > 0`) while seams add light
    /// (`alpha = 0`).
    fn blend() -> BlendState {
        BlendState::PREMULTIPLIED_ALPHA_BLENDING
    }
}

/// Shared tower sprite used to mask the core and seam glow.
#[derive(Asset, AsBindGroup, TypePath, Clone, Default)]
pub(crate) struct InfernoCoreMaterial {
    #[texture(1)]
    #[sampler(2)]
    sprite: Option<Handle<Image>>,
}
impl MaterialExtension2d for InfernoCoreMaterial {}

/// Ground ember ring at the tower's range boundary. Child of `TowerInferno`. Field order and types
/// mirror `Instance` in `assets/shaders/towers/inferno/ember_ring.wesl`.
#[derive(Component, ShaderType, TypePath, Clone, Default, PartialEq)]
#[require(ZDepth::GroundDecal)]
pub(crate) struct InfernoEmberRing {
    /// Range radius in world units.
    range: f32,
    /// Normalized reload progress.
    charge: f32,
    /// Quad half-size in world units.
    half_size: f32,
}
impl LaneVisual for InfernoEmberRing {
    const LABEL: &'static str = "inferno_ember_ring";
    const LOOK: &'static str = "shaders::towers::inferno::ember_ring";

    fn blend() -> BlendState {
        ADDITIVE_BLENDING
    }
}

/// Heat haze around the charging tower. Child of `TowerInferno`. Field order and types mirror
/// `Instance` in `assets/shaders/towers/inferno/haze_displacement.wesl`.
#[derive(Component, ShaderType, TypePath, Clone, Default, PartialEq)]
#[require(ZDepth::AirEffect)]
pub(crate) struct InfernoHaze {
    /// Outer radius of the haze in world units.
    reach: f32,
    /// Normalized reload progress.
    charge: f32,
}
impl LaneVisual for InfernoHaze {
    const LABEL: &'static str = "inferno_haze";
    const LOOK: &'static str = "shaders::towers::inferno::haze_displacement";
    const PARTS: LaneParts = LaneParts::DISPLACE;

    fn cut_quad_size(&self) -> Vec2 {
        Vec2::splat(2. * self.reach)
    }
}

/// Sets the tower sprite used by the shared core material.
fn set_inferno_core_sprite(
    registrations: Res<AlmanachRegistrations>,
    core_material: Res<LaneMaterialHandle<InfernoCore>>,
    mut materials: ResMut<Assets<LaneMaterial<InfernoCore>>>,
) {
    let building_info = &registrations.buildings[&BuildingType::Tower(TowerType::Inferno)];
    materials.get_mut(&core_material.0).unwrap().extension.sprite = Some(building_info.sprite.clone());
}

fn update_cores(
    mut cores: Query<(&ChildOf, &mut InfernoCore)>,
    towers: Query<&TowerShootingTimer, With<TowerInferno>>,
) {
    for (child_of, mut core) in cores.iter_mut() {
        let Ok(timer) = towers.get(child_of.parent()) else { continue; };
        core.set_if_neq(InfernoCore { charge: timer.0.fraction() });
    }
}

fn update_ember_rings(
    mut ember_rings: Query<(&ChildOf, &mut Transform, &mut InfernoEmberRing)>,
    towers: Query<(&AttackRange, &TowerShootingTimer), With<TowerInferno>>,
) {
    for (child_of, mut transform, mut ember_ring) in ember_rings.iter_mut() {
        let Ok((attack_range, timer)) = towers.get(child_of.parent()) else { continue; };
        let range = attack_range.get() * CELL_SIZE;
        let half_size = range + EMBER_RING_MESH_MARGIN;
        transform.set_if_neq(Transform { scale: Vec3::new(2. * half_size, 2. * half_size, 1.), ..*transform });
        ember_ring.set_if_neq(InfernoEmberRing { range, charge: timer.0.fraction(), half_size });
    }
}

fn update_hazes(
    mut hazes: Query<(&ChildOf, &mut InfernoHaze)>,
    towers: Query<(&GridArea, &TowerShootingTimer), With<TowerInferno>>,
) {
    for (child_of, mut haze) in hazes.iter_mut() {
        let Ok((area, timer)) = towers.get(child_of.parent()) else { continue; };
        let reach = area.imprint.world_size().max_element() * 0.5 + HAZE_MARGIN;
        haze.set_if_neq(InfernoHaze { reach, charge: timer.0.fraction() });
    }
}
