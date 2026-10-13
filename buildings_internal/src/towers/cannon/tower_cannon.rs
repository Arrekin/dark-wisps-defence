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
use game_core::prelude::*;
use grids::placement::{annotate_non_empty, PlacementModes, PlaceRequest};
use hud::prelude::{IndicatorDisplay, IndicatorType, Indicators};
use logging::prelude::*;
use persistence::prelude::*;
use resources::prelude::*;
use shards::prelude::*;
use states::prelude::*;
use weaponry::{lob::{LobPreview, LobProfile}, prelude::*};
use wisps::prelude::*;

use crate::{common::*, tooltip::building_tooltip};

pub(crate) struct TowerCannonPlugin;
impl Plugin for TowerCannonPlugin {
    fn build(&self, app: &mut App) {
        let almanach_info = BuilderTowerCannon::almanach_info(app.world().resource::<AssetServer>());
        app
            .add_systems(Update, shooting_system.after(TowerAimingSystems).run_if(in_state(GameState::Running)))
            .add_observer(BuilderTowerCannon::on_builder_add_spawn_tower_cannon)
            .add_observer(on_tower_cannon_place_request_do_so)
            .add_systems(CollectSave, collect_tower_cannons)
            .register_loader(MapLoadingStage::SpawnMapElements, "tower_cannons", load_tower_cannons)
            .register_building(BuildingType::Tower(TowerType::Cannon), almanach_info);
    }
}

/// The arc of every shot this tower fires.
const CANNON_LOB: LobProfile = LobProfile {
    launch_angle: 65_f32.to_radians(),
    gravity: 1700.,
    max_apex: 7. * CELL_SIZE,
};

#[derive(Component, SSS)]
pub(crate) struct BuilderTowerCannon {
    pub grid_position: GridCoords,
    /// Saved integrity points. `None` ⇒ defer to baseline (fresh spawn);
    /// `Some` ⇒ override with saved value (restore).
    pub integrity_points: Option<IntegrityPoints>,
    /// Set when the player disabled this building. `None` on fresh spawn.
    pub disabled_by_player: Option<DisabledByPlayer>,
    /// Saved reload progress. `None` on fresh spawn.
    pub shooting_timer: Option<TowerShootingTimer>,
}

impl BuilderTowerCannon {
    pub fn almanach_info(asset_server: &AssetServer) -> BuildingInfo {
        BuildingInfo {
            name: "Cannon Tower".to_string(),
            description: "Heavy single shots. Hits hard but fires slowly.".to_string(),
            sprite: asset_server.load("buildings/tower_cannon.png"),
            top_sprite: None,
            grid_imprint: GridImprint::Rectangle { width: 3, height: 3 },
            cost: vec![ResourceAmount::new(ResourceType::DarkOre, 250)],
            baseline: HashMap::from([
                (ModifierType::MaxIntegrityPoints, 100.),
                (ModifierType::AttackRange, 15.),
                (ModifierType::AttackSpeed, 0.5),
                (ModifierType::AttackDamage, 50.),
            ]),
            sockets: vec![
                ("attack_speed".into(), ShardSocket::new(ShardType::Speed, "Attack speed", ModifierType::AttackSpeed, [0.15, 0.3, 0.45])),
                ("attack_range".into(), ShardSocket::new(ShardType::Reach, "Attack range", ModifierType::AttackRange, [2., 4., 6.])),
                ("damage".into(), ShardSocket::new(ShardType::Strength, "Damage", ModifierType::AttackDamage, [15., 30., 45.])),
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
        Self { grid_position, integrity_points: None, disabled_by_player: None, shooting_timer: None }
    }
    pub fn with_integrity_points(mut self, integrity_points: f32) -> Self {
        self.integrity_points = Some(IntegrityPoints::new(integrity_points));
        self
    }
    pub fn with_disabled_by_player(mut self, disabled_by_player: bool) -> Self {
        self.disabled_by_player = disabled_by_player.then_some(DisabledByPlayer);
        self
    }
    pub fn with_shooting_elapsed(mut self, elapsed: f32) -> Self {
        self.shooting_timer = Some(TowerShootingTimer::from_elapsed(elapsed));
        self
    }

    pub fn on_builder_add_spawn_tower_cannon(
        trigger: On<Add<BuilderTowerCannon>>,
        mut commands: Commands,
        almanach: Res<Almanach>,
        builders: Query<&BuilderTowerCannon>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return; };

        let building_info = almanach.get_building_info(BuildingType::Tower(TowerType::Cannon));
        let grid_imprint = building_info.grid_imprint;

        commands.entity(entity)
            .remove::<BuilderTowerCannon>()
            .insert_some(builder.integrity_points)
            .insert_some(builder.disabled_by_player)
            .insert_some(builder.shooting_timer.clone())
            .insert((
                TowerCannon,
                CANNON_LOB,
                LobPreview,
                Sprite {
                    image: building_info.sprite.clone(),
                    custom_size: Some(grid_imprint.world_size()),
                    ..default()
                },
                builder.grid_position,
                grid_imprint,
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
        for (content_id, socket) in &building_info.sockets {
            commands.trigger(ShardSocketUpsert::new(entity, content_id.clone(), socket.clone()));
        }
        commands.trigger(TechnicalStateChanged { entity, kind: TechnicalChange::JustSpawned });
    }
}

fn on_tower_cannon_place_request_do_so(
    trigger: On<PlaceRequest>,
    mut commands: Commands,
    mut placement: BuildingPlacementManager,
) {
    let PlaceRequest(MapObject::Building(BuildingType::Tower(TowerType::Cannon))) = *trigger else { return };
    let Some(coords) = placement.claim(BuildingType::Tower(TowerType::Cannon)) else { return };
    commands.spawn(BuilderTowerCannon::new(coords));
}

#[log_tags(Tag::GameSave)]
fn collect_tower_cannons(
    towers: Query<(Entity, &GridCoords, &IntegrityPoints, Has<DisabledByPlayer>, &TowerShootingTimer), With<TowerCannon>>,
    mut save: SaveWriter,
) {
    if towers.is_empty() { return; }

    #[debug_dev("Saving {} tower cannons", rows.len())]
    let rows: Vec<(u32, GridCoords, f32, bool, f32)> = towers
        .iter()
        .map(|(entity, coords, integrity_points, disabled_by_player, shooting_timer)| {
            (
                entity.index_u32(),
                *coords,
                integrity_points.get_current(),
                disabled_by_player,
                shooting_timer.0.elapsed_secs(),
            )
        })
        .collect();
    save.submit(move |ctx| {
        for (id, coords, integrity_points, disabled_by_player, shooting_elapsed) in rows {
            ctx.save_marker("tower_cannons", id)?;
            ctx.save_grid_coords(id, coords)?;
            ctx.save_integrity_points(id, integrity_points)?;
            ctx.save_shooting_elapsed(id, shooting_elapsed)?;
            if disabled_by_player {
                ctx.save_disabled_by_player(id)?;
            }
        }
        Ok(())
    });
}

fn load_tower_cannons(ctx: &mut LoadContext) -> LoadResult {
    ctx.for_each_entity("SELECT id FROM tower_cannons", |ctx, old_id, entity, _| {
        let grid_position = ctx.grid_coords(old_id)?;
        let integrity_points = ctx.integrity_points(old_id)?;
        let disabled_by_player = ctx.disabled_by_player(old_id)?;
        let shooting_elapsed = ctx.shooting_elapsed(old_id)?;
        let builder = BuilderTowerCannon::new(grid_position)
            .with_integrity_points(integrity_points)
            .with_disabled_by_player(disabled_by_player)
            .with_shooting_elapsed(shooting_elapsed);
        ctx.insert(entity, builder);
        Ok(())
    })
}

fn shooting_system(
    mut commands: Commands,
    mut tower_cannons: Query<(Entity, &Transform, &LobProfile, &mut TowerShootingTimer, &mut TowerWispTarget, &AttackDamage), (With<TowerCannon>, With<IsOperational>)>,
    wisps: Query<WispHeading, With<Wisp>>,
) {
    for (tower, transform, lob_profile, mut timer, mut target, attack_damage) in tower_cannons.iter_mut() {
        let TowerWispTarget::Wisp(target_wisp) = *target else { continue; };
        if !timer.0.is_finished() { continue; }

        let Ok(wisp_heading) = wisps.get(target_wisp) else {
            // Target wisp does not exist anymore
            *target = TowerWispTarget::SearchForNewTarget;
            continue;
        };

        let lob = lob_profile.lob(transform.translation.xy(), wisp_heading.next_cell_center());
        commands.spawn(BuilderCannonball::new(lob, *attack_damage).with_shooter(tower));
        timer.0.reset();
    }
}
