//! # Crucible Tower
//!
//! The Field tower's fire evolution: a dome of heat. It generates a `CrucibleField`, which shares
//! space with every other field.

use bevy::{
    platform::collections::HashMap,
    prelude::*,
};

use almanach::{BuildingInfo, ObjectPresentation, prelude::*};
use alteration::{
    effects::prelude::*,
    modifiers::prelude::*,
};
use buildings::prelude::*;
use fields::{
    field::GeneratedField,
    prelude::*,
};
use game_core::prelude::*;
use grids::placement::{annotate_non_empty, PlacementModes, PlaceRequest};
use hud::prelude::{IndicatorDisplay, IndicatorType, Indicators};
use logging::prelude::*;
use persistence::prelude::*;
use resources::prelude::*;
use shards::prelude::*;
use states::prelude::*;

use crate::{common::*, tooltip::building_tooltip};

pub(crate) struct TowerCruciblePlugin;
impl Plugin for TowerCruciblePlugin {
    fn build(&self, app: &mut App) {
        let almanach_info = BuilderTowerCrucible::almanach_info(app.world().resource::<AssetServer>());
        app
            .add_observer(BuilderTowerCrucible::on_builder_add_spawn_tower_crucible)
            .add_observer(on_tower_crucible_place_request_do_so)
            .add_systems(CollectSave, collect_tower_crucibles)
            .register_loader(MapLoadingStage::SpawnMapElements, "tower_crucibles", load_tower_crucibles)
            .register_building(BuildingType::Tower(TowerType::Crucible), almanach_info);
    }
}

// Field size
const FIELD_RANGE_CELLS: f32 = 7.0;
/// Extra radius in cells. Half a cell aligns the rim with cell edges along the grid axes.
const FIELD_RIM_OFFSET_CELLS: f32 = 0.5;

#[derive(Component, SSS)]
pub(crate) struct BuilderTowerCrucible {
    pub grid_position: GridCoords,
    /// Custom integrity points, or `None` to use the fresh-spawn baseline.
    pub integrity_points: Option<IntegrityPoints>,
    /// Set when the player disabled this building. `None` on fresh spawn.
    pub disabled_by_player: Option<DisabledByPlayer>,
}

impl BuilderTowerCrucible {
    pub fn almanach_info(asset_server: &AssetServer) -> BuildingInfo {
        BuildingInfo {
            name: "Crucible".to_string(),
            description: "Projects a dome of heat. Has no effect yet.".to_string(),
            sprite: asset_server.load("buildings/tower_field.png"),
            top_sprite: None,
            grid_imprint: GridImprint::Plus { extents: 1 },
            cost: vec![ResourceAmount::new(ResourceType::DarkOre, 500)],
            baseline: HashMap::from([
                (ModifierType::MaxIntegrityPoints, 100.),
                (ModifierType::AttackRange, FIELD_RANGE_CELLS),
            ]),
            sockets: vec![
                ("field_range".into(), ShardSocket::new(ShardType::Reach, "Field range", ModifierType::AttackRange, [2., 4., 6.])),
            ],
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

    pub fn on_builder_add_spawn_tower_crucible(
        trigger: On<Add<BuilderTowerCrucible>>,
        mut commands: Commands,
        almanach: Res<Almanach>,
        builders: Query<&BuilderTowerCrucible>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return; };

        let building_info = almanach.get_building_info(BuildingType::Tower(TowerType::Crucible));

        commands.entity(entity)
            .remove::<BuilderTowerCrucible>()
            .insert_some(builder.integrity_points)
            .insert_some(builder.disabled_by_player)
            .insert((
                TowerCrucible,
                Sprite {
                    image: building_info.sprite.clone(),
                    custom_size: Some(building_info.grid_imprint.world_size()),
                    ..default()
                },
                builder.grid_position,
                building_info.grid_imprint,
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
            .observe(Self::on_insert_attack_range_resize_crucible_field)
            .observe(on_technical_state_changed_recompute_operational)
            .observe(Self::on_add_is_operational_unfold_crucible_field)
            .observe(Self::on_remove_is_operational_fold_crucible_field);
        let field_position = builder.grid_position.to_world_position_centered(building_info.grid_imprint);
        commands.spawn(BuilderCrucibleField::new(entity, field_position));
        for (content_id, socket) in &building_info.sockets {
            commands.trigger(ShardSocketUpsert::new(entity, content_id.clone(), socket.clone()));
        }
        commands.trigger(TechnicalStateChanged { entity, kind: TechnicalChange::JustSpawned });
    }

    fn field_radius(attack_range: &AttackRange) -> f32 {
        (attack_range.get() + FIELD_RIM_OFFSET_CELLS) * CELL_SIZE
    }

    fn on_insert_attack_range_resize_crucible_field(
        trigger: On<Insert<AttackRange>>,
        towers: Query<(&GeneratedField, &AttackRange), With<IsOperational>>,
        mut fields: Query<&mut Field>,
    ) {
        let Ok((generated_field, attack_range)) = towers.get(trigger.entity) else { return; };
        let Ok(mut field) = fields.get_mut(generated_field.field()) else { return; };
        field.target_radius = Self::field_radius(attack_range);
    }

    fn on_add_is_operational_unfold_crucible_field(
        trigger: On<Add<IsOperational>>,
        towers: Query<(&GeneratedField, &AttackRange)>,
        mut fields: Query<&mut Field>,
    ) {
        let Ok((generated_field, attack_range)) = towers.get(trigger.entity) else { return; };
        let Ok(mut field) = fields.get_mut(generated_field.field()) else { return; };
        field.target_radius = Self::field_radius(attack_range);
    }

    fn on_remove_is_operational_fold_crucible_field(
        trigger: On<Remove<IsOperational>>,
        towers: Query<&GeneratedField>,
        mut fields: Query<&mut Field>,
    ) {
        let Ok(generated_field) = towers.get(trigger.entity) else { return; };
        let Ok(mut field) = fields.get_mut(generated_field.field()) else { return; };
        field.target_radius = 0.;
    }
}

fn on_tower_crucible_place_request_do_so(
    trigger: On<PlaceRequest>,
    mut commands: Commands,
    mut placement: BuildingPlacementManager,
) {
    let PlaceRequest(MapObject::Building(BuildingType::Tower(TowerType::Crucible))) = *trigger else { return };
    let Some(coords) = placement.claim(BuildingType::Tower(TowerType::Crucible)) else { return };
    commands.spawn(BuilderTowerCrucible::new(coords));
}

#[log_tags(Tag::GameSave)]
fn collect_tower_crucibles(
    towers: Query<(Entity, &GridCoords, &IntegrityPoints, Has<DisabledByPlayer>), With<TowerCrucible>>,
    mut save: SaveWriter,
) {
    if towers.is_empty() { return; }

    #[debug_dev("Saving {} tower crucibles", rows.len())]
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
            ctx.save_marker("tower_crucibles", id)?;
            ctx.save_grid_coords(id, coords)?;
            ctx.save_integrity_points(id, integrity_points)?;
            if disabled_by_player {
                ctx.save_disabled_by_player(id)?;
            }
        }
        Ok(())
    });
}

fn load_tower_crucibles(ctx: &mut LoadContext) -> LoadResult {
    ctx.for_each_entity("SELECT id FROM tower_crucibles", |ctx, old_id, entity, _| {
        let grid_position = ctx.grid_coords(old_id)?;
        let integrity_points = ctx.integrity_points(old_id)?;
        let disabled_by_player = ctx.disabled_by_player(old_id)?;
        let builder = BuilderTowerCrucible::new(grid_position)
            .with_integrity_points(integrity_points)
            .with_disabled_by_player(disabled_by_player);
        ctx.insert(entity, builder);
        Ok(())
    })
}
