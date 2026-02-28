use crate::prelude::*;
use crate::ui::indicators::{IndicatorDisplay, IndicatorType, Indicators};
use crate::projectiles::cannonball::BuilderCannonball;
use crate::wisps::components::Wisp;
use crate::wisps::spawning::WISP_GRID_IMPRINT;

pub struct TowerCannonPlugin;
impl Plugin for TowerCannonPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Update, (
                shooting_system.run_if(in_state(GameState::Running)),
            ))
            .add_observer(BuilderTowerCannon::on_add)
            .register_db_loader::<BuilderTowerCannon>(MapLoadingStage::SpawnMapElements)
            .register_db_saver(BuilderTowerCannon::on_game_save)
            .register_building(BuildingType::Tower(TowerType::Cannon), BuilderTowerCannon::almanach_info());
    }
}

pub const TOWER_CANNON_BASE_IMAGE: &str = "buildings/tower_cannon.png";

#[derive(Clone, Debug)]
pub struct TowerCannonSaveData {
    entity: Entity,
    health: f32,
    disabled_by_player: bool,
    upgrade_levels: HashMap<UpgradeType, usize>,
}

#[derive(Component, SSS)]
pub struct BuilderTowerCannon {
    grid_position: GridCoords,
    save_data: Option<TowerCannonSaveData>,
}

impl Saveable for BuilderTowerCannon {
    fn save(self, tx: &rusqlite::Transaction) -> rusqlite::Result<()> {
        let save_data = self.save_data.expect("BuilderTowerCannon for saving must have save_data");
        let entity_index = save_data.entity.index_u32() as i64;

        tx.save_marker("tower_cannons", entity_index)?;
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

impl Loadable for BuilderTowerCannon {
    fn load(ctx: &mut LoadContext) -> rusqlite::Result<LoadResult> {
        let mut stmt = ctx.conn.prepare("SELECT id FROM tower_cannons LIMIT ?1 OFFSET ?2")?;
        let mut rows = stmt.query(ctx.pagination.as_params())?;
        
        let mut count = 0;
        while let Some(row) = rows.next()? {
            let old_id: i64 = row.get(0)?;
            let grid_position = ctx.conn.get_grid_coords(old_id)?;
            let health = ctx.conn.get_health(old_id)?;
            let disabled_by_player = ctx.conn.get_disabled_by_player(old_id)?;
            let upgrade_levels: HashMap<UpgradeType, usize> = ctx.conn.get_upgrade_levels_raw(old_id)?
                .into_iter()
                .filter_map(|(type_str, level)| UpgradeType::from_db_str(&type_str).map(|t| (t, level)))
                .collect();
            
            if let Some(new_entity) = ctx.get_new_entity_for_old(old_id) {
                let save_data = TowerCannonSaveData { entity: new_entity, health, disabled_by_player, upgrade_levels };
                ctx.commands.entity(new_entity).insert(BuilderTowerCannon::new_for_saving(grid_position, save_data));
            }
            count += 1;
        }

        Ok(count.into())
    }
}

impl BuilderTowerCannon {
    pub fn almanach_info() -> BuildingInfo {
        BuildingInfo {
            name: "Cannon Tower".to_string(),
            grid_imprint: GridImprint::Rectangle { width: 3, height: 3 },
            cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 250 }],
            baseline: HashMap::from([
                (ModifierType::MaxHealth, 100.),
                (ModifierType::AttackRange, 15.),
                (ModifierType::AttackSpeed, 0.5),
                (ModifierType::AttackDamage, 50.),
            ]),
            upgrades: HashMap::from([
                (UpgradeType::Modifier(ModifierType::AttackRange), UpgradeInfo {
                    levels: vec![
                        UpgradeLevelInfo { value: 1., cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 100 }] },
                        UpgradeLevelInfo { value: 2., cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 200 }] },
                        UpgradeLevelInfo { value: 3., cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 300 }] },
                    ],
                }),
                (UpgradeType::Modifier(ModifierType::AttackDamage), UpgradeInfo {
                    levels: vec![
                        UpgradeLevelInfo { value: 5., cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 100 }] },
                        UpgradeLevelInfo { value: 10., cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 200 }] },
                        UpgradeLevelInfo { value: 15., cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 300 }] },
                    ],
                }),
            ]),
            validate: building_validator,
            place_request: PlaceRequest::default(),
        }
    }

    pub fn new(grid_position: GridCoords) -> Self {
        Self { grid_position, save_data: None }
    }
    pub fn new_for_saving(grid_position: GridCoords, save_data: TowerCannonSaveData) -> Self {
        Self { grid_position, save_data: Some(save_data) }
    }

    fn on_game_save(
        mut commands: Commands,
        towers: Query<(Entity, &GridCoords, &Health, Has<DisabledByPlayer>, &Upgrades), With<TowerCannon>>,
    ) {
        if towers.is_empty() { return; }
        let batch = towers.iter().map(|(entity, coords, health, disabled_by_player, upgrades)| {
            let save_data = TowerCannonSaveData {
                entity,
                health: health.get_current(),
                disabled_by_player,
                upgrade_levels: upgrades.get_levels(),
            };
            BuilderTowerCannon::new_for_saving(*coords, save_data)
        }).collect::<SaveableBatchCommand<_>>();
        commands.queue(batch);
    }

    pub fn on_add(
        trigger: On<Add, BuilderTowerCannon>,
        mut commands: Commands,
        builders: Query<&BuilderTowerCannon>,
        asset_server: Res<AssetServer>,
        almanach: Res<Almanach>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return; };
        
        let building_info = almanach.get_building_info(BuildingType::Tower(TowerType::Cannon));
        let grid_imprint = building_info.grid_imprint;

        let mut entity_commands = commands.entity(entity);
        if let Some(save_data) = &builder.save_data {
            entity_commands.insert(Health::new(save_data.health));
            if save_data.disabled_by_player {
                entity_commands.insert(DisabledByPlayer);
            }
        }

        entity_commands
            .remove::<BuilderTowerCannon>()
            .insert((
                TowerCannon,
                Sprite {
                    image: asset_server.load(TOWER_CANNON_BASE_IMAGE),
                    custom_size: Some(grid_imprint.world_size()),
                    ..Default::default()
                },
                Tower,
                builder.grid_position,
                grid_imprint,
                NeedsPower::default(),
                ModifiersBank::from_baseline(&building_info.baseline),
                Upgrades::from_almanach(&building_info.upgrades, builder.save_data.as_ref().map(|d| &d.upgrade_levels)),
                related![Indicators[
                    IndicatorType::NoPower,
                    IndicatorType::DisabledByPlayer,
                ]],
                children![
                    IndicatorDisplay::default(),
                ],
            ));
    }
}

pub fn shooting_system(
    mut commands: Commands,
    mut tower_cannons: Query<(&Transform, &mut TowerShootingTimer, &mut TowerWispTarget, &AttackDamage), (With<TowerCannon>, With<HasPower>, Without<DisabledByPlayer>)>,
    wisps: Query<(&GridPath, &GridCoords), With<Wisp>>,
) {
    for (transform, mut timer, mut target, attack_damage) in tower_cannons.iter_mut() {
        let TowerWispTarget::Wisp(target_wisp) = *target else { continue; };
        if !timer.0.is_finished() { continue; }

        let Ok((wisp_grid_path, wisp_coords)) = wisps.get(target_wisp) else {
            // Target wisp does not exist anymore
            *target = TowerWispTarget::SearchForNewTarget;
            continue;
        };

        // If wisps has path, target the next path position. Otherwise, target the wisp's current position.
        let target_world_position = wisp_grid_path.next_in_path().map_or(
            wisp_coords.to_world_position_centered(WISP_GRID_IMPRINT),
            |coords| coords.to_world_position_centered(WISP_GRID_IMPRINT)
        );

        commands.spawn(BuilderCannonball::new(transform.translation.xy(), target_world_position, attack_damage.clone()));
        timer.0.reset();
    }
}
