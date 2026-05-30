use bevy::sprite::Anchor;

use lib_core::utils::angle_difference;

use crate::prelude::*;
use crate::ui::indicators::{IndicatorDisplay, IndicatorType, Indicators};
use crate::weaponry::rocket::BuilderRocket;
use crate::wisps::components::Wisp;

pub struct TowerRocketLauncherPlugin;
impl Plugin for TowerRocketLauncherPlugin {
    fn build(&self, app: &mut App) {
        let almanach_info = BuilderTowerRocketLauncher::almanach_info(app.world().resource::<AssetServer>());
        app
            .add_observer(BuilderTowerRocketLauncher::on_add)
            .add_systems(Update, (
                shooting_system.run_if(in_state(GameState::Running)),
            ))
            .register_db_loader::<BuilderTowerRocketLauncher>(MapLoadingStage::SpawnMapElements)
            .register_db_saver(BuilderTowerRocketLauncher::on_game_save)
            .register_building(BuildingType::Tower(TowerType::RocketLauncher), almanach_info)
            ;
    }
}

#[derive(Clone, Debug)]
pub struct TowerRocketLauncherSaveData {
    entity: Entity,
    integrity_points: f32,
    disabled_by_player: bool,
}

#[derive(Component, SSS)]
pub struct BuilderTowerRocketLauncher {
    grid_position: GridCoords,
    save_data: Option<TowerRocketLauncherSaveData>,
}

impl Saveable for BuilderTowerRocketLauncher {
    fn save(self, tx: &rusqlite::Transaction) -> rusqlite::Result<()> {
        let save_data = self.save_data.expect("BuilderTowerRocketLauncher for saving must have save_data");
        let entity_index = save_data.entity.index_u32() as i64;

        tx.save_marker("tower_rocket_launchers", entity_index)?;
        tx.save_grid_coords(entity_index, self.grid_position)?;
        tx.save_integrity_points(entity_index, save_data.integrity_points)?;
        if save_data.disabled_by_player {
            tx.save_disabled_by_player(entity_index)?;
        }

        Ok(())
    }
}

impl Loadable for BuilderTowerRocketLauncher {
    fn load(ctx: &mut LoadContext) -> rusqlite::Result<LoadResult> {
        let mut stmt = ctx.conn.prepare("SELECT id FROM tower_rocket_launchers LIMIT ?1 OFFSET ?2")?;
        let mut rows = stmt.query(ctx.pagination.as_params())?;

        let mut count = 0;
        while let Some(row) = rows.next()? {
            let old_id: i64 = row.get(0)?;
            let grid_position = ctx.conn.get_grid_coords(old_id)?;
            let integrity_points = ctx.conn.get_integrity_points(old_id)?;
            let disabled_by_player = ctx.conn.get_disabled_by_player(old_id)?;

            if let Some(new_entity) = ctx.get_new_entity_for_old(old_id) {
                let save_data = TowerRocketLauncherSaveData { entity: new_entity, integrity_points, disabled_by_player };
                ctx.commands.entity(new_entity).insert(BuilderTowerRocketLauncher::new_for_saving(grid_position, save_data));
            }
            count += 1;
        }

        Ok(count.into())
    }
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
        }
    }

    pub fn new(grid_position: GridCoords) -> Self { Self { grid_position, save_data: None } }
    pub fn new_for_saving(grid_position: GridCoords, save_data: TowerRocketLauncherSaveData) -> Self {
        Self { grid_position, save_data: Some(save_data) }
    }

    fn on_game_save(
        mut commands: Commands,
        towers: Query<(Entity, &GridCoords, &IntegrityPoints, Has<DisabledByPlayer>), With<TowerRocketLauncher>>,
    ) {
        if towers.is_empty() { return; }
        let batch = towers.iter().map(|(entity, coords, integrity_points, disabled_by_player)| {
            let save_data = TowerRocketLauncherSaveData {
                entity,
                integrity_points: integrity_points.get_current(),
                disabled_by_player,
            };
            BuilderTowerRocketLauncher::new_for_saving(*coords, save_data)
        }).collect::<SaveableBatchCommand<_>>();
        commands.queue(batch);
    }

    pub fn on_add(
        trigger: On<Add, BuilderTowerRocketLauncher>,
        mut commands: Commands,
        builders: Query<&BuilderTowerRocketLauncher>,
        almanach: Res<Almanach>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return; };

        let building_info = almanach.get_building_info(BuildingType::Tower(TowerType::RocketLauncher));
        let grid_imprint = building_info.grid_imprint;

        let mut entity_commands = commands.entity(entity);
        if let Some(save_data) = &builder.save_data {
            entity_commands.insert(IntegrityPoints::new(save_data.integrity_points));
            if save_data.disabled_by_player {
                entity_commands.insert(DisabledByPlayer);
            }
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
            .observe(Self::on_shard_apply)
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
    }

    fn on_shard_apply(
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
        }
    }
}

pub fn shooting_system(
    mut commands: Commands,
    mut tower_rocket_launchers: Query<(&GridImprint, &Transform, &mut TowerShootingTimer, &mut TowerWispTarget, &TowerTopRotation, &AttackDamage), (With<TowerRocketLauncher>, With<HasPower>, Without<DisabledByPlayer>)>,
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
