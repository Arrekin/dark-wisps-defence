use crate::weaponry::ripple::BuilderRipple;
use crate::prelude::*;
use crate::ui::indicators::{IndicatorDisplay, IndicatorType, Indicators};
use crate::wisps::components::Wisp;

pub struct TowerEmitterPlugin;
impl Plugin for TowerEmitterPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_observer(BuilderTowerEmitter::on_add)
            .add_systems(Update, (
                shooting_system.run_if(in_state(GameState::Running)),
            ))
            .register_db_loader::<BuilderTowerEmitter>(MapLoadingStage::SpawnMapElements)
            .register_db_saver(BuilderTowerEmitter::on_game_save)
            .register_building(BuildingType::Tower(TowerType::Emitter), BuilderTowerEmitter::almanach_info());
    }
}

pub const TOWER_EMITTER_BASE_IMAGE: &str = "buildings/tower_emitter.png";

#[derive(Clone, Debug)]
pub struct TowerEmitterSaveData {
    entity: Entity,
    health: f32,
    disabled_by_player: bool,
    upgrade_levels: HashMap<UpgradeType, usize>,
}

#[derive(Component, SSS)]
pub struct BuilderTowerEmitter {
    grid_position: GridCoords,
    save_data: Option<TowerEmitterSaveData>,
}

impl Saveable for BuilderTowerEmitter {
    fn save(self, tx: &rusqlite::Transaction) -> rusqlite::Result<()> {
        let save_data = self.save_data.expect("BuilderTowerEmitter for saving must have save_data");
        let entity_index = save_data.entity.index_u32() as i64;

        tx.save_marker("tower_emitters", entity_index)?;
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

impl Loadable for BuilderTowerEmitter {
    fn load(ctx: &mut LoadContext) -> rusqlite::Result<LoadResult> {
        let mut stmt = ctx.conn.prepare("SELECT id FROM tower_emitters LIMIT ?1 OFFSET ?2")?;
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
                let save_data = TowerEmitterSaveData { entity: new_entity, health, disabled_by_player, upgrade_levels };
                ctx.commands.entity(new_entity).insert(BuilderTowerEmitter::new_for_saving(grid_position, save_data));
            }
            count += 1;
        }

        Ok(count.into())
    }
}

impl BuilderTowerEmitter {
    pub fn almanach_info() -> BuildingInfo {
        BuildingInfo {
            name: "Emitter Tower".to_string(),
            grid_imprint: GridImprint::Rectangle { width: 2, height: 2 },
            cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 450 }],
            baseline: HashMap::from([
                (ModifierType::MaxHealth, 100.),
                (ModifierType::AttackRange, 4.),
                (ModifierType::AttackSpeed, 0.2),
                (ModifierType::AttackDamage, 1.),
            ]),
            upgrades: HashMap::from([
                (UpgradeType::Modifier(ModifierType::AttackRange), UpgradeInfo {
                    levels: vec![
                        UpgradeLevelInfo { value: 1., cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 100 }] },
                        UpgradeLevelInfo { value: 2., cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 200 }] },
                        UpgradeLevelInfo { value: 3., cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 300 }] },
                    ],
                }),
                (UpgradeType::Modifier(ModifierType::AttackSpeed), UpgradeInfo {
                    levels: vec![
                        UpgradeLevelInfo { value: 0.1, cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 100 }] },
                        UpgradeLevelInfo { value: 0.1, cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 200 }] },
                        UpgradeLevelInfo { value: 0.1, cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 300 }] },
                    ],
                }),
            ]),
            validate: building_validator,
        }
    }

    pub fn new(grid_position: GridCoords) -> Self { Self { grid_position, save_data: None } }
    pub fn new_for_saving(grid_position: GridCoords, save_data: TowerEmitterSaveData) -> Self {
        Self { grid_position, save_data: Some(save_data) }
    }

    fn on_game_save(
        mut commands: Commands,
        towers: Query<(Entity, &GridCoords, &Health, Has<DisabledByPlayer>, &Upgrades), With<TowerEmitter>>,
    ) {
        if towers.is_empty() { return; }
        let batch = towers.iter().map(|(entity, coords, health, disabled_by_player, upgrades)| {
            let save_data = TowerEmitterSaveData {
                entity,
                health: health.get_current(),
                disabled_by_player,
                upgrade_levels: upgrades.get_levels(),
            };
            BuilderTowerEmitter::new_for_saving(*coords, save_data)
        }).collect::<SaveableBatchCommand<_>>();
        commands.queue(batch);
    }

    pub fn on_add(
        trigger: On<Add, BuilderTowerEmitter>,
        mut commands: Commands,
        builders: Query<&BuilderTowerEmitter>,
        asset_server: Res<AssetServer>,
        almanach: Res<Almanach>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return; };
        
        let building_info = almanach.get_building_info(BuildingType::Tower(TowerType::Emitter));
        let grid_imprint = building_info.grid_imprint;

        let mut entity_commands = commands.entity(entity);
        if let Some(save_data) = &builder.save_data {
            entity_commands.insert(Health::new(save_data.health));
            if save_data.disabled_by_player {
                entity_commands.insert(DisabledByPlayer);
            }
        }

        entity_commands
            .remove::<BuilderTowerEmitter>()
            .insert((
                TowerEmitter,
                Sprite {
                    image: asset_server.load(TOWER_EMITTER_BASE_IMAGE),
                    custom_size: Some(grid_imprint.world_size()),
                    ..Default::default()
                },
                Tower,
                builder.grid_position,
                grid_imprint,
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
            ));
    }
}

pub fn shooting_system(
    mut commands: Commands,
    mut tower_emitters: Query<(&Transform, &AttackRange, &mut TowerShootingTimer, &mut TowerWispTarget), (With<TowerEmitter>, With<HasPower>, Without<DisabledByPlayer>)>,
    wisps: Query<(), With<Wisp>>,
) {
    for (transform, range, mut timer, mut target) in tower_emitters.iter_mut() {
        let TowerWispTarget::Wisp(target_wisp) = *target else { continue; };
        if !timer.0.is_finished() { continue; }

        if !wisps.contains(target_wisp) {
            // Target wisp does not exist anymore
            *target = TowerWispTarget::SearchForNewTarget;
            continue;
        };

        commands.spawn(BuilderRipple::new(transform.translation.xy(), range.get() * CELL_SIZE));
        timer.0.reset();
    }
}
