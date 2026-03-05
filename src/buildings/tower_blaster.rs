use lib_core::utils::angle_difference;

use crate::prelude::*;
use crate::ui::indicators::{IndicatorDisplay, IndicatorType, Indicators};
use crate::weaponry::laser_dart::BuilderLaserDart;
use crate::wisps::components::Wisp;

pub struct TowerBlasterPlugin;
impl Plugin for TowerBlasterPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_observer(BuilderTowerBlaster::on_add)
            .add_systems(Update, (
                shooting_system.run_if(in_state(GameState::Running)),
            ))
            .register_db_loader::<BuilderTowerBlaster>(MapLoadingStage::SpawnMapElements)
            .register_db_saver(BuilderTowerBlaster::on_game_save)
            .register_building(BuildingType::Tower(TowerType::Blaster), BuilderTowerBlaster::almanach_info());
    }
}

pub const TOWER_BLASTER_BASE_IMAGE: &str = "buildings/tower_blaster.png";
pub const TOWER_BLASTER_TOP_IMAGE: &str = "buildings/tower_blaster_top.png";

#[derive(Clone, Debug)]
pub struct TowerBlasterSaveData {
    entity: Entity,
    health: f32,
    disabled_by_player: bool,
    upgrade_levels: HashMap<UpgradeType, usize>,
}

#[derive(Component, SSS)]
pub struct BuilderTowerBlaster {
    grid_position: GridCoords,
    save_data: Option<TowerBlasterSaveData>,
}
impl Saveable for BuilderTowerBlaster {
    fn save(self, tx: &rusqlite::Transaction) -> rusqlite::Result<()> {
        let save_data = self.save_data.expect("BuilderTowerBlaster for saving must have save_data");
        let entity_index = save_data.entity.index_u32() as i64;

        tx.save_marker("tower_blasters", entity_index)?;
        tx.save_grid_coords(entity_index, self.grid_position)?;
        tx.save_health(entity_index, save_data.health)?;
        if save_data.disabled_by_player {
            tx.save_disabled_by_player(entity_index)?;
        }
        for (upgrade_type, level) in &save_data.upgrade_levels {
            tx.save_upgrade_level(entity_index, &upgrade_type.as_db_str(), *level)?;
        }
        Ok(())
    }
}

impl Loadable for BuilderTowerBlaster {
    fn load(ctx: &mut LoadContext) -> rusqlite::Result<LoadResult> {
        let mut stmt = ctx.conn.prepare("SELECT id FROM tower_blasters LIMIT ?1 OFFSET ?2")?;
        let mut rows = stmt.query(ctx.pagination.as_params())?;
        
        let mut count = 0;
        while let Some(row) = rows.next()? {
            let old_id: i64 = row.get(0)?;
            let grid_position = ctx.conn.get_grid_coords(old_id)?;
            let health = ctx.conn.get_health(old_id)?;
            let disabled_by_player = ctx.conn.get_disabled_by_player(old_id)?;
            let upgrade_levels_raw = ctx.conn.get_upgrade_levels_raw(old_id)?;
            let upgrade_levels: HashMap<UpgradeType, usize> = upgrade_levels_raw
                .into_iter()
                .filter_map(|(type_str, level)| {
                    UpgradeType::from_db_str(&type_str).map(|t| (t, level))
                })
                .collect();
            
            if let Some(new_entity) = ctx.get_new_entity_for_old(old_id) {
                let save_data = TowerBlasterSaveData { entity: new_entity, health, disabled_by_player, upgrade_levels };
                ctx.commands.entity(new_entity).insert(BuilderTowerBlaster::new_for_saving(grid_position, save_data));
            }
            count += 1;
        }

        Ok(count.into())
    }
}

impl BuilderTowerBlaster {
    pub fn almanach_info() -> BuildingInfo {
        BuildingInfo {
            name: "Blaster Tower".to_string(),
            grid_imprint: GridImprint::Rectangle { width: 2, height: 2 },
            cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 150 }],
            baseline: HashMap::from([
                (ModifierType::MaxHealth, 100.),
                (ModifierType::AttackRange, 15.),
                (ModifierType::AttackSpeed, 5.),
                (ModifierType::AttackDamage, 1.),
            ]),
            upgrades: HashMap::from([
                (UpgradeType::Modifier(ModifierType::AttackSpeed), UpgradeInfo {
                    levels: vec![
                        UpgradeLevelInfo { value: 0.1, cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 100 }] },
                        UpgradeLevelInfo { value: 0.1, cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 200 }] },
                        UpgradeLevelInfo { value: 0.1, cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 300 }] },
                    ],
                }),
                (UpgradeType::Modifier(ModifierType::AttackDamage), UpgradeInfo {
                    levels: vec![
                        UpgradeLevelInfo { value: 1., cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 100 }] },
                        UpgradeLevelInfo { value: 1., cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 200 }] },
                        UpgradeLevelInfo { value: 1., cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 300 }] },
                    ],
                }),
            ]),
            validate: building_validator,
        }
    }

    pub fn new(grid_position: GridCoords) -> Self {
        Self { grid_position, save_data: None }
    }
    pub fn new_for_saving(grid_position: GridCoords, save_data: TowerBlasterSaveData) -> Self {
        Self { grid_position, save_data: Some(save_data) }
    }

    fn on_game_save(
        mut commands: Commands,
        towers: Query<(Entity, &GridCoords, &Health, Has<DisabledByPlayer>, &Upgrades), With<TowerBlaster>>,
    ) {
        if towers.is_empty() { return; }
        let batch = towers.iter().map(|(entity, coords, health, disabled_by_player, upgrades)| {
            let save_data = TowerBlasterSaveData {
                entity,
                health: health.get_current(),
                disabled_by_player,
                upgrade_levels: upgrades.get_levels(),
            };
            BuilderTowerBlaster::new_for_saving(*coords, save_data)
        }).collect::<SaveableBatchCommand<_>>();
        commands.queue(batch);
    }

    pub fn on_add(
        trigger: On<Add, BuilderTowerBlaster>,
        mut commands: Commands,
        builders: Query<&BuilderTowerBlaster>,
        asset_server: Res<AssetServer>,
        almanach: Res<Almanach>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return; };
        
        let building_info = almanach.get_building_info(BuildingType::Tower(TowerType::Blaster));
        let grid_imprint = building_info.grid_imprint;

        let mut entity_commands = commands.entity(entity);
        if let Some(save_data) = &builder.save_data {
            entity_commands.insert(Health::new(save_data.health));
            if save_data.disabled_by_player {
                entity_commands.insert(DisabledByPlayer);
            }
        }

        let tower_base_entity = entity_commands
            .remove::<BuilderTowerBlaster>()
            .insert((
                TowerBlaster,
                Sprite {
                    image: asset_server.load(TOWER_BLASTER_BASE_IMAGE),
                    custom_size: Some(grid_imprint.world_size()),
                    ..Default::default()
                },
                Tower,
                builder.grid_position,
                grid_imprint,
                TowerTopRotation { speed: 10.0, current_angle: 0. },
                NeedsPower::default(),
                Upgrades::from_almanach(&building_info.upgrades, builder.save_data.as_ref().map(|d| &d.upgrade_levels)),
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
            )).id();
        let world_size = grid_imprint.world_size();
        let tower_top = commands.spawn((
            Sprite {
                image: asset_server.load(TOWER_BLASTER_TOP_IMAGE),
                custom_size: Some(Vec2::new(world_size.x * 1.52 * 0.5, world_size.y * 0.5)),
                ..Default::default()
            },
            ZDepth(Z_TOWER_TOP),
            MarkerTowerRotationalTop(tower_base_entity),
        )).id();
        commands.entity(entity).add_child(tower_top);
    }
}

pub fn shooting_system(
    mut commands: Commands,
    mut tower_blasters: Query<(&GridImprint, &Transform, &mut TowerShootingTimer, &mut TowerWispTarget, &TowerTopRotation, &AttackDamage), (With<TowerBlaster>, With<HasPower>, Without<DisabledByPlayer>)>,
    wisps: Query<&Transform, With<Wisp>>,
) {
    for (grid_imprint, transform, mut timer, mut target, top_rotation, attack_damage) in tower_blasters.iter_mut() {
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
        if angle_difference(target_angle, top_rotation.current_angle).abs() > std::f32::consts::PI / 36. { continue; }

        // Calculate transform offset in the direction we are aiming
        let tower_world_width = grid_imprint.world_size().x;
        let offset = Vec2::new(
            top_rotation.current_angle.cos() * tower_world_width * 0.4,
            top_rotation.current_angle.sin() * tower_world_width * 0.4,
        );
        let spawn_position = transform.translation.xy() + offset;

        commands.spawn(BuilderLaserDart::new(spawn_position, target_wisp, (wisp_position - spawn_position).normalize(), attack_damage.clone()));
        timer.0.reset();
    }
}