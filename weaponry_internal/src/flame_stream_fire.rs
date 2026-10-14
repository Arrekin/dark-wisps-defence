//! # Flame Stream Fire
//!
//! Visual entities for `FlameStream`:
//! - `Flame` — stream jet from `nozzle` to the hit point (or `head`), with a `FlameGlow` child for
//!   additive ground light.
//! - `ImpactFire` — splash, clinging flames, cell fire, and sparks on the hit tile.
//! - `FlameScorch` — standalone ground decal spawned when a tile stops burning; self-despawns.
//! - `PilotFlame` — small nozzle flame visible while the stream is armed.
//!
//! `Flame`, `ImpactFire`, and `PilotFlame` are spawned as children of `FlameStream`.

use bevy::{
    mesh::MeshVertexBufferLayoutRef,
    prelude::*,
    reflect::TypePath,
    render::render_resource::{AsBindGroup, BlendState, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError},
    shader::ShaderRef,
    sprite_render::{AlphaMode2d, Material2d, Material2dKey, Material2dPipeline, Material2dPlugin, MeshMaterial2d},
};

use game_core::prelude::{GridCoords, GridImprint, MapBound, ZDepth};
use states::prelude::GameState;
use visuals::{shader::{ADDITIVE_BLENDING, seed_from_entity, set_blend}, unit_quad::UnitQuad};
use weaponry::{flame_stream::FlameStreamSystems, prelude::*};

pub(crate) struct FlameStreamFirePlugin;
impl Plugin for FlameStreamFirePlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                Material2dPlugin::<FlameMaterial>::default(),
                Material2dPlugin::<FlameGlowMaterial>::default(),
                Material2dPlugin::<ImpactFireMaterial>::default(),
                Material2dPlugin::<FlameScorchMaterial>::default(),
                Material2dPlugin::<PilotFlameMaterial>::default(),
            ))
            .add_systems(Update, (
                (update_flames, update_pilot_flames).after(FlameStreamSystems),
                (
                    update_impact_fires.after(FlameStreamSystems),
                    advance_flame_scorches,
                ).run_if(in_state(GameState::Running)),
            ))
            .add_observer(on_add_flame_stream_spawn_fire)
            .add_observer(on_add_flame_set_up)
            .add_observer(on_add_flame_glow_set_up)
            .add_observer(on_add_impact_fire_set_up)
            .add_observer(on_add_pilot_flame_set_up);
    }
}

// Stream mesh
/// Multiplier on max cone half-width to fit edge noise and soot.
const FLAME_MESH_MARGIN: f32 = 1.6;
/// Extra mesh half-width in world units to fit head/pileup bulges.
const FLAME_MESH_PADDING: f32 = 13.;
/// Extra mesh length beyond `stream.length` for tip burnout and soot.
const FLAME_MESH_OVERHANG: f32 = 30.;

// Ground light
const GLOW_MESH_HALF_WIDTH: f32 = 100.;
/// Extra glow mesh length beyond each end of the stream mesh.
const GLOW_MESH_OVERHANG: f32 = 80.;
/// Glow intensity multiplier while venting.
const GLOW_VENTING: f32 = 0.6;

// Impact
/// Intensity ramp-up rate (per second) on an active hit tile.
const IMPACT_IGNITE_RATE: f32 = 12.;
/// Intensity decay rate (per second) after a tile is no longer hit.
const IMPACT_FADE_RATE: f32 = 4.;
const IMPACT_MESH_HALF_SIZE: f32 = 70.;

// Scorch
/// Burn duration (seconds) required for a full-strength scorch mark.
const SCORCH_BUILDUP: f32 = 1.;
const SCORCH_DURATION: f32 = 1.5;
const SCORCH_MESH_HALF_SIZE: f32 = 24.;

// Pilot flame
const PILOT_MESH_SIZE: f32 = 14.;
/// Axial offset of the pilot flame root ahead of `nozzle`.
const PILOT_OFFSET: f32 = 1.5;

/// Stream jet quad. Child of `FlameStream`.
#[derive(Component)]
struct Flame;

/// Field order and types mirror `Flame` in `assets/shaders/weaponry/flame_stream.wesl`.
#[derive(ShaderType, Clone, Copy, Debug, Default, PartialEq)]
struct FlameUniform {
    seed: f32,
    length: f32,
    half_width: f32,
    nozzle_half_width: f32,
    spread: f32,
    head: f32,
    tail: f32,
    end: f32,
    hit: f32,
    venting: f32,
}

/// Premultiplied alpha: additive flame emission with alpha-blended soot.
#[derive(Asset, AsBindGroup, TypePath, Clone, Copy, Debug, Default)]
struct FlameMaterial {
    #[uniform(0)]
    flame: FlameUniform,
}
impl Material2d for FlameMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/weaponry/flame_stream.wesl".into()
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

/// Additive ground light along the stream and around the hit. Child of `Flame`.
#[derive(Component)]
struct FlameGlow;

/// Field order and types mirror `FlameGlow` in `assets/shaders/weaponry/flame_stream_glow.wesl`.
#[derive(ShaderType, Clone, Copy, Debug, Default, PartialEq)]
struct FlameGlowUniform {
    intensity: f32,
    near: f32,
    far: f32,
    half_width: f32,
    tail: f32,
    end: f32,
    hit: f32,
}

#[derive(Asset, AsBindGroup, TypePath, Clone, Copy, Debug, Default)]
struct FlameGlowMaterial {
    #[uniform(0)]
    glow: FlameGlowUniform,
}
impl Material2d for FlameGlowMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/weaponry/flame_stream_glow.wesl".into()
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
        set_blend(descriptor, ADDITIVE_BLENDING);
        Ok(())
    }
}

/// Fire effect on the currently hit tile. Child of `FlameStream`.
#[derive(Component, Default)]
struct ImpactFire {
    /// Normalized intensity in `0.0..=1.0`.
    intensity: f32,
    /// Last burned tile, retained while fading out.
    coords: Option<GridCoords>,
    /// Cumulative burn time (seconds) on `coords`.
    burning_for: f32,
}
impl ImpactFire {
    /// Advances burn state on `coords` by `delta` seconds. If `coords` changed, resets intensity
    /// and returns the previous tile's `BurntTile` data.
    fn burn(&mut self, coords: GridCoords, delta: f32) -> Option<BurntTile> {
        let mut burnt = None;
        if self.coords != Some(coords) {
            burnt = self.stop_burning();
            self.coords = Some(coords);
            self.intensity = 0.;
        }
        self.burning_for += delta;
        self.intensity = (self.intensity + IMPACT_IGNITE_RATE * delta).min(1.);
        burnt
    }

    /// Fades intensity by `delta` seconds and flushes any active `BurntTile`.
    fn fade(&mut self, delta: f32) -> Option<BurntTile> {
        self.intensity = (self.intensity - IMPACT_FADE_RATE * delta).max(0.);
        self.stop_burning()
    }

    fn stop_burning(&mut self) -> Option<BurntTile> {
        if self.burning_for <= 0. { return None; }
        let coords = self.coords?;
        let strength = (self.burning_for / SCORCH_BUILDUP).min(1.);
        self.burning_for = 0.;
        Some(BurntTile { coords, strength })
    }
}

struct BurntTile {
    coords: GridCoords,
    /// Normalized scorch strength in `0.0..=1.0`.
    strength: f32,
}

/// Field order and types mirror `ImpactFire` in `assets/shaders/weaponry/flame_stream_impact.wesl`.
#[derive(ShaderType, Clone, Copy, Debug, Default, PartialEq)]
struct ImpactFireUniform {
    intensity: f32,
    seed: f32,
    half_size: f32,
    hit_point: Vec2,
    direction: Vec2,
}

#[derive(Asset, AsBindGroup, TypePath, Clone, Copy, Debug, Default)]
struct ImpactFireMaterial {
    #[uniform(0)]
    impact: ImpactFireUniform,
}
impl Material2d for ImpactFireMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/weaponry/flame_stream_impact.wesl".into()
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

/// Fading ground char decal spawned when a tile stops burning.
#[derive(Component)]
#[require(MapBound)]
struct FlameScorch {
    age: f32,
}
impl FlameScorch {
    fn new() -> Self {
        Self { age: 0. }
    }

    /// Advances age by `delta` seconds and returns normalized progress (`0.0..1.0`), or `None` when expired.
    fn advance(&mut self, delta: f32) -> Option<f32> {
        self.age += delta;
        (self.age < SCORCH_DURATION).then(|| self.age / SCORCH_DURATION)
    }
}

/// Field order and types mirror `FlameScorch` in `assets/shaders/weaponry/flame_stream_scorch.wesl`.
#[derive(ShaderType, Clone, Copy, Debug, Default, PartialEq)]
struct FlameScorchUniform {
    life: f32,
    strength: f32,
    seed: f32,
    half_size: f32,
}

/// Premultiplied alpha: darkens ground via alpha while cinders add light.
#[derive(Asset, AsBindGroup, TypePath, Clone, Copy, Debug, Default)]
struct FlameScorchMaterial {
    #[uniform(0)]
    scorch: FlameScorchUniform,
}
impl Material2d for FlameScorchMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/weaponry/flame_stream_scorch.wesl".into()
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

/// Nozzle pilot flame shown while the stream is armed. Child of `FlameStream`.
#[derive(Component)]
struct PilotFlame;

/// Field order and types mirror `PilotFlame` in `assets/shaders/weaponry/flame_stream_pilot.wesl`.
#[derive(ShaderType, Clone, Copy, Debug, Default, PartialEq)]
struct PilotFlameUniform {
    seed: f32,
    size: f32,
}

#[derive(Asset, AsBindGroup, TypePath, Clone, Copy, Debug, Default)]
struct PilotFlameMaterial {
    #[uniform(0)]
    pilot: PilotFlameUniform,
}
impl Material2d for PilotFlameMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/weaponry/flame_stream_pilot.wesl".into()
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
        set_blend(descriptor, ADDITIVE_BLENDING);
        Ok(())
    }
}

fn on_add_flame_stream_spawn_fire(
    trigger: On<Add<FlameStream>>,
    mut commands: Commands,
) {
    commands.entity(trigger.entity).insert(children![
        Flame,
        ImpactFire::default(),
        PilotFlame,
    ]);
}

fn on_add_flame_set_up(
    trigger: On<Add<Flame>>,
    mut commands: Commands,
    unit_quad: Res<UnitQuad>,
    mut flame_materials: ResMut<Assets<FlameMaterial>>,
    flames: Query<&ChildOf>,
) {
    let Ok(child_of) = flames.get(trigger.entity) else { return; };
    let material = FlameMaterial { flame: FlameUniform { seed: seed_from_entity(child_of.parent()), ..default() } };
    commands.entity(trigger.entity).insert((
        unit_quad.mesh(),
        MeshMaterial2d(flame_materials.add(material)),
        ZDepth::GROUND_EFFECT,
        Visibility::Hidden,
        children![FlameGlow],
    ));
}

fn on_add_flame_glow_set_up(
    trigger: On<Add<FlameGlow>>,
    mut commands: Commands,
    unit_quad: Res<UnitQuad>,
    mut glow_materials: ResMut<Assets<FlameGlowMaterial>>,
) {
    commands.entity(trigger.entity).insert((
        unit_quad.mesh(),
        MeshMaterial2d(glow_materials.add(FlameGlowMaterial::default())),
        ZDepth::GROUND_DECAL,
    ));
}

fn on_add_impact_fire_set_up(
    trigger: On<Add<ImpactFire>>,
    mut commands: Commands,
    unit_quad: Res<UnitQuad>,
    mut impact_fire_materials: ResMut<Assets<ImpactFireMaterial>>,
    impact_fires: Query<&ChildOf>,
) {
    let Ok(child_of) = impact_fires.get(trigger.entity) else { return; };
    let material = ImpactFireMaterial {
        impact: ImpactFireUniform { seed: seed_from_entity(child_of.parent()), half_size: IMPACT_MESH_HALF_SIZE, ..default() },
    };
    commands.entity(trigger.entity).insert((
        unit_quad.mesh(),
        MeshMaterial2d(impact_fire_materials.add(material)),
        Transform::from_scale(Vec3::splat(2. * IMPACT_MESH_HALF_SIZE)),
        ZDepth::GROUND_EFFECT,
        Visibility::Hidden,
    ));
}

fn on_add_pilot_flame_set_up(
    trigger: On<Add<PilotFlame>>,
    mut commands: Commands,
    unit_quad: Res<UnitQuad>,
    mut pilot_materials: ResMut<Assets<PilotFlameMaterial>>,
    pilots: Query<&ChildOf>,
) {
    let Ok(child_of) = pilots.get(trigger.entity) else { return; };
    let material = PilotFlameMaterial { pilot: PilotFlameUniform { seed: seed_from_entity(child_of.parent()), size: PILOT_MESH_SIZE } };
    commands.entity(trigger.entity).insert((
        unit_quad.mesh(),
        MeshMaterial2d(pilot_materials.add(material)),
        Transform::from_scale(Vec3::splat(PILOT_MESH_SIZE)),
        ZDepth::GROUND_EFFECT,
        Visibility::Hidden,
    ));
}

fn update_flames(
    mut flame_materials: ResMut<Assets<FlameMaterial>>,
    mut glow_materials: ResMut<Assets<FlameGlowMaterial>>,
    mut flames: Query<(&ChildOf, &Children, &mut Transform, &mut Visibility, &MeshMaterial2d<FlameMaterial>), With<Flame>>,
    mut glows: Query<(&mut Transform, &MeshMaterial2d<FlameGlowMaterial>), (With<FlameGlow>, Without<Flame>)>,
    streams: Query<&FlameStream>,
) {
    for (child_of, children, mut transform, mut visibility, material) in flames.iter_mut() {
        let Ok(stream) = streams.get(child_of.parent()) else { continue; };
        let Some(fuel) = stream.fuel() else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        visibility.set_if_neq(Visibility::Inherited);

        let nozzle = stream.nozzle;
        let direction = stream.direction;
        let length = stream.length + FLAME_MESH_OVERHANG;
        let half_width = stream.half_width_at(length) * FLAME_MESH_MARGIN + FLAME_MESH_PADDING;
        transform.set_if_neq(Transform {
            translation: (nozzle + direction * length * 0.5).extend(transform.translation.z),
            rotation: Quat::from_rotation_z(direction.to_angle()),
            scale: Vec3::new(length, 2. * half_width, 1.),
        });

        let (tail, head) = fuel.into_inner();
        let end = stream.hit.as_ref().map_or(head, |hit| hit.distance);
        let hit = f32::from(stream.hit.is_some());
        let venting = stream.is_venting();
        if let Some(mut current) = flame_materials.get_mut(&material.0) {
            let uniform = FlameUniform {
                length,
                half_width,
                nozzle_half_width: stream.half_width_at(0.),
                spread: stream.spread(),
                head,
                tail,
                end,
                hit,
                venting: f32::from(venting),
                ..current.flame
            };
            if current.flame != uniform {
                current.flame = uniform;
            }
        }

        // Child of `Flame`: scale is relative to the parent stream quad.
        for &child in children {
            let Ok((mut glow_transform, glow_material)) = glows.get_mut(child) else { continue; };
            glow_transform.set_if_neq(Transform {
                scale: Vec3::new((length + 2. * GLOW_MESH_OVERHANG) / length, GLOW_MESH_HALF_WIDTH / half_width, 1.),
                ..*glow_transform
            });
            let glow = FlameGlowUniform {
                intensity: if venting { GLOW_VENTING } else { 1. },
                near: -GLOW_MESH_OVERHANG,
                far: length + GLOW_MESH_OVERHANG,
                half_width: GLOW_MESH_HALF_WIDTH,
                tail,
                end,
                hit,
            };
            if let Some(mut current) = glow_materials.get_mut(&glow_material.0) && current.glow != glow {
                current.glow = glow;
            }
        }
    }
}

fn update_impact_fires(
    mut commands: Commands,
    time: Res<Time>,
    unit_quad: Res<UnitQuad>,
    mut impact_fire_materials: ResMut<Assets<ImpactFireMaterial>>,
    mut scorch_materials: ResMut<Assets<FlameScorchMaterial>>,
    mut impact_fires: Query<(&ChildOf, &mut ImpactFire, &mut Transform, &mut Visibility, &MeshMaterial2d<ImpactFireMaterial>)>,
    streams: Query<&FlameStream>,
) {
    let delta = time.delta_secs();
    for (child_of, mut impact_fire, mut transform, mut visibility, material) in impact_fires.iter_mut() {
        let Ok(stream) = streams.get(child_of.parent()) else { continue; };
        let Some(mut current) = impact_fire_materials.get_mut(&material.0) else { continue; };
        let mut uniform = current.impact;

        let burnt = match &stream.hit {
            Some(hit) => {
                let tile_center = hit.coords.to_world_position_centered(GridImprint::default());
                transform.set_if_neq(Transform { translation: tile_center.extend(transform.translation.z), ..*transform });
                uniform.hit_point = stream.nozzle + stream.direction * hit.distance - tile_center;
                uniform.direction = stream.direction;
                impact_fire.burn(hit.coords, delta)
            }
            None => impact_fire.fade(delta),
        };
        uniform.intensity = impact_fire.intensity;
        if current.impact != uniform {
            current.impact = uniform;
        }
        visibility.set_if_neq(if impact_fire.intensity > 0. { Visibility::Inherited } else { Visibility::Hidden });

        if let Some(burnt) = burnt {
            let scorch = FlameScorchUniform {
                life: 0.,
                strength: burnt.strength,
                seed: (burnt.coords.x * 31 + burnt.coords.y) as f32 % 97.,
                half_size: SCORCH_MESH_HALF_SIZE,
            };
            commands.spawn((
                FlameScorch::new(),
                unit_quad.mesh(),
                MeshMaterial2d(scorch_materials.add(FlameScorchMaterial { scorch })),
                Transform::from_translation(burnt.coords.to_world_position_centered(GridImprint::default()).extend(0.))
                    .with_scale(Vec3::splat(2. * SCORCH_MESH_HALF_SIZE)),
                ZDepth::GROUND_DECAL,
            ));
        }
    }
}

fn advance_flame_scorches(
    mut commands: Commands,
    time: Res<Time>,
    mut scorch_materials: ResMut<Assets<FlameScorchMaterial>>,
    mut scorches: Query<(Entity, &mut FlameScorch, &MeshMaterial2d<FlameScorchMaterial>)>,
) {
    for (entity, mut scorch, material) in scorches.iter_mut() {
        let Some(life) = scorch.advance(time.delta_secs()) else {
            commands.entity(entity).despawn();
            continue;
        };
        let Some(mut current) = scorch_materials.get_mut(&material.0) else { continue; };
        current.scorch.life = life;
    }
}

fn update_pilot_flames(
    mut pilots: Query<(&ChildOf, &mut Transform, &mut Visibility), With<PilotFlame>>,
    streams: Query<&FlameStream>,
) {
    for (child_of, mut transform, mut visibility) in pilots.iter_mut() {
        let Ok(stream) = streams.get(child_of.parent()) else { continue; };
        let armed = stream.is_armed();
        visibility.set_if_neq(if armed { Visibility::Inherited } else { Visibility::Hidden });
        if !armed { continue; }

        let direction = stream.direction;
        let root = stream.nozzle + direction * PILOT_OFFSET;
        transform.set_if_neq(Transform {
            translation: (root + direction * PILOT_MESH_SIZE * 0.5).extend(transform.translation.z),
            rotation: Quat::from_rotation_z(direction.to_angle()),
            ..*transform
        });
    }
}
