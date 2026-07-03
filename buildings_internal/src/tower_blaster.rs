use bevy::{
    platform::collections::HashMap,
    prelude::*,
};

use alteration::{
    effects::prelude::*,
    modifiers::prelude::*,
};
use almanach::prelude::*;
use buildings::prelude::*;
use game_core::{math::angle_difference, prelude::*};
use grids::{
    placement::annotate_non_empty,
    prelude::*,
};
use hud::prelude::{IndicatorDisplay, IndicatorType, Indicators};
use persistence::{
    prelude::*,
    rusqlite,
};
use resources::prelude::*;
use shards::prelude::*;
use states::prelude::*;
use weaponry::prelude::*;
use wisps::prelude::*;

use crate::common::*;

pub struct TowerBlasterPlugin;
impl Plugin for TowerBlasterPlugin {
    fn build(&self, app: &mut App) {
        let almanach_info = BuilderTowerBlaster::almanach_info(app.world().resource::<AssetServer>());
        app
            .add_observer(BuilderTowerBlaster::on_builder_add_spawn_tower_blaster)
            .add_systems(Update, (
                shooting_system.run_if(in_state(GameState::Running)),
            ))
            .register_db_loader::<BuilderTowerBlaster>(MapLoadingStage::SpawnMapElements)
            .register_db_saver(BuilderTowerBlaster::on_game_save_collect_tower_blaster)
            .register_building(BuildingType::Tower(TowerType::Blaster), almanach_info)
            ;
    }
}

#[derive(Clone, Debug)]
pub(crate) struct TowerBlasterSaveData {
    entity: Entity,
    integrity_points: f32,
    disabled_by_player: bool,
}

#[derive(Component, SSS)]
pub(crate) struct BuilderTowerBlaster {
    grid_position: GridCoords,
    save_data: Option<TowerBlasterSaveData>,
}
impl Saveable for BuilderTowerBlaster {
    fn save(self, tx: &rusqlite::Transaction) -> rusqlite::Result<()> {
        let save_data = self.save_data.expect("BuilderTowerBlaster for saving must have save_data");
        let entity_index = save_data.entity.index_u32() as i64;

        tx.save_marker("tower_blasters", entity_index)?;
        tx.save_grid_coords(entity_index, self.grid_position)?;
        tx.save_integrity_points(entity_index, save_data.integrity_points)?;
        if save_data.disabled_by_player {
            tx.save_disabled_by_player(entity_index)?;
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
            let integrity_points = ctx.conn.get_integrity_points(old_id)?;
            let disabled_by_player = ctx.conn.get_disabled_by_player(old_id)?;

            if let Some(new_entity) = ctx.get_new_entity_for_old(old_id) {
                let save_data = TowerBlasterSaveData { entity: new_entity, integrity_points, disabled_by_player };
                ctx.commands.entity(new_entity).insert(BuilderTowerBlaster::new_for_saving(grid_position, save_data));
            }
            count += 1;
        }

        Ok(count.into())
    }
}

impl BuilderTowerBlaster {
    pub fn almanach_info(asset_server: &AssetServer) -> BuildingInfo {
        BuildingInfo {
            name: "Blaster Tower".to_string(),
            sprite: asset_server.load("buildings/tower_blaster.png"),
            top_sprite: Some(asset_server.load("buildings/tower_blaster_top.png")),
            grid_imprint: GridImprint::Rectangle { width: 2, height: 2 },
            cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 150 }],
            baseline: HashMap::from([
                (ModifierType::MaxIntegrityPoints, 100.),
                (ModifierType::AttackRange, 15.),
                (ModifierType::AttackSpeed, 5.),
                (ModifierType::AttackDamage, 1.),
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

    pub fn new(grid_position: GridCoords) -> Self {
        Self { grid_position, save_data: None }
    }
    pub fn new_for_saving(grid_position: GridCoords, save_data: TowerBlasterSaveData) -> Self {
        Self { grid_position, save_data: Some(save_data) }
    }

    fn on_game_save_collect_tower_blaster(
        mut commands: Commands,
        towers: Query<(Entity, &GridCoords, &IntegrityPoints, Has<DisabledByPlayer>), With<TowerBlaster>>,
    ) {
        if towers.is_empty() { return; }
        let batch = towers.iter().map(|(entity, coords, integrity_points, disabled_by_player)| {
            let save_data = TowerBlasterSaveData {
                entity,
                integrity_points: integrity_points.get_current(),
                disabled_by_player,
            };
            BuilderTowerBlaster::new_for_saving(*coords, save_data)
        }).collect::<SaveableBatchCommand<_>>();
        commands.queue(batch);
    }

    pub fn on_builder_add_spawn_tower_blaster(
        trigger: On<Add, BuilderTowerBlaster>,
        mut commands: Commands,
        almanach: Res<Almanach>,
        builders: Query<&BuilderTowerBlaster>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return; };
        
        let building_info = almanach.get_building_info(BuildingType::Tower(TowerType::Blaster));
        let grid_imprint = building_info.grid_imprint;

        let mut entity_commands = commands.entity(entity);
        if let Some(save_data) = &builder.save_data {
            entity_commands.insert(IntegrityPoints::new(save_data.integrity_points));
            if save_data.disabled_by_player {
                entity_commands.insert(DisabledByPlayer);
            }
        }

        let tower_base_entity = entity_commands
            .remove::<BuilderTowerBlaster>()
            .insert((
                TowerBlaster,
                Sprite {
                    image: building_info.sprite.clone(),
                    custom_size: Some(grid_imprint.world_size()),
                    ..Default::default()
                },
                builder.grid_position,
                grid_imprint,
                TowerTopRotation { speed: 10.0, current_angle: 0. },
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
            .id();
        let world_size = grid_imprint.world_size();
        let tower_top = commands.spawn((
            Sprite {
                image: building_info.top_sprite.clone().unwrap(),
                custom_size: Some(Vec2::new(world_size.x * 1.52 * 0.5, world_size.y * 0.5)),
                ..Default::default()
            },
            ZDepth(Z_TOWER_TOP),
            MarkerTowerRotationalTop(tower_base_entity),
        )).id();
        commands.entity(entity).add_child(tower_top);
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
                commands.spawn(ShardEffect::from_modifiers(trigger.shard_target, HashMap::from([(ModifierType::AttackDamage, 2.0)])));
            }
            ShardType::Speed => {
                commands.spawn(ShardEffect::from_modifiers(trigger.shard_target, HashMap::from([(ModifierType::AttackSpeed, 1.0)])));
            }
            ShardType::Fire | ShardType::Water | ShardType::Light | ShardType::Electric => {}
        }
    }
}

fn shooting_system(
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