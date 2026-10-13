//! # Flamethrower Flame
//!
//! Everything a Flamethrower draws. Gameplay lives in `tower_flamethrower`; this module only reads
//! its `FlameState` and `FlameImpact` and shows them.
//!
//! - `Flame` - the stream, from the nozzle to where the fuel ends. It carries a `FlameGlow` child,
//!   the warm light the stream casts on the ground.
//! - `ImpactFire` - the fire on the tile the stream is burning: a splash, fuel clinging to the hit
//!   wisp, the whole tile in low flames and sparks. It shows on the tile the fuel hits.
//! - `FlameScorch` - char left on a tile once it stops burning. It fades on its own.
//! - `PilotFlame` - a small flicker at the nozzle while the tower is armed and idle.
//!
//! All pieces are children of the tower's base. They follow the top's facing from
//! `TowerTopRotation`, so they need nothing from how the top itself is drawn.

use bevy::{
    mesh::MeshVertexBufferLayoutRef,
    prelude::*,
    reflect::TypePath,
    render::render_resource::{AsBindGroup, BlendState, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError},
    shader::ShaderRef,
    sprite_render::{AlphaMode2d, Material2d, Material2dKey, Material2dPipeline, Material2dPlugin, MeshMaterial2d},
};

use alteration::modifiers::prelude::*;
use buildings::prelude::*;
use game_core::prelude::*;
use states::prelude::*;
use visuals::shader::{ADDITIVE_BLENDING, seed_from_entity, set_blend};

use crate::common::*;

use super::tower_flamethrower::{FlameAxis, FlameImpact, FlameState, update_flame_impacts};

pub(crate) struct TowerFlamethrowerFlamePlugin;
impl Plugin for TowerFlamethrowerFlamePlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                Material2dPlugin::<FlameMaterial>::default(),
                Material2dPlugin::<FlameGlowMaterial>::default(),
                Material2dPlugin::<ImpactFireMaterial>::default(),
                Material2dPlugin::<FlameScorchMaterial>::default(),
                Material2dPlugin::<PilotFlameMaterial>::default(),
            ))
            .init_resource::<FlameQuad>()
            .add_systems(Update, (
                (update_flames, update_impact_fires, update_pilot_flames).after(update_flame_impacts),
                advance_flame_scorches,
            ).run_if(in_state(GameState::Running)))
            .add_observer(on_add_tower_flamethrower_spawn_flame);
    }
}

// Stream mesh
/// Mesh half-width relative to the stream's widest point, leaving room for ragged edges and soot.
const FLAME_MESH_MARGIN: f32 = 1.6;
/// Mesh half-width added on top, in world units, covering the stream's bulges.
const FLAME_MESH_PADDING: f32 = 13.;
/// Mesh length past the end of the range, where a free tip burns out into soot.
const FLAME_MESH_OVERHANG: f32 = 30.;

// Ground light
/// Half-width of the light's mesh across the stream, in world units. Wide enough for the light to
/// fade out before the mesh's edge.
const GLOW_MESH_HALF_WIDTH: f32 = 100.;
/// Length the light's mesh reaches past the stream's mesh at each end, in world units, so the pool
/// at a hit near either end fades out before the mesh's edge.
const GLOW_MESH_OVERHANG: f32 = 80.;
/// Light strength once the stream is cut off, relative to a firing stream.
const GLOW_VENTING: f32 = 0.6;

// Impact
/// How fast the fire builds up on a newly hit tile, per second.
const IMPACT_IGNITE_RATE: f32 = 12.;
/// How fast it dies down once the tile is no longer hit, per second.
const IMPACT_FADE_RATE: f32 = 4.;
/// Impact mesh half-size, in world units: the hit tile plus room for the splash and the sparks.
const IMPACT_MESH_HALF_SIZE: f32 = 70.;

// Scorch
/// Burning time after which a tile's scorch mark is at full strength, in seconds.
const SCORCH_BUILDUP: f32 = 1.;
const SCORCH_DURATION: f32 = 1.5;
const SCORCH_MESH_HALF_SIZE: f32 = 24.;

// Pilot flame
const PILOT_MESH_SIZE: f32 = 14.;
/// Distance of the pilot flame's root past the nozzle, in world units.
const PILOT_OFFSET: f32 = 1.5;

/// Unit square shared by every piece of the flame; each scales it to its size.
#[derive(Resource)]
struct FlameQuad(Handle<Mesh>);
impl FromWorld for FlameQuad {
    fn from_world(world: &mut World) -> Self {
        Self(world.resource_mut::<Assets<Mesh>>().add(Rectangle::new(1., 1.)))
    }
}

/// The fire stream from the nozzle of a Flamethrower. Child of the tower.
#[derive(Component)]
struct Flame {
    tower: Entity,
}

/// Field order and types mirror `Flame` in `assets/shaders/towers/flamethrower/stream.wesl`.
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

/// Outputs premultiplied color, so the fire glows over the scene while its soot covers it.
#[derive(Asset, AsBindGroup, TypePath, Clone, Copy, Debug, Default)]
struct FlameMaterial {
    #[uniform(0)]
    flame: FlameUniform,
}
impl Material2d for FlameMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/towers/flamethrower/stream.wesl".into()
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

/// The warm light a stream casts on the ground along itself and around its hit. Child of a `Flame`.
#[derive(Component)]
struct FlameGlow;

/// Field order and types mirror `FlameGlow` in `assets/shaders/towers/flamethrower/glow.wesl`.
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

/// Adds its light on top of what is underneath.
#[derive(Asset, AsBindGroup, TypePath, Clone, Copy, Debug, Default)]
struct FlameGlowMaterial {
    #[uniform(0)]
    glow: FlameGlowUniform,
}
impl Material2d for FlameGlowMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/towers/flamethrower/glow.wesl".into()
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

/// The fire on the tile a Flamethrower is burning. Child of the tower. Shown while the fuel hits a
/// tile, and dies down quickly once it no longer does.
#[derive(Component)]
struct ImpactFire {
    tower: Entity,
    /// 0 = out, 1 = the tile burns in full.
    intensity: f32,
    /// The tile it burns, kept while the fire dies down.
    coords: Option<GridCoords>,
    /// How long the current tile has been burning, in seconds.
    burning_for: f32,
}
impl ImpactFire {
    fn new(tower: Entity) -> Self {
        Self { tower, intensity: 0., coords: None, burning_for: 0. }
    }

    fn intensity(&self) -> f32 {
        self.intensity
    }

    /// Burns `coords` for another `delta` seconds. A new tile starts from no fire. Returns the tile
    /// the fire just moved off, if any.
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

    /// Lets the fire die down for `delta` seconds. Returns the tile that just stopped burning, if any.
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

/// A tile an `ImpactFire` stopped burning, and how deeply it is charred: 0..1.
struct BurntTile {
    coords: GridCoords,
    strength: f32,
}

/// Field order and types mirror `ImpactFire` in `assets/shaders/towers/flamethrower/impact.wesl`.
#[derive(ShaderType, Clone, Copy, Debug, Default, PartialEq)]
struct ImpactFireUniform {
    intensity: f32,
    seed: f32,
    half_size: f32,
    hit_point: Vec2,
    direction: Vec2,
}

/// Outputs premultiplied color, so the fire glows over the scene.
#[derive(Asset, AsBindGroup, TypePath, Clone, Copy, Debug, Default)]
struct ImpactFireMaterial {
    #[uniform(0)]
    impact: ImpactFireUniform,
}
impl Material2d for ImpactFireMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/towers/flamethrower/impact.wesl".into()
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

/// Char left on a tile a Flamethrower stopped burning. Despawns once it has faded.
#[derive(Component)]
#[require(MapBound)]
struct FlameScorch {
    age: f32,
}
impl FlameScorch {
    fn new() -> Self {
        Self { age: 0. }
    }

    /// Ages the mark by `delta` seconds. Returns its life, 0..1, or `None` once it has faded.
    fn advance(&mut self, delta: f32) -> Option<f32> {
        self.age += delta;
        (self.age < SCORCH_DURATION).then(|| self.age / SCORCH_DURATION)
    }
}

/// Field order and types mirror `FlameScorch` in `assets/shaders/towers/flamethrower/scorch.wesl`.
#[derive(ShaderType, Clone, Copy, Debug, Default, PartialEq)]
struct FlameScorchUniform {
    life: f32,
    strength: f32,
    seed: f32,
    half_size: f32,
}

/// Outputs premultiplied color, so the mark darkens the ground while its cinders glow.
#[derive(Asset, AsBindGroup, TypePath, Clone, Copy, Debug, Default)]
struct FlameScorchMaterial {
    #[uniform(0)]
    scorch: FlameScorchUniform,
}
impl Material2d for FlameScorchMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/towers/flamethrower/scorch.wesl".into()
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

/// A small flicker at the nozzle while the tower is armed: operational and not firing. Child of
/// the tower.
#[derive(Component)]
struct PilotFlame {
    tower: Entity,
}

/// Field order and types mirror `PilotFlame` in `assets/shaders/towers/flamethrower/pilot.wesl`.
#[derive(ShaderType, Clone, Copy, Debug, Default, PartialEq)]
struct PilotFlameUniform {
    seed: f32,
    size: f32,
}

/// Adds its light on top of what is underneath.
#[derive(Asset, AsBindGroup, TypePath, Clone, Copy, Debug, Default)]
struct PilotFlameMaterial {
    #[uniform(0)]
    pilot: PilotFlameUniform,
}
impl Material2d for PilotFlameMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/towers/flamethrower/pilot.wesl".into()
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

fn on_add_tower_flamethrower_spawn_flame(
    trigger: On<Add<TowerFlamethrower>>,
    mut commands: Commands,
    flame_quad: Res<FlameQuad>,
    mut flame_materials: ResMut<Assets<FlameMaterial>>,
    mut glow_materials: ResMut<Assets<FlameGlowMaterial>>,
    mut impact_fire_materials: ResMut<Assets<ImpactFireMaterial>>,
    mut pilot_materials: ResMut<Assets<PilotFlameMaterial>>,
) {
    let tower = trigger.entity;
    let seed = seed_from_entity(tower);
    let flame = FlameMaterial { flame: FlameUniform { seed, ..default() } };
    let impact_fire = ImpactFireMaterial { impact: ImpactFireUniform { seed, half_size: IMPACT_MESH_HALF_SIZE, ..default() } };
    let pilot = PilotFlameMaterial { pilot: PilotFlameUniform { seed, size: PILOT_MESH_SIZE } };
    commands.entity(tower).with_children(|parent| {
        parent.spawn((
            Flame { tower },
            Mesh2d(flame_quad.0.clone()),
            MeshMaterial2d(flame_materials.add(flame)),
            ZDepth::GROUND_EFFECT,
            Visibility::Hidden,
            children![(
                FlameGlow,
                Mesh2d(flame_quad.0.clone()),
                MeshMaterial2d(glow_materials.add(FlameGlowMaterial::default())),
                ZDepth::GROUND_DECAL,
            )],
        ));
        parent.spawn((
            ImpactFire::new(tower),
            Mesh2d(flame_quad.0.clone()),
            MeshMaterial2d(impact_fire_materials.add(impact_fire)),
            Transform::from_scale(Vec3::splat(2. * IMPACT_MESH_HALF_SIZE)),
            ZDepth::GROUND_EFFECT,
            Visibility::Hidden,
        ));
        parent.spawn((
            PilotFlame { tower },
            Mesh2d(flame_quad.0.clone()),
            MeshMaterial2d(pilot_materials.add(pilot)),
            Transform::from_scale(Vec3::splat(PILOT_MESH_SIZE)),
            ZDepth::GROUND_EFFECT,
            Visibility::Hidden,
        ));
    });
}

/// Shows each flame while its tower's stream holds fuel, lays it along the top's facing from the
/// nozzle past the end of the range, and lights the ground under it. The stream ends at the hit, or
/// as far as its head has reached.
fn update_flames(
    mut flame_materials: ResMut<Assets<FlameMaterial>>,
    mut glow_materials: ResMut<Assets<FlameGlowMaterial>>,
    mut flames: Query<(&Flame, &Children, &mut Transform, &mut Visibility, &MeshMaterial2d<FlameMaterial>)>,
    mut glows: Query<(&mut Transform, &MeshMaterial2d<FlameGlowMaterial>), (With<FlameGlow>, Without<Flame>)>,
    towers: Query<(&TowerTopRotation, &AttackRange, &GridImprint, &FlameState, &FlameImpact), With<TowerFlamethrower>>,
) {
    for (flame, children, mut transform, mut visibility, material) in flames.iter_mut() {
        let Ok((top_rotation, attack_range, grid_imprint, state, impact)) = towers.get(flame.tower) else { continue; };
        let Some(fuel) = state.fuel() else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        visibility.set_if_neq(Visibility::Inherited);

        // Local to the tower's base.
        let axis = FlameAxis::new(Vec2::ZERO, top_rotation.current_angle, grid_imprint, attack_range);
        let length = axis.length + FLAME_MESH_OVERHANG;
        let half_width = axis.half_width_at(length) * FLAME_MESH_MARGIN + FLAME_MESH_PADDING;
        transform.translation = (axis.nozzle + axis.direction * length * 0.5).extend(transform.translation.z);
        transform.rotation = Quat::from_rotation_z(top_rotation.current_angle);
        transform.scale = Vec3::new(length, 2. * half_width, 1.);

        let (tail, head) = fuel.into_inner();
        let end = impact.hit().map_or(head, |hit| hit.distance);
        let hit = f32::from(impact.hit().is_some());
        let venting = matches!(state, FlameState::Venting { .. });
        if let Some(mut current) = flame_materials.get_mut(&material.0) {
            let uniform = FlameUniform {
                length,
                half_width,
                nozzle_half_width: axis.half_width_at(0.),
                spread: axis.spread(),
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

        // The light reaches past the stream's mesh at both ends. Its scale is set against the
        // stream's, which it is a child of.
        for &child in children {
            let Ok((mut glow_transform, glow_material)) = glows.get_mut(child) else { continue; };
            glow_transform.scale = Vec3::new((length + 2. * GLOW_MESH_OVERHANG) / length, GLOW_MESH_HALF_WIDTH / half_width, 1.);
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

/// Moves each impact fire onto its tower's hit tile and builds it up there, or lets it die down
/// once nothing is hit. A tile that stops burning gets a scorch mark.
fn update_impact_fires(
    mut commands: Commands,
    time: Res<Time>,
    flame_quad: Res<FlameQuad>,
    mut impact_fire_materials: ResMut<Assets<ImpactFireMaterial>>,
    mut scorch_materials: ResMut<Assets<FlameScorchMaterial>>,
    mut impact_fires: Query<(&mut ImpactFire, &mut Transform, &mut Visibility, &MeshMaterial2d<ImpactFireMaterial>)>,
    towers: Query<(&Transform, &TowerTopRotation, &GridImprint, &AttackRange, &FlameImpact), (With<TowerFlamethrower>, Without<ImpactFire>)>,
) {
    let delta = time.delta_secs();
    for (mut impact_fire, mut transform, mut visibility, material) in impact_fires.iter_mut() {
        let Ok((tower_transform, top_rotation, grid_imprint, attack_range, impact)) = towers.get(impact_fire.tower) else { continue; };
        let Some(mut current) = impact_fire_materials.get_mut(&material.0) else { continue; };
        let mut uniform = current.impact;

        let burnt = match impact.hit() {
            Some(hit) => {
                let tower_position = tower_transform.translation.xy();
                let axis = FlameAxis::new(tower_position, top_rotation.current_angle, grid_imprint, attack_range);
                let tile_center = hit.coords.to_world_position_centered(GridImprint::default());
                transform.translation = (tile_center - tower_position).extend(transform.translation.z);
                uniform.hit_point = axis.nozzle + axis.direction * hit.distance - tile_center;
                uniform.direction = axis.direction;
                impact_fire.burn(hit.coords, delta)
            }
            None => impact_fire.fade(delta),
        };
        uniform.intensity = impact_fire.intensity();
        if current.impact != uniform {
            current.impact = uniform;
        }
        visibility.set_if_neq(if impact_fire.intensity() > 0. { Visibility::Inherited } else { Visibility::Hidden });

        if let Some(burnt) = burnt {
            let scorch = FlameScorchUniform {
                life: 0.,
                strength: burnt.strength,
                seed: (burnt.coords.x * 31 + burnt.coords.y) as f32 % 97.,
                half_size: SCORCH_MESH_HALF_SIZE,
            };
            commands.spawn((
                FlameScorch::new(),
                Mesh2d(flame_quad.0.clone()),
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

/// Keeps each pilot flame at the nozzle and shows it only while its tower is armed.
fn update_pilot_flames(
    mut pilots: Query<(&PilotFlame, &mut Transform, &mut Visibility)>,
    towers: Query<(&TowerTopRotation, &AttackRange, &GridImprint, &FlameState, Has<IsOperational>), With<TowerFlamethrower>>,
) {
    for (pilot, mut transform, mut visibility) in pilots.iter_mut() {
        let Ok((top_rotation, attack_range, grid_imprint, state, operational)) = towers.get(pilot.tower) else { continue; };
        let armed = operational && matches!(state, FlameState::Idle);
        visibility.set_if_neq(if armed { Visibility::Inherited } else { Visibility::Hidden });
        if !armed { continue; }

        let axis = FlameAxis::new(Vec2::ZERO, top_rotation.current_angle, grid_imprint, attack_range);
        // The mesh starts just behind the flame's root.
        let root = axis.nozzle + axis.direction * PILOT_OFFSET;
        transform.translation = (root + axis.direction * PILOT_MESH_SIZE * 0.5).extend(transform.translation.z);
        transform.rotation = Quat::from_rotation_z(top_rotation.current_angle);
    }
}
