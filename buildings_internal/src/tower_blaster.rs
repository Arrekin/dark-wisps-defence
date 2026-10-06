use bevy::{
    platform::collections::HashMap,
    prelude::*,
};

use alteration::{
    effects::prelude::*,
    modifiers::prelude::*,
};
use almanach::{BuildingInfo, ObjectPresentation, prelude::*};
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

pub(crate) struct TowerBlasterPlugin;
impl Plugin for TowerBlasterPlugin {
    fn build(&self, app: &mut App) {
        let almanach_info = BuilderTowerBlaster::almanach_info(app.world().resource::<AssetServer>());
        app
            .add_observer(BuilderTowerBlaster::on_builder_add_spawn_tower_blaster)
            .add_observer(on_tower_blaster_place_request_do_so)
            .add_systems(Update, shooting_system.run_if(in_state(GameState::Running)))
            .add_systems(CollectSave, collect_tower_blasters)
            .register_loader(MapLoadingStage::SpawnMapElements, "tower_blasters", load_tower_blasters)
            .register_building(BuildingType::Tower(TowerType::Blaster), almanach_info);
    }
}

#[derive(Component, SSS)]
pub(crate) struct BuilderTowerBlaster {
    pub grid_position: GridCoords,
    /// Saved integrity points. `None` ⇒ defer to baseline (fresh spawn);
    /// `Some` ⇒ override with saved value (restore).
    pub integrity_points: Option<IntegrityPoints>,
    /// Set when the player disabled this building. `None` on fresh spawn.
    pub disabled_by_player: Option<DisabledByPlayer>,
}
impl BuilderTowerBlaster {
    pub fn almanach_info(asset_server: &AssetServer) -> BuildingInfo {
        BuildingInfo {
            name: "Blaster Tower".to_string(),
            description: "Rapid laser darts. Little damage per hit, but it almost never stops firing.".to_string(),
            sprite: asset_server.load("buildings/tower_blaster.png"),
            top_sprite: Some(asset_server.load("buildings/tower_blaster_top.png")),
            grid_imprint: GridImprint::Rectangle { width: 2, height: 2 },
            cost: vec![ResourceAmount::new(ResourceType::DarkOre, 150)],
            baseline: HashMap::from([
                (ModifierType::MaxIntegrityPoints, 100.),
                (ModifierType::AttackRange, 15.),
                (ModifierType::AttackSpeed, 5.),
                (ModifierType::AttackDamage, 1.),
            ]),
            sockets: vec![
                ("attack_speed".into(), ShardSocket::new(ShardType::Speed, "Attack speed", ModifierType::AttackSpeed, [1., 2., 3.])),
                ("attack_range".into(), ShardSocket::new(ShardType::Reach, "Attack range", ModifierType::AttackRange, [2., 4., 6.])),
                ("damage".into(), ShardSocket::new(ShardType::Strength, "Damage", ModifierType::AttackDamage, [2., 4., 6.])),
            ],
            validate: building_validator,
            annotate: annotate_non_empty,
            placement: PlacementModes::default(),
            presentation: ObjectPresentation {
                tooltip: Some(building_tooltip),
            },
        }
    }

    pub fn new(grid_position: GridCoords) -> Self {
        Self { grid_position, integrity_points: None, disabled_by_player: None }
    }
    pub fn with_integrity_points(mut self, integrity_points: f32) -> Self {
        self.integrity_points = Some(IntegrityPoints::new(integrity_points));
        self
    }
    pub fn with_disabled_by_player(mut self, disabled_by_player: bool) -> Self {
        self.disabled_by_player = disabled_by_player.then_some(DisabledByPlayer);
        self
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

        commands.entity(entity)
            .remove::<BuilderTowerBlaster>()
            .insert_some(builder.integrity_points)
            .insert_some(builder.disabled_by_player)
            .insert((
                TowerBlaster,
                Sprite {
                    image: building_info.sprite.clone(),
                    custom_size: Some(grid_imprint.world_size()),
                    ..default()
                },
                builder.grid_position,
                grid_imprint,
                TowerTopRotation { speed: 10.0, current_angle: 0. },
                NeedsPower,
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
            .observe(on_technical_state_changed_recompute_operational);
        let world_size = grid_imprint.world_size();
        let tower_top = commands.spawn((
            Sprite {
                image: building_info.top_sprite.clone().expect("Blaster Tower defines a top sprite"),
                custom_size: Some(Vec2::new(world_size.x * 1.52 * 0.5, world_size.y * 0.5)),
                ..default()
            },
            MarkerTowerRotationalTop(entity),
        )).id();
        commands.entity(entity).add_child(tower_top);
        for (content_id, socket) in &building_info.sockets {
            commands.trigger(ShardSocketUpsert::new(entity, content_id.clone(), socket.clone()));
        }
        commands.trigger(TechnicalStateChanged { entity, kind: TechnicalChange::JustSpawned });
    }
}

fn on_tower_blaster_place_request_do_so(
    trigger: On<PlaceRequest>,
    mut commands: Commands,
    mut placement: BuildingPlacementManager,
) {
    let PlaceRequest(MapObject::Building(BuildingType::Tower(TowerType::Blaster))) = *trigger else { return };
    let Some(coords) = placement.claim(BuildingType::Tower(TowerType::Blaster)) else { return };
    commands.spawn(BuilderTowerBlaster::new(coords));
}

#[log_tags(Tag::GameSave)]
fn collect_tower_blasters(
    towers: Query<(Entity, &GridCoords, &IntegrityPoints, Has<DisabledByPlayer>), With<TowerBlaster>>,
    mut save: SaveWriter,
) {
    if towers.is_empty() { return; }

    #[debug_dev("Saving {} tower blasters", rows.len())]
    let rows: Vec<(u32, GridCoords, f32, bool)> = towers
        .iter()
        .map(|(entity, coords, integrity_points, disabled_by_player)| {
            (
                entity.index_u32(),
                *coords,
                integrity_points.get_current(),
                disabled_by_player,
            )
        })
        .collect();
    save.submit(move |ctx| {
        for (id, coords, integrity_points, disabled_by_player) in rows {
            ctx.save_marker("tower_blasters", id)?;
            ctx.save_grid_coords(id, coords)?;
            ctx.save_integrity_points(id, integrity_points)?;
            if disabled_by_player {
                ctx.save_disabled_by_player(id)?;
            }
        }
        Ok(())
    });
}

fn load_tower_blasters(ctx: &mut LoadContext) -> LoadResult {
    ctx.for_each_entity("SELECT id FROM tower_blasters", |ctx, old_id, entity, _| {
        let grid_position = ctx.grid_coords(old_id)?;
        let integrity_points = ctx.integrity_points(old_id)?;
        let disabled_by_player = ctx.disabled_by_player(old_id)?;
        let builder = BuilderTowerBlaster::new(grid_position)
            .with_integrity_points(integrity_points)
            .with_disabled_by_player(disabled_by_player);
        ctx.insert(entity, builder);
        Ok(())
    })
}

fn shooting_system(
    mut commands: Commands,
    mut tower_blasters: Query<(&GridImprint, &Transform, &mut TowerShootingTimer, &mut TowerWispTarget, &TowerTopRotation, &AttackDamage), (With<TowerBlaster>, With<IsOperational>)>,
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
        let offset = Vec2::from_angle(top_rotation.current_angle) * tower_world_width * 0.4;
        let spawn_position = transform.translation.xy() + offset;

        commands.spawn(
            BuilderLaserDart::new(spawn_position, (wisp_position - spawn_position).normalize(), *attack_damage)
                .with_target_wisp(target_wisp)
        );
        timer.0.reset();
    }
}
