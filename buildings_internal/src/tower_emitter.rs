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
    placement::annotate_non_empty,
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

pub struct TowerEmitterPlugin;
impl Plugin for TowerEmitterPlugin {
    fn build(&self, app: &mut App) {
        let almanach_info = BuilderTowerEmitter::almanach_info(app.world().resource::<AssetServer>());
        app
            .add_observer(BuilderTowerEmitter::on_builder_add_spawn_tower_emitter)
            .add_systems(Update, (
                shooting_system.run_if(in_state(GameState::Running)),
            ))
            .add_systems(CollectSave, collect_tower_emitters)
            .register_loader(MapLoadingStage::SpawnMapElements, "tower_emitters", load_tower_emitters)
            .register_building(BuildingType::Tower(TowerType::Emitter), almanach_info)
            ;
    }
}

#[derive(Component, SSS)]
pub(crate) struct BuilderTowerEmitter {
    pub grid_position: GridCoords,
    /// Saved integrity points. `None` ⇒ defer to baseline (fresh spawn);
    /// `Some` ⇒ override with saved value (restore).
    pub integrity_points: Option<f32>,
    /// Whether the player disabled this building. False on fresh spawn.
    pub disabled_by_player: bool,
}

impl BuilderTowerEmitter {
    pub fn almanach_info(asset_server: &AssetServer) -> BuildingInfo {
        BuildingInfo {
            name: "Emitter Tower".to_string(),
            sprite: asset_server.load("buildings/tower_emitter.png"),
            top_sprite: None,
            grid_imprint: GridImprint::Rectangle { width: 2, height: 2 },
            cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 450 }],
            baseline: HashMap::from([
                (ModifierType::MaxIntegrityPoints, 100.),
                (ModifierType::AttackRange, 4.),
                (ModifierType::AttackSpeed, 0.2),
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

    pub fn new(grid_position: GridCoords) -> Self { Self { grid_position, integrity_points: None, disabled_by_player: false } }
    pub fn with_integrity_points(mut self, integrity_points: f32) -> Self {
        self.integrity_points = Some(integrity_points);
        self
    }
    pub fn with_disabled_by_player(mut self) -> Self {
        self.disabled_by_player = true;
        self
    }

    pub fn on_builder_add_spawn_tower_emitter(
        trigger: On<Add, BuilderTowerEmitter>,
        mut commands: Commands,
        almanach: Res<Almanach>,
        builders: Query<&BuilderTowerEmitter>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return; };

        let building_info = almanach.get_building_info(BuildingType::Tower(TowerType::Emitter));
        let grid_imprint = building_info.grid_imprint;

        let mut entity_commands = commands.entity(entity);
        if let Some(ip) = builder.integrity_points {
            entity_commands.insert(IntegrityPoints::new(ip));
        }
        if builder.disabled_by_player {
            entity_commands.insert(DisabledByPlayer);
        }

        entity_commands
            .remove::<BuilderTowerEmitter>()
            .insert((
                TowerEmitter,
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
            .observe(Self::on_shard_apply_do_so);
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
                commands.spawn(ShardEffect::from_modifiers(trigger.shard_target, HashMap::from([(ModifierType::AttackSpeed, 0.05)])));
            }
            ShardType::Fire | ShardType::Water | ShardType::Light | ShardType::Electric => {}
        }
    }
}

fn collect_tower_emitters(
    towers: Query<(Entity, &GridCoords, &IntegrityPoints, Has<DisabledByPlayer>), With<TowerEmitter>>,
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
    Log::debug().dev().tag(Tag::GameSave).message(format!("Saving {} tower emitters", rows.len()));
    save.submit(move |tx| {
        for (id, gx, gy, integrity_points, disabled_by_player) in rows {
            tx.save_marker("tower_emitters", id)?;
            tx.save_grid_coords(id, GridCoords { x: gx, y: gy })?;
            tx.save_integrity_points(id, integrity_points)?;
            if disabled_by_player {
                tx.save_disabled_by_player(id)?;
            }
        }
        Ok(())
    });
}

fn load_tower_emitters(ctx: &mut LoadContext) -> rusqlite::Result<()> {
    let mut stmt = ctx.conn.prepare("SELECT id FROM tower_emitters")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let old_id: i64 = row.get(0)?;
        let grid_position = ctx.conn.get_grid_coords(old_id)?;
        let integrity_points = ctx.conn.get_integrity_points(old_id)?;
        let disabled_by_player = ctx.conn.get_disabled_by_player(old_id)?;

        let Some(entity) = ctx.entity(old_id) else {
            Log::warn().dev().tag(Tag::GameLoad).message(format!("TowerEmitter with old ID {old_id} has no corresponding new entity"));
            continue;
        };
        let mut builder = BuilderTowerEmitter::new(grid_position)
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
