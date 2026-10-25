//! # Blaze Mortar Tower
//!
//! Fire evolution of the Cannon. Trades slow reloads for wide-area damage, lobbing fireballs
//! at its target wisp's next cell. Each impact hits a 3x3 area with reduced damage in the
//! surrounding one-cell ring.

use bevy::{
    platform::collections::HashMap,
    prelude::*,
};

use almanach::{BuildingInfo, prelude::*};
use alteration::{
    effects::prelude::*,
    modifiers::prelude::*,
};
use buildings::prelude::*;
use game_core::prelude::*;
use grids::placement::{annotate_non_empty, PlacementModes, PlaceRequest};
use hud::prelude::{IndicatorDisplay, IndicatorType, Indicators};
use logging::prelude::*;
use persistence::prelude::*;
use resources::prelude::*;
use session::GameClock;
use shards::prelude::*;
use states::prelude::*;
use weaponry::{lob::{LobPreview, LobProfile}, prelude::*};
use wisps::prelude::*;

use crate::common::*;

pub(crate) struct TowerBlazeMortarPlugin;
impl Plugin for TowerBlazeMortarPlugin {
    fn build(&self, app: &mut App) {
        let almanach_info = BuilderTowerBlazeMortar::almanach_info(app.world().resource::<AssetServer>());
        app
            .add_systems(Update, shooting_system.after(TowerAimingSystems).run_if(in_state(GameState::Running)))
            .add_observer(BuilderTowerBlazeMortar::on_builder_add_spawn_tower_blaze_mortar)
            .add_observer(on_tower_blaze_mortar_place_request_do_so)
            .add_systems(CollectSave, collect_tower_blaze_mortars)
            .register_loader(MapLoadingStage::SpawnMapElements, "tower_blaze_mortars", load_tower_blaze_mortars)
            .register_building(BuildingType::Tower(TowerType::BlazeMortar), almanach_info);
    }
}

/// The arc of every shot this tower fires.
const BLAZE_MORTAR_LOB: LobProfile = LobProfile {
    launch_angle: 65_f32.to_radians(),
    gravity: 1700.,
    max_apex: 7. * CELL_SIZE,
};

#[derive(Component, SSS)]
pub(crate) struct BuilderTowerBlazeMortar {
    pub grid_position: GridCoords,
    /// Custom integrity points, or `None` to use the fresh-spawn baseline.
    pub integrity_points: Option<IntegrityPoints>,
    /// Set when the player disabled this building. `None` on fresh spawn.
    pub disabled_by_player: Option<DisabledByPlayer>,
    /// Saved reload progress. `None` on fresh spawn.
    pub shooting_timer: Option<TowerShootingTimer>,
}

impl BuilderTowerBlazeMortar {
    pub fn almanach_info(asset_server: &AssetServer) -> BuildingInfo {
        BuildingInfo {
            name: "Blaze Mortar".to_string(),
            description: "Lobs fireballs that burst over a wide area. Hits hard, reloads slowly.".to_string(),
            sprite: asset_server.load("buildings/tower_cannon.png"),
            top_sprite: None,
            grid_imprint: GridImprint::square(3),
            cost: vec![ResourceAmount::new(ResourceType::DarkOre, 350)],
            baseline: HashMap::from([
                (ModifierType::MaxIntegrityPoints, 100.),
                (ModifierType::AttackRange, 15.),
                (ModifierType::AttackSpeed, 0.3),
                (ModifierType::AttackDamage, 70.),
            ]),
            sockets: vec![
                ("attack_speed".into(), ShardSocket::new(ShardType::Speed, "Attack speed", ModifierType::AttackSpeed, [0.15, 0.3, 0.45])),
                ("attack_range".into(), ShardSocket::new(ShardType::Reach, "Attack range", ModifierType::AttackRange, [2., 4., 6.])),
                ("damage".into(), ShardSocket::new(ShardType::Strength, "Damage", ModifierType::AttackDamage, [15., 30., 45.])),
            ],
            validate: building_validator,
            annotate: annotate_non_empty,
            placement: PlacementModes::default(),
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

    pub fn on_builder_add_spawn_tower_blaze_mortar(
        trigger: On<Add<BuilderTowerBlazeMortar>>,
        mut commands: Commands,
        almanach: Res<Almanach>,
        builders: Query<&BuilderTowerBlazeMortar>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return; };

        let building_info = almanach.get_building_info(BuildingType::Tower(TowerType::BlazeMortar));

        commands.entity(entity)
            .remove::<BuilderTowerBlazeMortar>()
            .insert_some(builder.integrity_points)
            .insert_some(builder.disabled_by_player)
            .insert_some(builder.shooting_timer.clone())
            .insert((
                TowerBlazeMortar,
                BLAZE_MORTAR_LOB,
                LobPreview,
                building_info.world_sprite(),
                GridArea::new(builder.grid_position, building_info.grid_imprint),
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

fn on_tower_blaze_mortar_place_request_do_so(
    trigger: On<PlaceRequest>,
    mut commands: Commands,
    mut placement: BuildingPlacementManager,
) {
    let PlaceRequest(MapObject::Building(BuildingType::Tower(TowerType::BlazeMortar))) = *trigger else { return };
    let Some(coords) = placement.claim(BuildingType::Tower(TowerType::BlazeMortar)) else { return };
    commands.spawn(BuilderTowerBlazeMortar::new(coords));
}

#[log_tags(Tag::GameSave)]
fn collect_tower_blaze_mortars(
    towers: Query<(Entity, &GridArea, &IntegrityPoints, Has<DisabledByPlayer>, &TowerShootingTimer), With<TowerBlazeMortar>>,
    mut save: SaveWriter,
) {
    if towers.is_empty() { return; }

    #[debug_dev("Saving {} blaze mortars", rows.len())]
    let rows: Vec<(u32, GridCoords, f32, bool, f32)> = towers
        .iter()
        .map(|(entity, area, integrity_points, disabled_by_player, shooting_timer)| {
            (
                entity.index_u32(),
                area.origin,
                integrity_points.current,
                disabled_by_player,
                shooting_timer.0.elapsed_secs(),
            )
        })
        .collect();
    save.submit(move |ctx| {
        for (id, coords, integrity_points, disabled_by_player, shooting_elapsed) in rows {
            ctx.save_marker("tower_blaze_mortars", id)?;
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

fn load_tower_blaze_mortars(ctx: &mut LoadContext) -> LoadResult {
    ctx.for_each_entity("SELECT id FROM tower_blaze_mortars", |ctx, old_id, entity, _| {
        let grid_position = ctx.grid_coords(old_id)?;
        let integrity_points = ctx.integrity_points(old_id)?;
        let disabled_by_player = ctx.disabled_by_player(old_id)?;
        let shooting_elapsed = ctx.shooting_elapsed(old_id)?;
        let builder = BuilderTowerBlazeMortar::new(grid_position)
            .with_integrity_points(integrity_points)
            .with_disabled_by_player(disabled_by_player)
            .with_shooting_elapsed(shooting_elapsed);
        ctx.insert(entity, builder);
        Ok(())
    })
}

fn shooting_system(
    mut commands: Commands,
    clock: Res<GameClock>,
    mut tower_blaze_mortars: Query<(Entity, &Transform, &LobProfile, &mut TowerShootingTimer, &TowerWispTarget, &AttackDamage), (With<TowerBlazeMortar>, With<IsOperational>)>,
    wisps: Query<WispHeading, With<Wisp>>,
) {
    for (tower, transform, lob_profile, mut timer, target, attack_damage) in tower_blaze_mortars.iter_mut() {
        if !timer.0.is_finished() { continue; }
        let Some(Ok(wisp_heading)) = target.wisp().map(|wisp| wisps.get(wisp)) else { continue; };

        let lob = lob_profile.lob(transform.translation.xy(), wisp_heading.next_cell_center(), clock.now());
        commands.spawn(BuilderFireball::new(lob, *attack_damage).with_shooter(tower));
        timer.0.reset();
    }
}
