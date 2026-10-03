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
use grids::placement::{annotate_non_empty, PlacementModes, PlaceRequest};
use hud::prelude::{IndicatorDisplay, IndicatorType, Indicators};
use logging::prelude::*;
use persistence::prelude::*;
use resources::prelude::*;
use shards::prelude::*;
use states::prelude::*;
use weaponry::prelude::*;
use wisps::prelude::*;

use crate::{common::*, tooltip::building_tooltip};

pub(crate) struct TowerRocketLauncherPlugin;
impl Plugin for TowerRocketLauncherPlugin {
    fn build(&self, app: &mut App) {
        let almanach_info = BuilderTowerRocketLauncher::almanach_info(app.world().resource::<AssetServer>());
        app
            .add_observer(BuilderTowerRocketLauncher::on_builder_add_spawn_tower_rocket_launcher)
            .add_observer(on_tower_rocket_launcher_place_request_do_so)
            .add_systems(Update, shooting_system.run_if(in_state(GameState::Running)))
            .add_systems(CollectSave, collect_tower_rocket_launchers)
            .register_loader(MapLoadingStage::SpawnMapElements, "tower_rocket_launchers", load_tower_rocket_launchers)
            .register_building(BuildingType::Tower(TowerType::RocketLauncher), almanach_info);
    }
}

#[derive(Component, SSS)]
pub(crate) struct BuilderTowerRocketLauncher {
    pub grid_position: GridCoords,
    /// Saved integrity points. `None` ⇒ defer to baseline (fresh spawn);
    /// `Some` ⇒ override with saved value (restore).
    pub integrity_points: Option<IntegrityPoints>,
    /// Set when the player disabled this building. `None` on fresh spawn.
    pub disabled_by_player: Option<DisabledByPlayer>,
}

impl BuilderTowerRocketLauncher {
    pub fn almanach_info(asset_server: &AssetServer) -> BuildingInfo {
        BuildingInfo {
            name: "Rocket Launcher Tower".to_string(),
            description: "Long-range rockets, reaching well beyond the other towers.".to_string(),
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
            placement: PlacementModes::default(),
            presentation: ObjectPresentation {
                tooltip: Some(building_tooltip),
            },
        }
    }

    pub fn new(grid_position: GridCoords) -> Self { Self { grid_position, integrity_points: None, disabled_by_player: None } }
    pub fn with_integrity_points(mut self, integrity_points: f32) -> Self {
        self.integrity_points = Some(IntegrityPoints::new(integrity_points));
        self
    }
    pub fn with_disabled_by_player(mut self, disabled_by_player: bool) -> Self {
        self.disabled_by_player = disabled_by_player.then_some(DisabledByPlayer);
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

        commands.entity(entity)
            .remove::<BuilderTowerRocketLauncher>()
            .insert_some(builder.integrity_points)
            .insert_some(builder.disabled_by_player)
            .insert((
                TowerRocketLauncher,
                Sprite {
                    image: building_info.sprite.clone(),
                    custom_size: Some(grid_imprint.world_size()),
                    ..default()
                },
                builder.grid_position,
                grid_imprint,
                TowerTopRotation { speed: 1.0, current_angle: 0. },
                NeedsPower,
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
        let world_size = grid_imprint.world_size();
        let tower_top = commands.spawn((
            Sprite {
                image: building_info.top_sprite.clone().expect("Rocket Launcher Tower defines a top sprite"),
                custom_size: Some(Vec2::new(world_size.x * 1.52 * 0.5, world_size.y * 0.5)),
                ..default()
            },
            Anchor(Vec2::new(-0.20, 0.0)),
            MarkerTowerRotationalTop(entity),
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

fn on_tower_rocket_launcher_place_request_do_so(
    trigger: On<PlaceRequest>,
    mut commands: Commands,
    mut placement: BuildingPlacementManager,
) {
    let PlaceRequest(MapObject::Building(BuildingType::Tower(TowerType::RocketLauncher))) = *trigger else { return };
    let Some(coords) = placement.claim(BuildingType::Tower(TowerType::RocketLauncher)) else { return };
    commands.spawn(BuilderTowerRocketLauncher::new(coords));
}

#[log_tags(Tag::GameSave)]
fn collect_tower_rocket_launchers(
    towers: Query<(Entity, &GridCoords, &IntegrityPoints, Has<DisabledByPlayer>), With<TowerRocketLauncher>>,
    mut save: SaveWriter,
) {
    if towers.is_empty() { return; }

    #[debug_dev("Saving {} tower rocket launchers", rows.len())]
    let rows: Vec<(i64, GridCoords, f32, bool)> = towers
        .iter()
        .map(|(entity, coords, integrity_points, disabled_by_player)| {
            (
                entity.index_u32() as i64,
                *coords,
                integrity_points.get_current(),
                disabled_by_player,
            )
        })
        .collect();
    save.submit(move |ctx| {
        for (id, coords, integrity_points, disabled_by_player) in rows {
            ctx.save_marker("tower_rocket_launchers", id)?;
            ctx.save_grid_coords(id, coords)?;
            ctx.save_integrity_points(id, integrity_points)?;
            if disabled_by_player {
                ctx.save_disabled_by_player(id)?;
            }
        }
        Ok(())
    });
}

fn load_tower_rocket_launchers(ctx: &mut LoadContext) -> LoadResult {
    ctx.for_each_entity("SELECT id FROM tower_rocket_launchers", |ctx, old_id, entity, _| {
        let grid_position = ctx.grid_coords(old_id)?;
        let integrity_points = ctx.integrity_points(old_id)?;
        let disabled_by_player = ctx.disabled_by_player(old_id)?;
        let builder = BuilderTowerRocketLauncher::new(grid_position)
            .with_integrity_points(integrity_points)
            .with_disabled_by_player(disabled_by_player);
        ctx.insert(entity, builder);
        Ok(())
    })
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
        let offset = Vec2::from_angle(top_rotation.current_angle) * tower_world_width * 0.4;
        let spawn_position = transform.translation.xy() + offset;

        let rocket_angle = Quat::from_rotation_z(top_rotation.current_angle);
        commands.spawn(BuilderRocket::new(spawn_position, rocket_angle, target_wisp, *attack_damage));
        timer.0.reset();
    }
}
