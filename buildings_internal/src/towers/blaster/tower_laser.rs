//! # Laser Tower
//!
//! Light evolution of the Blaster. Fires a continuous `LaserBeam` (linked via `GeneratedLaserBeam`)
//! locked onto the tower's target. The beam deals this tower's `AttackDamage` per second to that
//! target only. The tower has no attack speed.
//!
//! ## Firing
//!
//! The beam locks on once the turret faces the target. While locked, the turret keeps tracking the
//! target but no longer needs to face it. The beam switches off when `TowerWispTarget` stops naming
//! the locked wisp (it died or became ineligible) or the tower loses `IsOperational`.

use bevy::prelude::*;

use almanach::{BuildingInfo, prelude::*};
use alteration::{
    effects::prelude::*,
    modifiers::prelude::*,
};
use buildings::prelude::*;
use game_core::prelude::*;
use grids::placement::{PlaceRequest, PlaceResult, PlacementObject, PlacementSession};
use hud::prelude::{IndicatorDisplay, IndicatorType, Indicators};
use logging::prelude::*;
use persistence::prelude::*;
use resources::prelude::*;
use shards::prelude::*;
use states::prelude::*;
use weaponry::{
    laser_beam::{GeneratedLaserBeam, LaserBeamSystems},
    prelude::*,
};
use wisps::prelude::*;

use crate::common::*;

pub(crate) struct TowerLaserPlugin;
impl Plugin for TowerLaserPlugin {
    fn build(&self, app: &mut App) {
        let almanach_info = BuilderTowerLaser::almanach_info(app.world().resource::<AssetServer>());
        app
            .add_systems(Update, (
                aim_laser_beams,
                update_laser_beam_firing.run_if(in_state(GameState::Running)),
            ).after(TowerAimingSystems).before(LaserBeamSystems))
            .add_observer(BuilderTowerLaser::on_builder_add_spawn_tower_laser)
            .add_observer(on_tower_laser_place_request_do_so)
            .add_observer(on_remove_is_operational_switch_off_laser_beam)
            .add_systems(CollectSave, collect_tower_lasers)
            .register_loader(MapLoadingStage::SpawnMapElements, "tower_lasers", load_tower_lasers)
            .register_building(BuildingType::Tower(TowerType::Laser), almanach_info);
    }
}

/// Distance from the tower center to the muzzle, relative to the tower's world width.
const MUZZLE_OFFSET: f32 = 0.4;

#[derive(Component, SSS)]
pub(crate) struct BuilderTowerLaser {
    pub grid_position: GridPosition,
    /// Custom integrity points, or `None` to use the fresh-spawn baseline.
    pub integrity_points: Option<IntegrityPoints>,
    /// Set when the player disabled this building. `None` on fresh spawn.
    pub disabled_by_player: Option<DisabledByPlayer>,
    /// Turret angle in radians. 0 on fresh spawn.
    pub top_angle: f32,
}
impl BuilderTowerLaser {
    pub fn almanach_info(asset_server: &AssetServer) -> BuildingInfo {
        BuildingInfo {
            name: "Laser Tower".to_string(),
            description: "A continuous beam of light locked onto its target. It arrives instantly and never misses.".to_string(),
            sprite: asset_server.load("buildings/tower_blaster.png"),
            top_sprite: Some(asset_server.load("buildings/tower_blaster_top.png")),
            grid_imprint: GridImprint::square(2),
            attack_range: AttackRangeType(RangeType::Cells).into(),
            cost: vec![ResourceAmount::new(ResourceType::DarkOre, 150)],
            baseline: ModifierContributions::from([
                (ModifierType::MaxIntegrityPoints, 100.),
                (ModifierType::MaxAttackRange, 10.),
                (ModifierType::AttackDamage, 2.),
                (ModifierType::TurnSpeed, 2.),
            ]),
            sockets: vec![
                ("attack_speed".into(), ShardSocket::without_contributions(ShardType::Speed, "Attack speed (no effect yet)")),
                ("attack_range".into(), ShardSocket::new(ShardType::Reach, "Attack range", ModifierType::MaxAttackRange, [1., 2., 3.])),
                ("damage".into(), ShardSocket::new(ShardType::Strength, "Damage", ModifierType::AttackDamage, [1., 2., 3.])),
            ],
        }
    }

    pub fn new(grid_position: GridPosition) -> Self {
        Self { grid_position, integrity_points: None, disabled_by_player: None, top_angle: 0. }
    }
    pub fn with_integrity_points(mut self, integrity_points: f32) -> Self {
        self.integrity_points = Some(IntegrityPoints::new(integrity_points));
        self
    }
    pub fn with_disabled_by_player(mut self, disabled_by_player: bool) -> Self {
        self.disabled_by_player = disabled_by_player.then_some(DisabledByPlayer);
        self
    }
    pub fn with_top_angle(mut self, angle: f32) -> Self {
        self.top_angle = angle;
        self
    }

    pub fn on_builder_add_spawn_tower_laser(
        trigger: On<Add<BuilderTowerLaser>>,
        mut commands: Commands,
        almanach: Res<Almanach>,
        builders: Query<&BuilderTowerLaser>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return; };

        let building_info = almanach.get_building_info(BuildingType::Tower(TowerType::Laser));
        let area = GridArea::new(builder.grid_position, building_info.grid_imprint);
        let world_size = area.imprint.world_size();

        commands.entity(entity)
            .remove::<BuilderTowerLaser>()
            .insert_some(building_info.attack_range)
            .insert_some(builder.integrity_points)
            .insert_some(builder.disabled_by_player)
            .insert((
                TowerLaser,
                building_info.world_sprite(),
                area,
                TowerTopRotation { current_angle: builder.top_angle },
                NeedsPower,
                related![Indicators[
                    IndicatorType::NoPower,
                    IndicatorType::DisabledByPlayer,
                ]],
                related![EffectInstances[
                    (building_info.baseline.clone(), BaselineEffect),
                ]],
                related![GeneratedLaserBeam[LaserBeam::default()]],
                related![TowerTop[(
                    Sprite {
                        image: building_info.top_sprite.clone().expect("Laser Tower defines a top sprite"),
                        custom_size: Some(Vec2::new(world_size.x * 1.52 * 0.5, world_size.y * 0.5)),
                        ..default()
                    },
                    Transform::from_translation(area.world_center().extend(0.)),
                )]],
                children![
                    IndicatorDisplay::default(),
                ],
            ))
            .observe(on_technical_state_changed_recompute_operational);
        for (content_id, socket) in &building_info.sockets {
            commands.trigger(ShardSocketUpsert::new(entity, content_id.clone(), socket.clone()));
        }
        commands.trigger(TechnicalStateChanged { entity, kind: TechnicalChange::JustSpawned });
    }
}

fn on_tower_laser_place_request_do_so(
    trigger: On<PlaceRequest>,
    mut rules: BuildingPlacementRules,
    placement_sessions: Query<(&PlacementObject, Option<&GridArea>), With<PlacementSession>>,
) {
    let Ok((&PlacementObject(MapObject::Building(building_type @ BuildingType::Tower(TowerType::Laser))), area)) = placement_sessions.get(trigger.placement_session) else { return };
    let outcome = area.ok_or(PlaceResult::InvalidSession).and_then(|&area| rules.place(building_type, area, BuilderTowerLaser::new));
    trigger.report(&mut rules.commands, outcome);
}

#[log_tags(Tag::GameSave)]
fn collect_tower_lasers(
    towers: Query<(Entity, &GridArea, &IntegrityPoints, Has<DisabledByPlayer>, &TowerTopRotation), With<TowerLaser>>,
    mut save: SaveWriter,
) {
    if towers.is_empty() { return; }

    #[debug_dev("Saving {} tower lasers", rows.len())]
    let rows: Vec<(u32, GridPosition, f32, bool, f32)> = towers
        .iter()
        .map(|(entity, area, integrity_points, disabled_by_player, top_rotation)| {
            (
                entity.index_u32(),
                area.origin,
                integrity_points.current,
                disabled_by_player,
                top_rotation.current_angle,
            )
        })
        .collect();
    save.submit(move |ctx| {
        for (id, grid_position, integrity_points, disabled_by_player, top_angle) in rows {
            ctx.save_marker("tower_lasers", id)?;
            ctx.save_grid_position(id, grid_position)?;
            ctx.save_integrity_points(id, integrity_points)?;
            ctx.save_top_angle(id, top_angle)?;
            if disabled_by_player {
                ctx.save_disabled_by_player(id)?;
            }
        }
        Ok(())
    });
}

fn load_tower_lasers(ctx: &mut LoadContext) -> LoadResult {
    ctx.for_each_entity("SELECT id FROM tower_lasers", |ctx, old_id, entity, _| {
        let grid_position = ctx.grid_position(old_id)?;
        let integrity_points = ctx.integrity_points(old_id)?;
        let disabled_by_player = ctx.disabled_by_player(old_id)?;
        let top_angle = ctx.top_angle(old_id)?;
        let builder = BuilderTowerLaser::new(grid_position)
            .with_integrity_points(integrity_points)
            .with_disabled_by_player(disabled_by_player)
            .with_top_angle(top_angle);
        ctx.insert(entity, builder);
        Ok(())
    })
}

/// Syncs beam muzzle and direction from the turret. Runs while paused so placed or loaded towers
/// line up immediately.
fn aim_laser_beams(
    mut beams: Query<&mut LaserBeam>,
    towers: Query<(&Transform, &TowerTopRotation, &GridArea, &GeneratedLaserBeam), With<TowerLaser>>,
) {
    for (transform, top_rotation, area, generated_beam) in towers.iter() {
        let Ok(mut beam) = beams.get_mut(generated_beam.beam()) else { continue; };
        let direction = Vec2::from_angle(top_rotation.current_angle);
        beam.muzzle = transform.translation.xy() + direction * area.imprint.world_size().x * MUZZLE_OFFSET;
        beam.direction = direction;
    }
}

/// Switches a beam off once the tower's target is no longer the locked wisp, and locks it onto the
/// target once the turret faces it.
fn update_laser_beam_firing(
    mut beams: Query<&mut LaserBeam>,
    towers: Query<(&Transform, &TowerTopRotation, &TowerWispTarget, &GeneratedLaserBeam), (With<TowerLaser>, With<IsOperational>)>,
    wisps: Query<&Transform, With<Wisp>>,
) {
    for (transform, top_rotation, target, generated_beam) in towers.iter() {
        let Ok(mut beam) = beams.get_mut(generated_beam.beam()) else { continue; };
        if beam.target().is_some_and(|locked| target.wisp() != Some(locked)) {
            beam.switch_off();
        }
        if beam.target().is_none()
            && let Some(wisp) = target.wisp()
            && let Ok(wisp_transform) = wisps.get(wisp)
            && top_rotation.is_facing(wisp_transform.translation.xy() - transform.translation.xy())
        {
            beam.lock_on(wisp);
        }
    }
}

fn on_remove_is_operational_switch_off_laser_beam(
    trigger: On<Remove<IsOperational>>,
    mut beams: Query<&mut LaserBeam>,
    towers: Query<&GeneratedLaserBeam, With<TowerLaser>>,
) {
    let Ok(generated_beam) = towers.get(trigger.entity) else { return; };
    let Ok(mut beam) = beams.get_mut(generated_beam.beam()) else { return; };
    beam.switch_off();
}
