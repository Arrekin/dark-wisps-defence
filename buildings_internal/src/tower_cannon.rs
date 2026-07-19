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
use game_core::prelude::*;
use grids::{
    placement::{annotate_non_empty, PlacementChannel},
    prelude::*,
};
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

pub struct TowerCannonPlugin;
impl Plugin for TowerCannonPlugin {
    fn build(&self, app: &mut App) {
        let almanach_info = BuilderTowerCannon::almanach_info(app.world().resource::<AssetServer>());
        app
            .add_systems(Update, (
                shooting_system.run_if(in_state(GameState::Running)),
            ))
            .add_observer(BuilderTowerCannon::on_builder_add_spawn_tower_cannon)
            .add_systems(CollectSave, collect_tower_cannons)
            .register_loader(MapLoadingStage::SpawnMapElements, "tower_cannons", load_tower_cannons)
            .register_building(BuildingType::Tower(TowerType::Cannon), almanach_info)
            ;
    }
}

#[derive(Component, SSS)]
pub(crate) struct BuilderTowerCannon {
    pub grid_position: GridCoords,
    /// Saved integrity points. `None` ⇒ defer to baseline (fresh spawn);
    /// `Some` ⇒ override with saved value (restore).
    pub integrity_points: Option<f32>,
    /// Whether the player disabled this building. False on fresh spawn.
    pub disabled_by_player: bool,
}

impl BuilderTowerCannon {
    pub fn almanach_info(asset_server: &AssetServer) -> BuildingInfo {
        BuildingInfo {
            name: "Cannon Tower".to_string(),
            sprite: asset_server.load("buildings/tower_cannon.png"),
            top_sprite: None,
            grid_imprint: GridImprint::Rectangle { width: 3, height: 3 },
            cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 250 }],
            baseline: HashMap::from([
                (ModifierType::MaxIntegrityPoints, 100.),
                (ModifierType::AttackRange, 15.),
                (ModifierType::AttackSpeed, 0.5),
                (ModifierType::AttackDamage, 50.),
            ]),
            validate: building_validator,
            annotate: annotate_non_empty,
            placement: PlacementChannel::of::<Building>(),
        }
    }

    pub fn new(grid_position: GridCoords) -> Self {
        Self { grid_position, integrity_points: None, disabled_by_player: false }
    }
    pub fn with_integrity_points(mut self, integrity_points: f32) -> Self {
        self.integrity_points = Some(integrity_points);
        self
    }
    pub fn with_disabled_by_player(mut self) -> Self {
        self.disabled_by_player = true;
        self
    }

    pub fn on_builder_add_spawn_tower_cannon(
        trigger: On<Add, BuilderTowerCannon>,
        mut commands: Commands,
        almanach: Res<Almanach>,
        builders: Query<&BuilderTowerCannon>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return; };

        let building_info = almanach.get_building_info(BuildingType::Tower(TowerType::Cannon));
        let grid_imprint = building_info.grid_imprint;

        let mut entity_commands = commands.entity(entity);
        if let Some(ip) = builder.integrity_points {
            entity_commands.insert(IntegrityPoints::new(ip));
        }
        if builder.disabled_by_player {
            entity_commands.insert(DisabledByPlayer);
        }

        entity_commands
            .remove::<BuilderTowerCannon>()
            .insert((
                TowerCannon,
                Sprite {
                    image: building_info.sprite.clone(),
                    custom_size: Some(grid_imprint.world_size()),
                    ..Default::default()
                },
                builder.grid_position,
                grid_imprint,
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
            .observe(on_technical_state_changed_recompute_operational);
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
                commands.spawn(ShardEffect::from_modifiers(trigger.shard_target, HashMap::from([(ModifierType::AttackSpeed, 0.15)])));
            }
            ShardType::Fire | ShardType::Water | ShardType::Light | ShardType::Electric => {}
        }
    }
}

fn collect_tower_cannons(
    towers: Query<(Entity, &GridCoords, &IntegrityPoints, Has<DisabledByPlayer>), With<TowerCannon>>,
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
    Log::debug().dev().tag(Tag::GameSave).message(format!("Saving {} tower cannons", rows.len()));
    save.submit(move |tx| {
        for (id, gx, gy, integrity_points, disabled_by_player) in rows {
            tx.save_marker("tower_cannons", id)?;
            tx.save_grid_coords(id, GridCoords { x: gx, y: gy })?;
            tx.save_integrity_points(id, integrity_points)?;
            if disabled_by_player {
                tx.save_disabled_by_player(id)?;
            }
        }
        Ok(())
    });
}

fn load_tower_cannons(ctx: &mut LoadContext) -> rusqlite::Result<()> {
    let mut stmt = ctx.conn.prepare("SELECT id FROM tower_cannons")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let old_id: i64 = row.get(0)?;
        let grid_position = ctx.conn.get_grid_coords(old_id)?;
        let integrity_points = ctx.conn.get_integrity_points(old_id)?;
        let disabled_by_player = ctx.conn.get_disabled_by_player(old_id)?;

        let Some(entity) = ctx.entity(old_id) else {
            Log::warn().dev().tag(Tag::GameLoad).message(format!("TowerCannon with old ID {old_id} has no corresponding new entity"));
            continue;
        };
        let mut builder = BuilderTowerCannon::new(grid_position)
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
    mut tower_cannons: Query<(&Transform, &mut TowerShootingTimer, &mut TowerWispTarget, &AttackDamage), (With<TowerCannon>, With<IsOperational>)>,
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
