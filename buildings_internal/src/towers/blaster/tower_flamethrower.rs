//! # Flamethrower Tower
//!
//! Fire evolution of the Blaster. Sprays a continuous stream of fire at its target. The stream stops
//! at the first wisp it meets and engulfs it, damaging every wisp on that tile, so the tower keeps
//! the Blaster's single-target identity. Short range is the price.
//!
//! ## Firing
//!
//! The stream cycles through `FlameState`. It ignites once the top faces the target within
//! `IGNITE_ANGLE`. It keeps firing while the target stays within `CUTOFF_ANGLE`, so it follows a
//! burning target and moves on to a neighbor without a break. A target beyond the cutoff, or no
//! target left, cuts the stream off. Targeting then stays suspended while the fire in flight
//! dissipates. The top holds still and turns only once the fire is gone, so a burning stream never
//! swings across the map.
//!
//! ## Impact
//!
//! The hit comes from the top's actual facing, not the target's direction: the nearest wisp along
//! the stream's axis, inside the cone and within range. A wisp in front of the target takes the hit.

use bevy::{
    platform::collections::HashMap,
    prelude::*,
    reflect::TypePath,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
    sprite_render::{AlphaMode2d, Material2d, Material2dPlugin, MeshMaterial2d},
};

use alteration::{
    effects::prelude::*,
    modifiers::prelude::*,
};
use almanach::{BuildingInfo, ObjectPresentation, prelude::*};
use buildings::prelude::*;
use game_core::{math::angle_difference, prelude::*};
use grids::{
    placement::{annotate_non_empty, PlacementModes, PlaceRequest},
    wisps::WispsGrid,
};
use hud::prelude::{IndicatorDisplay, IndicatorType, Indicators};
use logging::prelude::*;
use persistence::prelude::*;
use resources::prelude::*;
use shards::prelude::*;
use states::prelude::*;
use wisps::prelude::*;

use crate::{common::*, tooltip::building_tooltip};

pub(crate) struct TowerFlamethrowerPlugin;
impl Plugin for TowerFlamethrowerPlugin {
    fn build(&self, app: &mut App) {
        let almanach_info = BuilderTowerFlamethrower::almanach_info(app.world().resource::<AssetServer>());
        app
            .add_plugins(Material2dPlugin::<FlameMaterial>::default())
            .add_systems(Update, (
                update_flame_states,
                update_flame_impacts,
                damage_flame_impacts,
                update_flames,
            ).chain().run_if(in_state(GameState::Running)))
            .add_observer(BuilderTowerFlamethrower::on_builder_add_spawn_tower_flamethrower)
            .add_observer(on_tower_flamethrower_place_request_do_so)
            .add_systems(CollectSave, collect_tower_flamethrowers)
            .register_loader(MapLoadingStage::SpawnMapElements, "tower_flamethrowers", load_tower_flamethrowers)
            .register_building(BuildingType::Tower(TowerType::Flamethrower), almanach_info);
    }
}

// Flame stream geometry
const CONE_HALF_ANGLE: f32 = 7_f32.to_radians();
/// Distance from the tower center to the nozzle, relative to the tower's world width.
const NOZZLE_OFFSET: f32 = 0.4;
const NOZZLE_HALF_WIDTH: f32 = 1.5;
/// Mesh width relative to the cone's widest point, leaving room for the billow's puffs.
const FLAME_MESH_MARGIN: f32 = 2.5;
const WISP_HIT_RADIUS: f32 = 8.;

// Firing gate
const IGNITE_ANGLE: f32 = 8_f32.to_radians();
const CUTOFF_ANGLE: f32 = 25_f32.to_radians();
/// How long targeting stays suspended after a cutoff: until the flame has fully faded.
const VENT_DURATION: f32 = 1. / EXTINGUISH_RATE;

// Flame visuals
const IGNITION_RATE: f32 = 6.;
const EXTINGUISH_RATE: f32 = 5.;
/// How fast the stream's front travels out from the nozzle, in world units per second.
const FRONT_EXTEND_SPEED: f32 = 1200.;
/// How fast the front pulls back when something closer gets in the way.
const FRONT_RETRACT_SPEED: f32 = 6000.;
/// How far the splash reaches along the stream around the impact point, at full force, right at the nozzle.
const SPLASH_RADIUS: f32 = 32.;
/// How much width the splash adds on each side of the stream at full force, right at the nozzle.
const SPLASH_HALF_WIDTH: f32 = 24.;
const SPLASH_FADE_RATE: f32 = 8.;
/// Share of the stream within which a hit stops the stream and splashes in full.
const REACTIVE_FULL: f32 = 0.5;
/// Share of the stream beyond which a hit lands in the fumes, which pass it without reacting.
const REACTIVE_END: f32 = 0.75;
/// How deep into the hit wisp the stream's front reaches, relative to its radius: 0 = near side, 1 = center.
const FRONT_DEPTH: f32 = 0.5;

#[derive(Component, SSS)]
pub(crate) struct BuilderTowerFlamethrower {
    pub grid_position: GridCoords,
    /// Saved integrity points. `None` ⇒ defer to baseline (fresh spawn);
    /// `Some` ⇒ override with saved value (restore).
    pub integrity_points: Option<IntegrityPoints>,
    /// Set when the player disabled this building. `None` on fresh spawn.
    pub disabled_by_player: Option<DisabledByPlayer>,
}
impl BuilderTowerFlamethrower {
    pub fn almanach_info(asset_server: &AssetServer) -> BuildingInfo {
        BuildingInfo {
            name: "Flamethrower Tower".to_string(),
            description: "A continuous stream of fire. Short range, but it engulfs the first wisp in its path and everything sharing its tile.".to_string(),
            sprite: asset_server.load("buildings/tower_blaster.png"),
            top_sprite: Some(asset_server.load("buildings/tower_blaster_top.png")),
            grid_imprint: GridImprint::Rectangle { width: 2, height: 2 },
            cost: vec![ResourceAmount::new(ResourceType::DarkOre, 150)],
            baseline: HashMap::from([
                (ModifierType::MaxIntegrityPoints, 100.),
                (ModifierType::AttackRange, 6.),
                (ModifierType::AttackSpeed, 10.),
                (ModifierType::AttackDamage, 0.5),
            ]),
            sockets: vec![
                ("attack_speed".into(), ShardSocket::new(ShardType::Speed, "Attack speed", ModifierType::AttackSpeed, [2., 4., 6.])),
                ("attack_range".into(), ShardSocket::new(ShardType::Reach, "Attack range", ModifierType::AttackRange, [1., 2., 3.])),
                ("damage".into(), ShardSocket::new(ShardType::Strength, "Damage", ModifierType::AttackDamage, [1., 2., 3.])),
            ],
            validate: building_validator,
            annotate: annotate_non_empty,
            placement: PlacementModes::default(),
            presentation: ObjectPresentation {
                tooltip: Some(building_tooltip),
            },
        }
    }

    pub fn new(grid_position: GridCoords) -> Self {
        Self { grid_position, integrity_points: None, disabled_by_player: None }
    }
    pub fn with_integrity_points(mut self, integrity_points: f32) -> Self {
        self.integrity_points = Some(IntegrityPoints::new(integrity_points));
        self
    }
    pub fn with_disabled_by_player(mut self, disabled_by_player: bool) -> Self {
        self.disabled_by_player = disabled_by_player.then_some(DisabledByPlayer);
        self
    }

    pub fn on_builder_add_spawn_tower_flamethrower(
        trigger: On<Add<BuilderTowerFlamethrower>>,
        mut commands: Commands,
        almanach: Res<Almanach>,
        mut meshes: ResMut<Assets<Mesh>>,
        mut flame_materials: ResMut<Assets<FlameMaterial>>,
        builders: Query<&BuilderTowerFlamethrower>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return; };

        let building_info = almanach.get_building_info(BuildingType::Tower(TowerType::Flamethrower));
        let grid_imprint = building_info.grid_imprint;

        commands.entity(entity)
            .remove::<BuilderTowerFlamethrower>()
            .insert_some(builder.integrity_points)
            .insert_some(builder.disabled_by_player)
            .insert((
                TowerFlamethrower,
                Sprite {
                    image: building_info.sprite.clone(),
                    custom_size: Some(grid_imprint.world_size()),
                    ..default()
                },
                builder.grid_position,
                grid_imprint,
                TowerTopRotation { speed: 10.0, current_angle: 0. },
                FlameState::default(),
                FlameImpact::default(),
                NeedsPower,
                related![Indicators[
                    IndicatorType::NoPower,
                    IndicatorType::DisabledByPlayer,
                ]],
                related![EffectInstances[
                    (ModifierContributions(building_info.baseline.clone()), BaselineEffect),
                ]],
                children![
                    IndicatorDisplay::default(),
                ],
            ))
            .observe(on_technical_state_changed_recompute_operational);
        let world_size = grid_imprint.world_size();
        let tower_top = commands.spawn((
            Sprite {
                image: building_info.top_sprite.clone().expect("Flamethrower Tower defines a top sprite"),
                custom_size: Some(Vec2::new(world_size.x * 1.52 * 0.5, world_size.y * 0.5)),
                ..default()
            },
            MarkerTowerRotationalTop(entity),
            children![(
                Flame { tower: entity, intensity: 0., front: 0., splash: 0. },
                Mesh2d(meshes.add(Rectangle::new(1., 1.))),
                MeshMaterial2d(flame_materials.add(FlameMaterial::default())),
                ZDepth::GROUND_EFFECT,
                Visibility::Hidden,
            )],
        )).id();
        commands.entity(entity).add_child(tower_top);
        for (content_id, socket) in &building_info.sockets {
            commands.trigger(ShardSocketUpsert::new(entity, content_id.clone(), socket.clone()));
        }
        commands.trigger(TechnicalStateChanged { entity, kind: TechnicalChange::JustSpawned });
    }
}

fn on_tower_flamethrower_place_request_do_so(
    trigger: On<PlaceRequest>,
    mut commands: Commands,
    mut placement: BuildingPlacementManager,
) {
    let PlaceRequest(MapObject::Building(BuildingType::Tower(TowerType::Flamethrower))) = *trigger else { return };
    let Some(coords) = placement.claim(BuildingType::Tower(TowerType::Flamethrower)) else { return };
    commands.spawn(BuilderTowerFlamethrower::new(coords));
}

#[log_tags(Tag::GameSave)]
fn collect_tower_flamethrowers(
    towers: Query<(Entity, &GridCoords, &IntegrityPoints, Has<DisabledByPlayer>), With<TowerFlamethrower>>,
    mut save: SaveWriter,
) {
    if towers.is_empty() { return; }

    #[debug_dev("Saving {} tower flamethrowers", rows.len())]
    let rows: Vec<(u32, GridCoords, f32, bool)> = towers
        .iter()
        .map(|(entity, coords, integrity_points, disabled_by_player)| {
            (
                entity.index_u32(),
                *coords,
                integrity_points.get_current(),
                disabled_by_player,
            )
        })
        .collect();
    save.submit(move |ctx| {
        for (id, coords, integrity_points, disabled_by_player) in rows {
            ctx.save_marker("tower_flamethrowers", id)?;
            ctx.save_grid_coords(id, coords)?;
            ctx.save_integrity_points(id, integrity_points)?;
            if disabled_by_player {
                ctx.save_disabled_by_player(id)?;
            }
        }
        Ok(())
    });
}

fn load_tower_flamethrowers(ctx: &mut LoadContext) -> LoadResult {
    ctx.for_each_entity("SELECT id FROM tower_flamethrowers", |ctx, old_id, entity, _| {
        let grid_position = ctx.grid_coords(old_id)?;
        let integrity_points = ctx.integrity_points(old_id)?;
        let disabled_by_player = ctx.disabled_by_player(old_id)?;
        let builder = BuilderTowerFlamethrower::new(grid_position)
            .with_integrity_points(integrity_points)
            .with_disabled_by_player(disabled_by_player);
        ctx.insert(entity, builder);
        Ok(())
    })
}

#[derive(Component, Default)]
enum FlameState {
    /// No stream; the top turns freely toward its target.
    #[default]
    Idle,
    Firing,
    /// The stream was cut off; targeting is suspended while the fire in flight dissipates.
    Venting { remaining: f32 },
}
impl FlameState {
    fn is_firing(&self) -> bool {
        matches!(self, Self::Firing)
    }
}

/// What the stream hits this frame; `None` while not firing or when nothing is in the way.
#[derive(Component, Default)]
struct FlameImpact(Option<FlameHit>);

struct FlameHit {
    /// The tile whose wisps all take the damage.
    coords: GridCoords,
    /// Distance from the nozzle to the hit wisp along the stream.
    distance: f32,
}

/// Stream geometry along the top's facing.
struct FlameAxis {
    nozzle: Vec2,
    direction: Vec2,
    /// Distance from the nozzle to the end of the range.
    length: f32,
}
impl FlameAxis {
    fn new(tower_position: Vec2, angle: f32, grid_imprint: &GridImprint, attack_range: &AttackRange) -> Self {
        // The stream reaches `attack_range` cells beyond the tower's footprint.
        let tower_width = grid_imprint.world_size().x;
        let nozzle_offset = tower_width * NOZZLE_OFFSET;
        let reach = attack_range.get() * CELL_SIZE + tower_width * 0.5;
        let direction = Vec2::from_angle(angle);
        Self {
            nozzle: tower_position + direction * nozzle_offset,
            direction,
            length: (reach - nozzle_offset).max(CELL_SIZE),
        }
    }

    fn half_width_at(&self, distance: f32) -> f32 {
        NOZZLE_HALF_WIDTH + distance * CONE_HALF_ANGLE.tan()
    }

    /// Distance along the stream at which it hits a wisp at `position`, if the wisp is inside the cone.
    fn hit_distance(&self, position: Vec2) -> Option<f32> {
        let offset = position - self.nozzle;
        let distance = offset.dot(self.direction);
        let lateral = offset.perp_dot(self.direction).abs();
        ((0.0..=self.length).contains(&distance) && lateral <= self.half_width_at(distance) + WISP_HIT_RADIUS)
            .then_some(distance)
    }

    /// The cell columns under the stream, nozzle first, each with the shortest stream distance a wisp
    /// in it can have. A column holds every cell where the center of a wisp the stream hits can be.
    fn columns(&self) -> impl Iterator<Item = (f32, GridCoords, GridImprint)> {
        // Columns cut across the grid axis the stream runs along most.
        let along = if self.direction.x.abs() >= self.direction.y.abs() { 0 } else { 1 };
        let across = 1 - along;
        let max_reach = self.half_width_at(self.length) + WISP_HIT_RADIUS;
        // How far a wisp's stream distance can differ from the centerline's within one column.
        let max_lead = max_reach * (self.direction[across] / self.direction[along]).abs();
        let cell_of = move |world: f32| (world / CELL_SIZE).floor() as i32;
        let distance_of = move |world: f32| (world - self.nozzle[along]) / self.direction[along];
        let step = self.direction[along].signum() as i32;
        let first = cell_of(self.nozzle[along] - max_reach * step as f32);
        let last = cell_of(self.nozzle[along] + self.direction[along] * self.length + max_reach * step as f32);
        (0..=(last - first) * step).map(move |index| {
            let column = first + index * step;
            let edges = [distance_of(column as f32 * CELL_SIZE), distance_of((column + 1) as f32 * CELL_SIZE)];
            let near = (edges[0].min(edges[1]) - max_lead).clamp(0., self.length);
            let far = (edges[0].max(edges[1]) + max_lead).clamp(0., self.length);
            let reach = self.half_width_at(far) + WISP_HIT_RADIUS;
            let ends = [near, far].map(|distance| self.nozzle[across] + self.direction[across] * distance);
            let low = cell_of(ends[0].min(ends[1]) - reach);
            let high = cell_of(ends[0].max(ends[1]) + reach);
            let (origin, imprint) = if along == 0 {
                (GridCoords { x: column, y: low }, GridImprint::Rectangle { width: 1, height: high - low + 1 })
            } else {
                (GridCoords { x: low, y: column }, GridImprint::Rectangle { width: high - low + 1, height: 1 })
            };
            (near, origin, imprint)
        })
    }
}

/// Where the top stands relative to its target this frame.
enum Aim {
    /// Angle between the top's facing and the target.
    Target(f32),
    NoTarget,
    /// The target died or a search is pending. Targeting resolves it within a frame.
    Pending,
}

fn update_flame_states(
    time: Res<Time>,
    mut towers: Query<(&Transform, &TowerTopRotation, &mut TowerWispTarget, &mut FlameState, Has<IsOperational>), With<TowerFlamethrower>>,
    wisps: Query<&Transform, With<Wisp>>,
) {
    for (transform, top_rotation, mut target, mut state, operational) in towers.iter_mut() {
        let aim = match *target {
            _ if !operational => Aim::NoTarget,
            TowerWispTarget::Wisp(target_wisp) => match wisps.get(target_wisp) {
                Ok(wisp) => {
                    let direction_to_target = wisp.translation.xy() - transform.translation.xy();
                    Aim::Target(angle_difference(direction_to_target.to_angle(), top_rotation.current_angle).abs())
                }
                Err(_) => Aim::Pending,
            },
            TowerWispTarget::SearchForNewTarget => Aim::Pending,
            TowerWispTarget::NoValidTargets(_) | TowerWispTarget::Suspended => Aim::NoTarget,
        };
        match (&mut *state, aim) {
            (FlameState::Idle, Aim::Target(angle_off)) if angle_off <= IGNITE_ANGLE => {
                *state = FlameState::Firing;
            }
            (FlameState::Firing, Aim::NoTarget) => {
                *state = FlameState::Venting { remaining: VENT_DURATION };
                *target = TowerWispTarget::Suspended;
            }
            (FlameState::Firing, Aim::Target(angle_off)) if angle_off > CUTOFF_ANGLE => {
                *state = FlameState::Venting { remaining: VENT_DURATION };
                *target = TowerWispTarget::Suspended;
            }
            (FlameState::Venting { remaining }, _) => {
                *remaining -= time.delta_secs();
                if *remaining <= 0. {
                    *state = FlameState::Idle;
                    *target = TowerWispTarget::SearchForNewTarget;
                }
            }
            _ => {}
        }
    }
}

/// Walks the cells under the stream column by column and picks the tile of the nearest wisp inside the cone.
fn update_flame_impacts(
    wisps_grid: Res<WispsGrid>,
    mut towers: Query<(&Transform, &TowerTopRotation, &GridImprint, &AttackRange, &FlameState, &mut FlameImpact), With<TowerFlamethrower>>,
    wisps: Query<&Transform, With<Wisp>>,
) {
    for (transform, top_rotation, grid_imprint, attack_range, state, mut impact) in towers.iter_mut() {
        if !state.is_firing() {
            impact.0 = None;
            continue;
        }
        let axis = FlameAxis::new(transform.translation.xy(), top_rotation.current_angle, grid_imprint, attack_range);
        let mut nearest: Option<FlameHit> = None;
        for (column_distance, origin, imprint) in axis.columns() {
            if nearest.as_ref().is_some_and(|hit| column_distance > hit.distance) { break; }
            for coords in imprint.iter_in_bounds(origin, wisps_grid.bounds) {
                for &wisp in &wisps_grid[coords] {
                    let Ok(wisp_transform) = wisps.get(wisp) else { continue; };
                    let Some(hit_distance) = axis.hit_distance(wisp_transform.translation.xy()) else { continue; };
                    if nearest.as_ref().is_none_or(|hit| hit_distance < hit.distance) {
                        nearest = Some(FlameHit { coords, distance: hit_distance });
                    }
                }
            }
        }
        impact.0 = nearest;
    }
}

/// Each shooting tick damages every wisp on the impact tile.
fn damage_flame_impacts(
    mut damage_messages: MessageWriter<DamageMessage>,
    wisps_grid: Res<WispsGrid>,
    mut towers: Query<(&FlameImpact, &AttackDamage, &mut TowerShootingTimer), (With<TowerFlamethrower>, With<IsOperational>)>,
) {
    for (impact, attack_damage, mut timer) in towers.iter_mut() {
        let Some(hit) = &impact.0 else { continue; };
        if !timer.0.is_finished() { continue; }
        for &wisp in &wisps_grid[hit.coords] {
            damage_messages.write(DamageMessage { target: wisp, amount: attack_damage.get() });
        }
        timer.0.reset();
    }
}

/// The fire stream drawn from the nozzle of a Flamethrower's rotating top.
#[derive(Component)]
struct Flame {
    tower: Entity,
    /// 0 = extinguished, 1 = full stream.
    intensity: f32,
    /// Distance from the nozzle where the stream ends, smoothed toward the impact or the full range.
    front: f32,
    /// 0 = no hit, 1 = full splash at the front, smoothed.
    splash: f32,
}

/// Field order and types mirror `Flame` in `assets/shaders/towers/flame.wesl`.
#[derive(ShaderType, Clone, Copy, Debug, Default, PartialEq)]
struct FlameUniform {
    intensity: f32,
    seed: f32,
    start_half_width: f32,
    end_half_width: f32,
    aspect: f32,
    front: f32,
    splash_radius: f32,
    splash: f32,
    splash_half_width: f32,
}

#[derive(Asset, AsBindGroup, TypePath, Clone, Copy, Debug, Default)]
pub(crate) struct FlameMaterial {
    #[uniform(0)]
    flame: FlameUniform,
}
impl Material2d for FlameMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/towers/flame.wesl".into()
    }
    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

/// Fades each flame in while its tower fires and out otherwise, and stretches it from the nozzle to
/// the end of the tower's range. The stream ends at its front, which shoots out on ignition and
/// follows the impact. A cut-off stream keeps its front and splash while it fades.
fn update_flames(
    time: Res<Time>,
    mut flame_materials: ResMut<Assets<FlameMaterial>>,
    mut flames: Query<(&mut Flame, &mut Transform, &mut Visibility, &MeshMaterial2d<FlameMaterial>)>,
    towers: Query<(&AttackRange, &GridImprint, &FlameState, &FlameImpact), With<TowerFlamethrower>>,
) {
    for (mut flame, mut transform, mut visibility, material) in flames.iter_mut() {
        let Ok((attack_range, grid_imprint, state, impact)) = towers.get(flame.tower) else { continue; };

        // Local to the top, which carries the tower's position and facing.
        let axis = FlameAxis::new(Vec2::ZERO, 0., grid_imprint, attack_range);
        let length = axis.length;

        if state.is_firing() {
            if flame.intensity == 0. {
                flame.front = 0.;
            }
            // The stream stops partway into the hit wisp. A hit's reaction fades out toward the
            // fumes, where the stream no longer has the force to splash around a wisp and keeps its
            // full length.
            let (target_front, target_splash) = match &impact.0 {
                Some(hit) => {
                    let reactivity = (hit.distance / length).remap(REACTIVE_FULL, REACTIVE_END, 1., 0.).clamp(0., 1.);
                    let stop = (hit.distance - WISP_HIT_RADIUS * (1. - FRONT_DEPTH)).max(0.);
                    (length.lerp(stop, reactivity), reactivity)
                }
                None => (length, 0.),
            };
            flame.front = if target_front < flame.front {
                (flame.front - FRONT_RETRACT_SPEED * time.delta_secs()).max(target_front)
            } else {
                (flame.front + FRONT_EXTEND_SPEED * time.delta_secs()).min(target_front)
            };
            let splash_step = SPLASH_FADE_RATE * time.delta_secs();
            flame.splash += (target_splash - flame.splash).clamp(-splash_step, splash_step);
            flame.intensity = (flame.intensity + IGNITION_RATE * time.delta_secs()).min(1.);
        } else {
            flame.intensity = (flame.intensity - EXTINGUISH_RATE * time.delta_secs()).max(0.);
        }
        visibility.set_if_neq(if flame.intensity > 0. { Visibility::Inherited } else { Visibility::Hidden });
        if flame.intensity == 0. { continue; }

        let end_half_width = axis.half_width_at(length);
        let width = 2. * end_half_width * FLAME_MESH_MARGIN;
        transform.translation.x = axis.nozzle.x + length * 0.5;
        transform.scale = Vec3::new(length, width, 1.);

        let uniform = FlameUniform {
            intensity: flame.intensity,
            seed: (flame.tower.index_u32() % 1024) as f32 * 7.31,
            start_half_width: NOZZLE_HALF_WIDTH / width,
            end_half_width: end_half_width / width,
            aspect: length / width,
            front: (flame.front / length).min(1.),
            splash_radius: SPLASH_RADIUS / length,
            splash: flame.splash,
            splash_half_width: SPLASH_HALF_WIDTH / width,
        };
        let Some(mut current) = flame_materials.get_mut(&material.0) else { continue; };
        if current.flame != uniform {
            current.flame = uniform;
        }
    }
}
