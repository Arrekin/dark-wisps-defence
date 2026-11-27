//! # Napalm Battery
//!
//! Fire evolution of the Rocket Launcher. Fires salvos of napalm bombs: the first targets the wisp's
//! predicted next cell, and the remaining bombs target distinct random cells within
//! [`SALVO_AREA_REACH`].
//!
//! All bombs in a salvo spawn at once; bombs after the first carry a staggered
//! [`ProjectileLaunchCounter`]. While queued bombs are waiting to launch, the battery's lob preview
//! displays the next queued bomb's arc rather than predictive aim.

use bevy::{
    prelude::*,
    sprite::Anchor,
};
use nanorand::Rng;

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
use session::GameClock;
use shards::prelude::*;
use states::prelude::*;
use weaponry::{lob::{LobArc, LobPreviewShown, LobProfile}, prelude::*};
use wisps::prelude::*;

use crate::{common::*, towers::lob_preview::TowerLobArcSystems};

pub(crate) struct TowerNapalmBatteryPlugin;
impl Plugin for TowerNapalmBatteryPlugin {
    fn build(&self, app: &mut App) {
        let almanach_info = BuilderTowerNapalmBattery::almanach_info(app.world().resource::<AssetServer>());
        app
            .add_systems(Update, (
                shoot_targets.after(TowerAimingSystems).run_if(in_state(GameState::Running)),
                sync_lob_arc_with_salvo_or_predictive_aiming.in_set(TowerLobArcSystems),
            ))
            .add_observer(BuilderTowerNapalmBattery::on_builder_add_spawn_tower_napalm_battery)
            .add_observer(on_tower_napalm_battery_place_request_do_so)
            .add_systems(CollectSave, collect_tower_napalm_batteries)
            .register_loader(MapLoadingStage::SpawnMapElements, "tower_napalm_batteries", load_tower_napalm_batteries)
            .register_building(BuildingType::Tower(TowerType::NapalmBattery), almanach_info);
    }
}

/// Flight parameters shared by all bombs.
const NAPALM_BATTERY_LOB: LobProfile = LobProfile {
    launch_angle: 60_f32.to_radians(),
    gravity: 1500.,
    max_apex: 8. * CELL_SIZE,
};

// Salvo
/// Bombs per salvo. Capped to the in-bounds cells of the salvo area.
const SALVO_SIZE: usize = 6;
/// Maximum offset from the aim cell along either grid axis, in cells.
const SALVO_AREA_REACH: i32 = 2;
const SALVO_AREA_SIDE: usize = (2 * SALVO_AREA_REACH + 1) as usize;
/// Candidate landing cells relative to the aim cell, the aim cell first.
const SALVO_AREA_OFFSETS: [(i32, i32); SALVO_AREA_SIDE * SALVO_AREA_SIDE] = salvo_area_offsets();
/// Seconds between the launches of consecutive bombs in a salvo.
const SALVO_LAUNCH_GAP: f32 = 0.1;

const fn salvo_area_offsets() -> [(i32, i32); SALVO_AREA_SIDE * SALVO_AREA_SIDE] {
    let mut offsets = [(0, 0); SALVO_AREA_SIDE * SALVO_AREA_SIDE];
    let mut index = 0;
    while index < offsets.len() {
        offsets[index] = ((index % SALVO_AREA_SIDE) as i32 - SALVO_AREA_REACH, (index / SALVO_AREA_SIDE) as i32 - SALVO_AREA_REACH);
        index += 1;
    }
    // Swap the aim cell, at the centre of the area, to the front.
    let centre = offsets.len() / 2;
    offsets[centre] = offsets[0];
    offsets[0] = (0, 0);
    offsets
}

#[derive(Component, SSS)]
pub(crate) struct BuilderTowerNapalmBattery {
    pub grid_position: GridPosition,
    /// Custom integrity points, or `None` to use the fresh-spawn baseline.
    pub integrity_points: Option<IntegrityPoints>,
    /// Set when the player disabled this building. `None` on fresh spawn.
    pub disabled_by_player: Option<DisabledByPlayer>,
    /// Saved reload progress. `None` on fresh spawn.
    pub shooting_timer: Option<TowerShootingTimer>,
    /// Turret angle in radians. 0 on fresh spawn.
    pub top_angle: f32,
}

impl BuilderTowerNapalmBattery {
    pub fn almanach_info(asset_server: &AssetServer) -> BuildingInfo {
        BuildingInfo {
            name: "Napalm Battery".to_string(),
            description: "Lobs salvos of napalm bombs that set the ground around the target ablaze.".to_string(),
            sprite: asset_server.load("buildings/tower_rocket_launcher.png"),
            top_sprite: Some(asset_server.load("buildings/tower_rocket_launcher_top.png")),
            grid_imprint: GridImprint::square(3),
            attack_range: AttackRangeType(RangeType::Cells).into(),
            cost: vec![ResourceAmount::new(ResourceType::DarkOre, 350)],
            baseline: ModifierContributions::from([
                (ModifierType::MaxIntegrityPoints, 100.),
                (ModifierType::MinAttackRange, 7.),
                (ModifierType::MaxAttackRange, 30.),
                (ModifierType::AttackSpeed, 0.33),
                (ModifierType::TurnSpeed, 1.),
            ]),
            sockets: vec![
                ("attack_speed".into(), ShardSocket::new(ShardType::Speed, "Attack speed", ModifierType::AttackSpeed, [0.1, 0.2, 0.3])),
                ("attack_range".into(), ShardSocket::new(ShardType::Reach, "Attack range", ModifierType::MaxAttackRange, [2., 4., 6.])),
                ("projectile_count".into(), ShardSocket::without_contributions(ShardType::Strength, "Projectile count (no effect yet)")),
            ],
        }
    }

    pub fn new(grid_position: GridPosition) -> Self { Self { grid_position, integrity_points: None, disabled_by_player: None, shooting_timer: None, top_angle: 0. } }
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

    pub fn on_builder_add_spawn_tower_napalm_battery(
        trigger: On<Add<BuilderTowerNapalmBattery>>,
        mut commands: Commands,
        almanach: Res<Almanach>,
        builders: Query<&BuilderTowerNapalmBattery>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return; };

        let building_info = almanach.get_building_info(BuildingType::Tower(TowerType::NapalmBattery));
        let area = GridArea::new(builder.grid_position, building_info.grid_imprint);
        let world_size = area.imprint.world_size();

        commands.entity(entity)
            .remove::<BuilderTowerNapalmBattery>()
            .insert_some(building_info.attack_range)
            .insert_some(builder.integrity_points)
            .insert_some(builder.disabled_by_player)
            .insert_some(builder.shooting_timer.clone())
            .insert((
                TowerNapalmBattery,
                NAPALM_BATTERY_LOB,
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
                related![TowerTop[(
                    Sprite {
                        image: building_info.top_sprite.clone().expect("Napalm Battery defines a top sprite"),
                        custom_size: Some(Vec2::new(world_size.x * 1.52 * 0.5, world_size.y * 0.5)),
                        ..default()
                    },
                    Anchor(Vec2::new(-0.20, 0.0)),
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

fn on_tower_napalm_battery_place_request_do_so(
    trigger: On<PlaceRequest>,
    mut rules: BuildingPlacementRules,
    placement_sessions: Query<(&PlacementObject, Option<&GridArea>), With<PlacementSession>>,
) {
    let Ok((&PlacementObject(MapObject::Building(building_type @ BuildingType::Tower(TowerType::NapalmBattery))), area)) = placement_sessions.get(trigger.placement_session) else { return };
    let outcome = area.ok_or(PlaceResult::InvalidSession).and_then(|&area| rules.place(building_type, area, BuilderTowerNapalmBattery::new));
    trigger.report(&mut rules.commands, outcome);
}

#[log_tags(Tag::GameSave)]
fn collect_tower_napalm_batteries(
    towers: Query<(Entity, &GridArea, &IntegrityPoints, Has<DisabledByPlayer>, &TowerShootingTimer, &TowerTopRotation), With<TowerNapalmBattery>>,
    mut save: SaveWriter,
) {
    if towers.is_empty() { return; }

    #[debug_dev("Saving {} tower napalm batteries", rows.len())]
    let rows: Vec<(u32, GridPosition, f32, bool, f32, f32)> = towers
        .iter()
        .map(|(entity, area, integrity_points, disabled_by_player, shooting_timer, top_rotation)| {
            (
                entity.index_u32(),
                area.origin,
                integrity_points.current,
                disabled_by_player,
                shooting_timer.0.elapsed_secs(),
                top_rotation.current_angle,
            )
        })
        .collect();
    save.submit(move |ctx| {
        for (id, grid_position, integrity_points, disabled_by_player, shooting_elapsed, top_angle) in rows {
            ctx.save_marker("tower_napalm_batteries", id)?;
            ctx.save_grid_position(id, grid_position)?;
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

fn load_tower_napalm_batteries(ctx: &mut LoadContext) -> LoadResult {
    ctx.for_each_entity("SELECT id FROM tower_napalm_batteries", |ctx, old_id, entity, _| {
        let grid_position = ctx.grid_position(old_id)?;
        let integrity_points = ctx.integrity_points(old_id)?;
        let disabled_by_player = ctx.disabled_by_player(old_id)?;
        let shooting_elapsed = ctx.shooting_elapsed(old_id)?;
        let top_angle = ctx.top_angle(old_id)?;
        let builder = BuilderTowerNapalmBattery::new(grid_position)
            .with_integrity_points(integrity_points)
            .with_disabled_by_player(disabled_by_player)
            .with_shooting_elapsed(shooting_elapsed)
            .with_top_angle(top_angle);
        ctx.insert(entity, builder);
        Ok(())
    })
}

fn shoot_targets(
    mut commands: Commands,
    clock: Res<GameClock>,
    map_info: Res<MapInfo>,
    predictive_aim: PredictiveAim,
    mut tower_napalm_batteries: Query<(Entity, &Transform, &LobProfile, &mut TowerShootingTimer, &TowerWispTarget, &TowerTopRotation), (With<TowerNapalmBattery>, With<IsOperational>)>,
    wisps: Query<&Transform, With<Wisp>>,
) {
    for (tower, transform, lob_profile, mut timer, target, top_rotation) in tower_napalm_batteries.iter_mut() {
        if !timer.0.is_finished() { continue; }
        let Some(wisp) = target.wisp() else { continue; };
        let (Ok(wisp_transform), Some(aim_coords)) = (wisps.get(wisp), predictive_aim.next_cell(wisp)) else { continue; };

        let tower_center = transform.translation.xy();
        if !top_rotation.is_facing(wisp_transform.translation.xy() - tower_center) { continue; }

        // The first bomb takes the aim cell. A partial Fisher-Yates shuffle selects distinct cells for
        // the rest without shuffling the entire area.
        let mut offsets = SALVO_AREA_OFFSETS;
        let mut rng = nanorand::tls_rng();
        let mut launched = 0;
        for index in 0..offsets.len() {
            if launched == SALVO_SIZE { break; }
            if index > 0 { offsets.swap(index, rng.generate_range(index..SALVO_AREA_OFFSETS.len())); }
            let landing_coords = aim_coords.shifted(offsets[index]);
            if !map_info.grid_bounds.contains(landing_coords) { continue; }

            let landing_point = landing_coords.world_center();
            let arc = lob_profile.arc(tower_center, landing_point);
            let launch_counter = (launched > 0).then(|| ProjectileLaunchCounter::new(clock.from_now(launched as f32 * SALVO_LAUNCH_GAP)));
            commands.spawn(BuilderNapalmBomb::new(arc).with_shooter(tower).with_launch_counter(launch_counter));
            launched += 1;
        }
        timer.0.reset();
    }
}

/// Updates [`LobArc`] on previewed batteries to the soonest-launching queued bomb in the active
/// salvo, or to the target wisp's predicted next cell when no salvo is mid-launch.
///
/// Removes [`LobArc`] when no bomb is queued and the battery is non-operational or has no targets;
/// retains the previous arc while [`TowerWispTarget::SearchForNewTarget`] is pending.
fn sync_lob_arc_with_salvo_or_predictive_aiming(
    mut commands: Commands,
    predictive_aim: PredictiveAim,
    batteries: Query<(Entity, &Transform, &LobProfile, &TowerWispTarget, Has<IsOperational>, Option<&FiredProjectiles>, Option<&LobArc>), (With<TowerNapalmBattery>, With<LobPreviewShown>)>,
    waiting_bombs: Query<(&LobArc, &ProjectileLaunchCounter)>,
) {
    for (battery, transform, lob_profile, target, is_operational, fired_projectiles, current_arc) in batteries.iter() {
        let soonest_bomb_arc = fired_projectiles.into_iter().flat_map(FiredProjectiles::iter)
            .filter_map(|bomb| waiting_bombs.get(bomb).ok())
            .min_by(|(_, a), (_, b)| a.launch_at.total_cmp(&b.launch_at))
            .map(|(arc, _)| *arc);
        let arc = match soonest_bomb_arc {
            Some(arc) => arc,
            None => {
                if !is_operational || target.has_no_targets() {
                    if current_arc.is_some() { commands.entity(battery).remove::<LobArc>(); }
                    continue;
                }
                let Some(aim_coords) = target.wisp().and_then(|wisp| predictive_aim.next_cell(wisp)) else { continue; };
                lob_profile.arc(transform.translation.xy(), aim_coords.world_center())
            }
        };
        if current_arc != Some(&arc) { commands.entity(battery).insert(arc); }
    }
}
