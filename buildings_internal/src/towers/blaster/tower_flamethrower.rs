//! # Flamethrower Tower
//!
//! Fire evolution of the Blaster. Sprays a continuous `FlameStream` (linked via
//! `GeneratedFlameStream`) that stops at the nearest wisp in the cone and damages all wisps on that
//! cell each `TowerShootingTimer` tick.
//!
//! ## Aim
//!
//! Stream geometry follows `TowerTopRotation::current_angle` (not the target vector) from the turret
//! nozzle (`NOZZLE_OFFSET * width`) out to `attack_range` cells beyond the tower footprint.
//!
//! ## Firing & Target Suspension
//!
//! Ignites when the turret faces the target within `IGNITE_ANGLE` and continues firing while within
//! `CUTOFF_ANGLE`. If the target leaves `CUTOFF_ANGLE`, dies, or the tower loses `IsOperational`,
//! the stream is cut off (`Venting`) and `TowerWispTarget` is set to `Suspended` so the turret does
//! not rotate while in-flight fuel burns out. `FlameStreamBurntOut` restores `SearchForNewTarget`.

use bevy::{
    platform::collections::HashMap,
    prelude::*,
};

use almanach::{BuildingInfo, ObjectPresentation, prelude::*};
use alteration::{
    effects::prelude::*,
    modifiers::prelude::*,
};
use buildings::prelude::*;
use game_core::prelude::*;
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
use weaponry::{
    flame_stream::{FlameStreamBurntOut, FlameStreamSystems, GeneratedFlameStream},
    prelude::*,
};
use wisps::prelude::*;

use crate::{common::*, tooltip::building_tooltip};

pub(crate) struct TowerFlamethrowerPlugin;
impl Plugin for TowerFlamethrowerPlugin {
    fn build(&self, app: &mut App) {
        let almanach_info = BuilderTowerFlamethrower::almanach_info(app.world().resource::<AssetServer>());
        app
            .add_systems(Update, (
                aim_flame_streams.after(TowerAimingSystems).before(FlameStreamSystems),
                (
                    update_flame_stream_firing.after(TowerAimingSystems).before(FlameStreamSystems),
                    damage_flame_hits.after(FlameStreamSystems),
                ).run_if(in_state(GameState::Running)),
            ))
            .add_observer(BuilderTowerFlamethrower::on_builder_add_spawn_tower_flamethrower)
            .add_observer(on_tower_flamethrower_place_request_do_so)
            .add_observer(on_add_is_operational_arm_flame_stream)
            .add_observer(on_remove_is_operational_disarm_flame_stream)
            .add_observer(on_flame_stream_burnt_out_resume_targeting)
            .add_systems(CollectSave, collect_tower_flamethrowers)
            .register_loader(MapLoadingStage::SpawnMapElements, "tower_flamethrowers", load_tower_flamethrowers)
            .register_building(BuildingType::Tower(TowerType::Flamethrower), almanach_info);
    }
}

// Aim
/// Distance from the tower center to the nozzle, relative to the tower's world width.
const NOZZLE_OFFSET: f32 = 0.4;

// Firing gate
const IGNITE_ANGLE: f32 = 8_f32.to_radians();
const CUTOFF_ANGLE: f32 = 25_f32.to_radians();

#[derive(Component, SSS)]
pub(crate) struct BuilderTowerFlamethrower {
    pub grid_position: GridCoords,
    /// Custom integrity points, or `None` to use the fresh-spawn baseline.
    pub integrity_points: Option<IntegrityPoints>,
    /// Set when the player disabled this building. `None` on fresh spawn.
    pub disabled_by_player: Option<DisabledByPlayer>,
    /// Saved reload progress. `None` on fresh spawn.
    pub shooting_timer: Option<TowerShootingTimer>,
    /// Turret angle in radians. 0 on fresh spawn.
    pub top_angle: f32,
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
        Self { grid_position, integrity_points: None, disabled_by_player: None, shooting_timer: None, top_angle: 0. }
    }
    pub fn with_integrity_points(mut self, integrity_points: f32) -> Self {
        self.integrity_points = Some(IntegrityPoints::new(integrity_points));
        self
    }
    pub fn with_disabled_by_player(mut self, disabled_by_player: bool) -> Self {
        self.disabled_by_player = disabled_by_player.then_some(DisabledByPlayer);
        self
    }
    pub fn with_shooting_elapsed(mut self, elapsed: f32) -> Self {
        self.shooting_timer = Some(TowerShootingTimer::from_elapsed(elapsed));
        self
    }
    pub fn with_top_angle(mut self, angle: f32) -> Self {
        self.top_angle = angle;
        self
    }

    pub fn on_builder_add_spawn_tower_flamethrower(
        trigger: On<Add<BuilderTowerFlamethrower>>,
        mut commands: Commands,
        almanach: Res<Almanach>,
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
            .insert_some(builder.shooting_timer.clone())
            .insert((
                TowerFlamethrower,
                Sprite {
                    image: building_info.sprite.clone(),
                    custom_size: Some(grid_imprint.world_size()),
                    ..default()
                },
                builder.grid_position,
                grid_imprint,
                TowerTopRotation { speed: 10.0, current_angle: builder.top_angle },
                NeedsPower,
                related![Indicators[
                    IndicatorType::NoPower,
                    IndicatorType::DisabledByPlayer,
                ]],
                related![EffectInstances[
                    (ModifierContributions(building_info.baseline.clone()), BaselineEffect),
                ]],
                related![GeneratedFlameStream[FlameStream::default()]],
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
    towers: Query<(Entity, &GridCoords, &IntegrityPoints, Has<DisabledByPlayer>, &TowerShootingTimer, &TowerTopRotation), With<TowerFlamethrower>>,
    mut save: SaveWriter,
) {
    if towers.is_empty() { return; }

    #[debug_dev("Saving {} tower flamethrowers", rows.len())]
    let rows: Vec<(u32, GridCoords, f32, bool, f32, f32)> = towers
        .iter()
        .map(|(entity, coords, integrity_points, disabled_by_player, shooting_timer, top_rotation)| {
            (
                entity.index_u32(),
                *coords,
                integrity_points.get_current(),
                disabled_by_player,
                shooting_timer.0.elapsed_secs(),
                top_rotation.current_angle,
            )
        })
        .collect();
    save.submit(move |ctx| {
        for (id, coords, integrity_points, disabled_by_player, shooting_elapsed, top_angle) in rows {
            ctx.save_marker("tower_flamethrowers", id)?;
            ctx.save_grid_coords(id, coords)?;
            ctx.save_integrity_points(id, integrity_points)?;
            ctx.save_shooting_elapsed(id, shooting_elapsed)?;
            ctx.save_top_angle(id, top_angle)?;
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
        let shooting_elapsed = ctx.shooting_elapsed(old_id)?;
        let top_angle = ctx.top_angle(old_id)?;
        let builder = BuilderTowerFlamethrower::new(grid_position)
            .with_integrity_points(integrity_points)
            .with_disabled_by_player(disabled_by_player)
            .with_shooting_elapsed(shooting_elapsed)
            .with_top_angle(top_angle);
        ctx.insert(entity, builder);
        Ok(())
    })
}

/// Turret alignment with the current target.
enum Aim {
    /// Absolute angle delta (radians) between turret facing and target direction.
    Target(f32),
    NoTarget,
    /// Target died or a search is queued; resolved on the next targeting pass.
    Pending,
}

/// Syncs stream nozzle, direction, and length from tower transform and range. Runs while paused so
/// placed or modified towers update immediately.
fn aim_flame_streams(
    mut streams: Query<&mut FlameStream>,
    towers: Query<(&Transform, &TowerTopRotation, &GridImprint, &AttackRange, &GeneratedFlameStream), With<TowerFlamethrower>>,
) {
    for (transform, top_rotation, grid_imprint, attack_range, generated_stream) in towers.iter() {
        let Ok(mut stream) = streams.get_mut(generated_stream.stream()) else { continue; };
        let direction = Vec2::from_angle(top_rotation.current_angle);
        let tower_width = grid_imprint.world_size().x;
        let nozzle_offset = tower_width * NOZZLE_OFFSET;
        stream.nozzle = transform.translation.xy() + direction * nozzle_offset;
        stream.direction = direction;
        stream.length = (attack_range.get() * CELL_SIZE + tower_width * 0.5 - nozzle_offset).max(CELL_SIZE);
    }
}

fn update_flame_stream_firing(
    mut streams: Query<&mut FlameStream>,
    mut towers: Query<(&Transform, &TowerTopRotation, &GeneratedFlameStream, &mut TowerWispTarget), With<TowerFlamethrower>>,
    wisps: Query<&Transform, With<Wisp>>,
) {
    for (transform, top_rotation, generated_stream, mut target) in towers.iter_mut() {
        let Ok(mut stream) = streams.get_mut(generated_stream.stream()) else { continue; };

        let aim = match *target {
            TowerWispTarget::Wisp(target_wisp) => match wisps.get(target_wisp) {
                Ok(wisp) => Aim::Target(top_rotation.angle_to(wisp.translation.xy() - transform.translation.xy()).abs()),
                Err(_) => Aim::Pending,
            },
            TowerWispTarget::SearchForNewTarget => Aim::Pending,
            TowerWispTarget::NoValidTargets(_) | TowerWispTarget::Suspended => Aim::NoTarget,
        };
        match aim {
            Aim::Target(angle_off) if stream.is_armed() && angle_off <= IGNITE_ANGLE => stream.ignite(),
            Aim::NoTarget if stream.is_firing() => {
                stream.cut_off();
                *target = TowerWispTarget::Suspended;
            }
            Aim::Target(angle_off) if stream.is_firing() && angle_off > CUTOFF_ANGLE => {
                stream.cut_off();
                *target = TowerWispTarget::Suspended;
            }
            _ => {}
        }
    }
}

/// Each shooting tick damages every wisp on the tile the stream hits.
fn damage_flame_hits(
    mut damage_messages: MessageWriter<DamageMessage>,
    wisps_grid: Res<WispsGrid>,
    streams: Query<&FlameStream>,
    mut towers: Query<(&GeneratedFlameStream, &AttackDamage, &mut TowerShootingTimer), (With<TowerFlamethrower>, With<IsOperational>)>,
) {
    for (generated_stream, attack_damage, mut timer) in towers.iter_mut() {
        let Ok(stream) = streams.get(generated_stream.stream()) else { continue; };
        let Some(hit) = &stream.hit else { continue; };
        if !timer.0.is_finished() { continue; }
        for &wisp in &wisps_grid[hit.coords] {
            damage_messages.write(DamageMessage { target: wisp, amount: attack_damage.get() });
        }
        timer.0.reset();
    }
}

fn on_add_is_operational_arm_flame_stream(
    trigger: On<Add<IsOperational>>,
    mut streams: Query<&mut FlameStream>,
    towers: Query<&GeneratedFlameStream, With<TowerFlamethrower>>,
) {
    let Ok(generated_stream) = towers.get(trigger.entity) else { return; };
    let Ok(mut stream) = streams.get_mut(generated_stream.stream()) else { return; };
    stream.arm();
}

/// Disarms the stream when no longer operational; suspends targeting if fuel is still in flight.
fn on_remove_is_operational_disarm_flame_stream(
    trigger: On<Remove<IsOperational>>,
    mut streams: Query<&mut FlameStream>,
    mut towers: Query<(&GeneratedFlameStream, &mut TowerWispTarget), With<TowerFlamethrower>>,
) {
    let Ok((generated_stream, mut target)) = towers.get_mut(trigger.entity) else { return; };
    let Ok(mut stream) = streams.get_mut(generated_stream.stream()) else { return; };
    if stream.is_firing() {
        *target = TowerWispTarget::Suspended;
    }
    stream.disarm();
}

fn on_flame_stream_burnt_out_resume_targeting(
    trigger: On<FlameStreamBurntOut>,
    mut towers: Query<&mut TowerWispTarget, With<TowerFlamethrower>>,
) {
    let Ok(mut target) = towers.get_mut(trigger.tower) else { return; };
    *target = TowerWispTarget::SearchForNewTarget;
}
