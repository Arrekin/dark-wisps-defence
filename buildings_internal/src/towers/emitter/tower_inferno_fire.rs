//! # Inferno Fire
//!
//! Charging visuals for `TowerInferno`, driven by `TowerShootingTimer`:
//! - `InfernoCore` — core and seam glow overlay on the tower sprite.
//! - `InfernoEmberRing` — ground ember ring along the outer edge of `AttackRange`.
//!
//! Spawned as children of `TowerInferno` and initialized via `On<Add<...>>` observers.

use bevy::{
    mesh::MeshVertexBufferLayoutRef,
    prelude::*,
    reflect::TypePath,
    render::render_resource::{AsBindGroup, BlendState, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError},
    shader::ShaderRef,
    sprite_render::{AlphaMode2d, Material2d, Material2dKey, Material2dPipeline, Material2dPlugin, MeshMaterial2d},
};

use almanach::prelude::*;
use alteration::modifiers::prelude::*;
use buildings::prelude::*;
use game_core::prelude::*;
use visuals::{shader::set_blend, unit_quad::UnitQuad};

pub(crate) struct TowerInfernoFirePlugin;
impl Plugin for TowerInfernoFirePlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                Material2dPlugin::<InfernoCoreMaterial>::default(),
                Material2dPlugin::<InfernoEmberRingMaterial>::default(),
            ))
            .add_systems(Update, (update_cores, update_ember_rings))
            .add_observer(on_add_inferno_core_set_up)
            .add_observer(on_add_inferno_ember_ring_set_up);
    }
}

// Ember ring
/// Extra mesh half-size beyond `range` to fit the ring falloff.
const EMBER_RING_MESH_MARGIN: f32 = 24.;

/// Core and seam glow overlay. Child of `TowerInferno`.
#[derive(Component)]
pub(crate) struct InfernoCore;

/// Field order and types mirror `InfernoCore` in `assets/shaders/towers/inferno/core.wesl`.
#[derive(ShaderType, Clone, Copy, Debug, Default, PartialEq)]
struct InfernoCoreUniform {
    charge: f32,
}

/// Samples the tower sprite to mask core and seam glow. Premultiplied alpha: the core occludes the
/// sprite (`alpha > 0`) while seams add light (`alpha = 0`).
#[derive(Asset, AsBindGroup, TypePath, Clone, Debug)]
struct InfernoCoreMaterial {
    #[uniform(0)]
    core: InfernoCoreUniform,
    #[texture(1)]
    #[sampler(2)]
    sprite: Handle<Image>,
}
impl Material2d for InfernoCoreMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/towers/inferno/core.wesl".into()
    }
    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
    fn specialize(
        _pipeline: &Material2dPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: Material2dKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        set_blend(descriptor, BlendState::PREMULTIPLIED_ALPHA_BLENDING);
        Ok(())
    }
}

/// Ground ember ring at the tower's range boundary. Child of `TowerInferno`.
#[derive(Component)]
pub(crate) struct InfernoEmberRing;

/// Field order and types mirror `InfernoEmberRing` in `assets/shaders/towers/inferno/ember_ring.wesl`.
#[derive(ShaderType, Clone, Copy, Debug, Default, PartialEq)]
struct InfernoEmberRingUniform {
    range: f32,
    charge: f32,
    half_size: f32,
}

/// Premultiplied alpha with zero alpha output (purely additive).
#[derive(Asset, AsBindGroup, TypePath, Clone, Copy, Debug, Default)]
struct InfernoEmberRingMaterial {
    #[uniform(0)]
    ember_ring: InfernoEmberRingUniform,
}
impl Material2d for InfernoEmberRingMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/towers/inferno/ember_ring.wesl".into()
    }
    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
    fn specialize(
        _pipeline: &Material2dPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: Material2dKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        set_blend(descriptor, BlendState::PREMULTIPLIED_ALPHA_BLENDING);
        Ok(())
    }
}

fn on_add_inferno_core_set_up(
    trigger: On<Add<InfernoCore>>,
    mut commands: Commands,
    almanach: Res<Almanach>,
    unit_quad: Res<UnitQuad>,
    mut core_materials: ResMut<Assets<InfernoCoreMaterial>>,
) {
    let building_info = almanach.get_building_info(BuildingType::Tower(TowerType::Inferno));
    let material = InfernoCoreMaterial {
        core: InfernoCoreUniform::default(),
        sprite: building_info.sprite.clone(),
    };
    commands.entity(trigger.entity).insert((
        unit_quad.mesh(),
        MeshMaterial2d(core_materials.add(material)),
        Transform::from_scale(building_info.grid_imprint.world_size().extend(1.)),
        ZDepth::BUILDING_EFFECT,
    ));
}

fn on_add_inferno_ember_ring_set_up(
    trigger: On<Add<InfernoEmberRing>>,
    mut commands: Commands,
    unit_quad: Res<UnitQuad>,
    mut ember_ring_materials: ResMut<Assets<InfernoEmberRingMaterial>>,
) {
    commands.entity(trigger.entity).insert((
        unit_quad.mesh(),
        MeshMaterial2d(ember_ring_materials.add(InfernoEmberRingMaterial::default())),
        ZDepth::GROUND_DECAL,
    ));
}

fn update_cores(
    mut core_materials: ResMut<Assets<InfernoCoreMaterial>>,
    cores: Query<(&ChildOf, &MeshMaterial2d<InfernoCoreMaterial>), With<InfernoCore>>,
    towers: Query<&TowerShootingTimer, With<TowerInferno>>,
) {
    for (child_of, material) in cores.iter() {
        let Ok(timer) = towers.get(child_of.parent()) else { continue; };
        let Some(mut current) = core_materials.get_mut(&material.0) else { continue; };
        let uniform = InfernoCoreUniform {
            charge: timer.0.fraction(),
        };
        if current.core != uniform {
            current.core = uniform;
        }
    }
}

fn update_ember_rings(
    mut ember_ring_materials: ResMut<Assets<InfernoEmberRingMaterial>>,
    mut ember_rings: Query<(&ChildOf, &mut Transform, &MeshMaterial2d<InfernoEmberRingMaterial>), With<InfernoEmberRing>>,
    towers: Query<(&AttackRange, &TowerShootingTimer), With<TowerInferno>>,
) {
    for (child_of, mut transform, material) in ember_rings.iter_mut() {
        let Ok((attack_range, timer)) = towers.get(child_of.parent()) else { continue; };
        let range = attack_range.get() * CELL_SIZE;
        let half_size = range + EMBER_RING_MESH_MARGIN;
        transform.set_if_neq(Transform { scale: Vec3::new(2. * half_size, 2. * half_size, 1.), ..*transform });
        let Some(mut current) = ember_ring_materials.get_mut(&material.0) else { continue; };
        let uniform = InfernoEmberRingUniform {
            range,
            charge: timer.0.fraction(),
            half_size,
        };
        if current.ember_ring != uniform {
            current.ember_ring = uniform;
        }
    }
}
