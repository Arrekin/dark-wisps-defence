//! # Inferno Dome
//!
//! Standalone fire dome entity spawned when a `TowerInferno` discharges. Self-despawns once all
//! visual layers complete:
//! - `InfernoDomeFire` — hemisphere shell, interior billows, rim spill light, and falling embers.
//! - `InfernoDomeGround` — release flash and ground scorch decal.
//! - Screen-space distortion is handled by `inferno_dome_post_process`.
//!
//! Transient visual entity; not persisted.

use bevy::{
    mesh::MeshVertexBufferLayoutRef,
    prelude::*,
    reflect::TypePath,
    render::render_resource::{AsBindGroup, BlendState, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError},
    shader::ShaderRef,
    sprite_render::{AlphaMode2d, Material2d, Material2dKey, Material2dPipeline, Material2dPlugin, MeshMaterial2d},
};

use game_core::prelude::ZDepth;
use states::prelude::GameState;
use visuals::{shader::{seed_from_entity, set_blend}, unit_quad::UnitQuad};
use weaponry::prelude::*;

pub(crate) struct InfernoDomePlugin;
impl Plugin for InfernoDomePlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                Material2dPlugin::<InfernoDomeFireMaterial>::default(),
                Material2dPlugin::<InfernoDomeGroundMaterial>::default(),
            ))
            .add_systems(Update, (
                advance_inferno_domes,
                (update_inferno_dome_fires, update_inferno_dome_grounds).after(advance_inferno_domes),
            ).run_if(in_state(GameState::Running)))
            .add_observer(on_add_inferno_dome_spawn_fire);
    }
}

// Fire
/// Active duration (seconds) of `InfernoDomeFire`. Matches breakup + ember lifetimes in
/// `assets/shaders/weaponry/inferno_dome.wesl`.
const FIRE_DURATION: f32 = 1.6;
/// Extra mesh half-size beyond `radius` for rim noise and spilled light.
const FIRE_MESH_MARGIN: f32 = 50.;

// Ground
/// Active duration (seconds) of `InfernoDomeGround`. Matches `SCORCH_DURATION` in
/// `assets/shaders/weaponry/inferno_dome_ground.wesl`.
const GROUND_DURATION: f32 = 2.5;
/// Extra mesh half-size beyond `radius` for ragged scorch edges.
const GROUND_MESH_MARGIN: f32 = 24.;

// Shimmer
/// Active duration (seconds) of the screen-space distortion pass. Matches `SHIMMER_DURATION` in
/// `assets/shaders/weaponry/inferno_dome_post_process.wesl`.
const SHIMMER_DURATION: f32 = 3.5;

/// Field order and types mirror `InfernoDome` in `assets/shaders/weaponry/inferno_dome.wesl` and
/// `InfernoDomeGround` in `assets/shaders/weaponry/inferno_dome_ground.wesl`.
#[derive(ShaderType, Clone, Copy, Debug, Default, PartialEq)]
struct InfernoDomeUniform {
    age: f32,
    radius: f32,
    seed: f32,
    half_size: f32,
}

/// Dome fire quad. Child of `InfernoDome`.
#[derive(Component)]
struct InfernoDomeFire;

/// Premultiplied alpha: opaque/semi-opaque flame shell with additive spill light and embers.
#[derive(Asset, AsBindGroup, TypePath, Clone, Copy, Debug, Default)]
struct InfernoDomeFireMaterial {
    #[uniform(0)]
    fire: InfernoDomeUniform,
}
impl Material2d for InfernoDomeFireMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/weaponry/inferno_dome.wesl".into()
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

/// Ground flash and scorch quad. Child of `InfernoDome`.
#[derive(Component)]
struct InfernoDomeGround;

/// Premultiplied alpha: darkens ground via scorch alpha while release flash adds light.
#[derive(Asset, AsBindGroup, TypePath, Clone, Copy, Debug, Default)]
struct InfernoDomeGroundMaterial {
    #[uniform(0)]
    ground: InfernoDomeUniform,
}
impl Material2d for InfernoDomeGroundMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/weaponry/inferno_dome_ground.wesl".into()
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

fn on_add_inferno_dome_spawn_fire(
    trigger: On<Add<InfernoDome>>,
    mut commands: Commands,
    unit_quad: Res<UnitQuad>,
    mut fire_materials: ResMut<Assets<InfernoDomeFireMaterial>>,
    mut ground_materials: ResMut<Assets<InfernoDomeGroundMaterial>>,
    domes: Query<&InfernoDome>,
) {
    let entity = trigger.entity;
    let Ok(dome) = domes.get(entity) else { return; };
    let seed = seed_from_entity(entity);
    let fire_half_size = dome.radius + FIRE_MESH_MARGIN;
    let ground_half_size = dome.radius + GROUND_MESH_MARGIN;
    let fire = InfernoDomeFireMaterial {
        fire: InfernoDomeUniform { age: 0., radius: dome.radius, seed, half_size: fire_half_size },
    };
    let ground = InfernoDomeGroundMaterial {
        ground: InfernoDomeUniform { age: 0., radius: dome.radius, seed, half_size: ground_half_size },
    };
    commands.entity(entity).insert(children![
        (
            InfernoDomeFire,
            unit_quad.mesh(),
            MeshMaterial2d(fire_materials.add(fire)),
            Transform::from_scale(Vec3::new(2. * fire_half_size, 2. * fire_half_size, 1.)),
            ZDepth::GROUND_EFFECT,
        ),
        (
            InfernoDomeGround,
            unit_quad.mesh(),
            MeshMaterial2d(ground_materials.add(ground)),
            Transform::from_scale(Vec3::new(2. * ground_half_size, 2. * ground_half_size, 1.)),
            ZDepth::GROUND_DECAL,
        ),
    ]);
}

fn advance_inferno_domes(
    mut commands: Commands,
    time: Res<Time>,
    mut domes: Query<(Entity, &mut InfernoDome)>,
) {
    for (entity, mut dome) in domes.iter_mut() {
        dome.age += time.delta_secs();
        if dome.age >= FIRE_DURATION.max(GROUND_DURATION).max(SHIMMER_DURATION) {
            commands.entity(entity).despawn();
        }
    }
}

fn update_inferno_dome_fires(
    mut fire_materials: ResMut<Assets<InfernoDomeFireMaterial>>,
    mut fires: Query<(&ChildOf, &mut Visibility, &MeshMaterial2d<InfernoDomeFireMaterial>), With<InfernoDomeFire>>,
    domes: Query<&InfernoDome>,
) {
    for (child_of, mut visibility, material) in fires.iter_mut() {
        let Ok(dome) = domes.get(child_of.parent()) else { continue; };
        let showing = dome.age < FIRE_DURATION;
        visibility.set_if_neq(if showing { Visibility::Inherited } else { Visibility::Hidden });
        if !showing { continue; }

        let Some(mut current) = fire_materials.get_mut(&material.0) else { continue; };
        current.fire.age = dome.age;
    }
}

fn update_inferno_dome_grounds(
    mut ground_materials: ResMut<Assets<InfernoDomeGroundMaterial>>,
    mut grounds: Query<(&ChildOf, &mut Visibility, &MeshMaterial2d<InfernoDomeGroundMaterial>), With<InfernoDomeGround>>,
    domes: Query<&InfernoDome>,
) {
    for (child_of, mut visibility, material) in grounds.iter_mut() {
        let Ok(dome) = domes.get(child_of.parent()) else { continue; };
        let showing = dome.age < GROUND_DURATION;
        visibility.set_if_neq(if showing { Visibility::Inherited } else { Visibility::Hidden });
        if !showing { continue; }

        let Some(mut current) = ground_materials.get_mut(&material.0) else { continue; };
        current.ground.age = dome.age;
    }
}
