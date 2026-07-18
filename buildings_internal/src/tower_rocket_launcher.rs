use bevy::{
    platform::collections::HashMap,
    prelude::*,
    sprite::Anchor,
};

use alteration::{
    effects::prelude::*,
    modifiers::prelude::*,
};
use almanach::prelude::*;
use buildings::prelude::*;
use game_core::{math::angle_difference, prelude::*};
use grids::placement::{annotate_non_empty, PlacementMode};
use hud::prelude::{IndicatorDisplay, IndicatorType, Indicators};
use logging::prelude::*;
use persistence::{
    prelude::{AppGameLoadSaveExtension, CollectSave, GameDbHelpers, LoadContext, SaveWriter},
    rusqlite,
};
use resources::prelude::*;
use shards::prelude::*;
use states::prelude::*;
use weaponry::prelude::*;
use wisps::prelude::*;

use crate::common::*;

pub struct TowerRocketLauncherPlugin;
impl Plugin for TowerRocketLauncherPlugin {
    fn build(&self, app: &mut App) {
        let almanach_info = BuilderTowerRocketLauncher::almanach_info(app.world().resource::<AssetServer>());
        app
            .add_observer(BuilderTowerRocketLauncher::on_builder_add_spawn_tower_rocket_launcher)
            .add_systems(Update, (
                shooting_system.run_if(in_state(GameState::Running)),
            ))
            .add_systems(CollectSave, collect_tower_rocket_launchers)
            .register_loader(MapLoadingStage::SpawnMapElements, "tower_rocket_launchers", load_tower_rocket_launchers)
            .register_building(BuildingType::Tower(TowerType::RocketLauncher), almanach_info)
            ;
    }
}

#[derive(Component, SSS)]
pub(crate) struct BuilderTowerRocketLauncher {
    pub grid_position: GridCoords,
    /// Saved integrity points. `None` ⇒ defer to baseline (fresh spawn);
    /// `Some` ⇒ override with saved value (restore).
    pub integrity_points: Option<f32>,
    /// Whether the player disabled this building. False on fresh spawn.
    pub disabled_by_player: bool,
}

impl BuilderTowerRocketLauncher {
    pub fn almanach_info(asset_server: &AssetServer) -> BuildingInfo {
        BuildingInfo {
            name: "Rocket Launcher Tower".to_string(),
            sprite: asset_server.load("buildings/tower_rocket_launcher.png"),
            top_sprite: Some(asset_server.load("buildings/tower_rocket_launcher_top.png")),
            grid_imprint: GridImprint::Rectangle { width: 3, height: 3 },
            cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 350 }],
            baseline: HashMap::from([
                (ModifierType::MaxIntegrityPoints, 100.),
                (ModifierType::AttackRange, 30.),
                (ModifierType::AttackSpeed, 0.33),
                (ModifierType::AttackDamage, 50.),
            ]),
            validate: building_validator,
            annotate: annotate_non_empty,
            place_emitter: building_place_emitter,
            remove_emitter: None,
            begin_placing_emitter: None,
            place_mode: PlacementMode::OnRelease,
            remove_mode: PlacementMode::OnRelease,
        }
    }

    pub fn new(grid_position: GridCoords) -> Self { Self { grid_position, integrity_points: None, disabled_by_player: false } }
    pub fn with_integrity_points(mut self, integrity_points: f32) -> Self {
        self.integrity_points = Some(integrity_points);
        self
    }
    pub fn with_disabled_by_player(mut self) -> Self {
        self.disabled_by_player = true;
        self
    }

    pub fn on_builder_add_spawn_tower_rocket_launcher(
        trigger: On<Add, BuilderTowerRocketLauncher>,
        mut commands: Commands,
        almanach: Res<Almanach>,
        builders: Query<&BuilderTowerRocketLauncher>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return; };

        let building_info = almanach.get_building_info(BuildingType::Tower(TowerType::RocketLauncher));
        let grid_imprint = building_info.grid_imprint;

        let mut entity_commands = commands.entity(entity);
        if let Some(ip) = builder.integrity_points {
            entity_commands.insert(IntegrityPoints::new(ip));
        }
        if builder.disabled_by_player {
            entity_commands.insert(DisabledByPlayer);
        }

        let tower_base_entity = entity_commands
            .remove::<BuilderTowerRocketLauncher>()
            .insert((
                TowerRocketLauncher,
                Sprite {
                    image: building_info.sprite.clone(),
                    custom_size: Some(grid_imprint.world_size()),
                    ..Default::default()
                },
                builder.grid_position,
                grid_imprint,
                TowerTopRotation { speed: 1.0, current_angle: 0. },
                NeedsPower::default(),
                ShardSlots::new(3),
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
            .observe(Self::on_shard_apply_do_so)
            .observe(on_technical_state_changed_recompute_operational)
            .id();
        let world_size = grid_imprint.world_size();
        let tower_top = commands.spawn((
            Sprite {
                image: building_info.top_sprite.clone().unwrap(),
                custom_size: Some(Vec2::new(world_size.x * 1.52 * 0.5, world_size.y * 0.5)),
                ..Default::default()
            },
            Anchor(Vec2::new(-0.20, 0.0)),
            ZDepth(Z_TOWER_TOP),
            MarkerTowerRotationalTop(tower_base_entity),
        )).id();
        commands.entity(entity).add_child(tower_top);
        commands.trigger(TechnicalStateChanged { entity, kind: TechnicalChange::JustSpawned });
    }

    fn on_shard_apply_do_so(
        trigger: On<ShardApplyEvent>,
        mut commands: Commands,
    ) {
        match trigger.shard_type {
            ShardType::Range => {
                commands.spawn(ShardEffect::from_modifiers(trigger.shard_target, HashMap::from([(ModifierType::AttackRange, 2.0)])));
            }
            ShardType::Damage => {
                commands.spawn(ShardEffect::from_modifiers(trigger.shard_target, HashMap::from([(ModifierType::AttackDamage, 15.0)])));
            }
            ShardType::Speed => {
                commands.spawn(ShardEffect::from_modifiers(trigger.shard_target, HashMap::from([(ModifierType::AttackSpeed, 0.1)])));
            }
            ShardType::Fire | ShardType::Water | ShardType::Light | ShardType::Electric => {}
        }
    }
}

fn collect_tower_rocket_launchers(
    towers: Query<(Entity, &GridCoords, &IntegrityPoints, Has<DisabledByPlayer>), With<TowerRocketLauncher>>,
    mut save: SaveWriter,
) {
    if towers.is_empty() { return; }
    let rows: Vec<(i64, i32, i32, f32, bool)> = towers
        .iter()
        .map(|(entity, coords, integrity_points, disabled_by_player)| {
            (
                entity.index_u32() as i64,
                coords.x,
                coords.y,
                integrity_points.get_current(),
                disabled_by_player,
            )
        })
        .collect();
    Log::debug().dev().tag(Tag::GameSave).message(format!("Saving {} tower rocket launchers", rows.len()));
    save.submit(move |tx| {
        for (id, gx, gy, integrity_points, disabled_by_player) in rows {
            tx.save_marker("tower_rocket_launchers", id)?;
            tx.save_grid_coords(id, GridCoords { x: gx, y: gy })?;
            tx.save_integrity_points(id, integrity_points)?;
            if disabled_by_player {
                tx.save_disabled_by_player(id)?;
            }
        }
        Ok(())
    });
}

fn load_tower_rocket_launchers(ctx: &mut LoadContext) -> rusqlite::Result<()> {
    let mut stmt = ctx.conn.prepare("SELECT id FROM tower_rocket_launchers")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let old_id: i64 = row.get(0)?;
        let grid_position = ctx.conn.get_grid_coords(old_id)?;
        let integrity_points = ctx.conn.get_integrity_points(old_id)?;
        let disabled_by_player = ctx.conn.get_disabled_by_player(old_id)?;

        let Some(entity) = ctx.entity(old_id) else {
            Log::warn().dev().tag(Tag::GameLoad).message(format!("TowerRocketLauncher with old ID {old_id} has no corresponding new entity"));
            continue;
        };
        let mut builder = BuilderTowerRocketLauncher::new(grid_position)
            .with_integrity_points(integrity_points);
        if disabled_by_player {
            builder = builder.with_disabled_by_player();
        }
        ctx.insert(entity, builder);
    }
    Ok(())
}

fn shooting_system(
    mut commands: Commands,
    mut tower_rocket_launchers: Query<(&GridImprint, &Transform, &mut TowerShootingTimer, &mut TowerWispTarget, &TowerTopRotation, &AttackDamage), (With<TowerRocketLauncher>, With<IsOperational>)>,
    wisps: Query<&Transform, With<Wisp>>,
) {
    for (grid_imprint, transform, mut timer, mut target, top_rotation, attack_damage) in tower_rocket_launchers.iter_mut() {
        let TowerWispTarget::Wisp(target_wisp) = *target else { continue; };
        if !timer.0.is_finished() { continue; }

        let Ok(wisp_position) = wisps.get(target_wisp).map(|target| target.translation.xy()) else {
            // Target wisp does not exist anymore
            *target = TowerWispTarget::SearchForNewTarget;
            continue;
        };

        // Check if the tower top is facing the target
        let direction_to_target = wisp_position - transform.translation.xy();
        let target_angle = direction_to_target.y.atan2(direction_to_target.x);
        if angle_difference(target_angle, top_rotation.current_angle).abs() > std::f32::consts::PI / 72. { continue; }

        // Calculate transform offset in the direction we are aiming
        let tower_world_width = grid_imprint.world_size().x;
        let offset = Vec2::new(
            top_rotation.current_angle.cos() * tower_world_width * 0.4,
            top_rotation.current_angle.sin() * tower_world_width * 0.4,
        );
        let spawn_position = transform.translation.xy() + offset;

        let rocket_angle = Quat::from_rotation_z(top_rotation.current_angle);
        commands.spawn(BuilderRocket::new(spawn_position, rocket_angle, target_wisp, attack_damage.clone()));
        timer.0.reset();
    }
}
