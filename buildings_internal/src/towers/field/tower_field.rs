use bevy::{
    platform::collections::HashMap,
    prelude::*,
};

use almanach::{BuildingInfo, prelude::*};
use alteration::{
    effects::{
        prelude::*,
        slow::BuilderSlowEffect,
    },
    modifiers::prelude::*,
};
use buildings::prelude::*;
use fields::{
    field::{FieldEntered, FieldExited, GeneratedField},
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

use crate::common::*;

pub(crate) struct TowerFieldPlugin;
impl Plugin for TowerFieldPlugin {
    fn build(&self, app: &mut App) {
        let almanach_info = BuilderTowerField::almanach_info(app.world().resource::<AssetServer>());
        app
            .add_observer(BuilderTowerField::on_builder_add_spawn_tower_field)
            .add_observer(on_tower_field_place_request_do_so)
            .add_systems(CollectSave, collect_tower_fields)
            .register_loader(MapLoadingStage::SpawnMapElements, "tower_fields", load_tower_fields)
            .register_building(BuildingType::Tower(TowerType::Field), almanach_info);
    }
}

// Field size
/// Radius in cells. The half-cell fraction aligns the rim with cell borders along the grid axes.
const FIELD_RANGE_CELLS: f32 = 7.5;

// Slow effect
/// Reduction in a wisp's `MovementSpeed`, in world pixels per second.
const SLOW_AMOUNT: f32 = 40.0;

#[derive(Component, SSS)]
pub(crate) struct BuilderTowerField {
    pub grid_position: GridCoords,
    /// Custom integrity points, or `None` to use the fresh-spawn baseline.
    pub integrity_points: Option<IntegrityPoints>,
    /// Set when the player disabled this building. `None` on fresh spawn.
    pub disabled_by_player: Option<DisabledByPlayer>,
}

impl BuilderTowerField {
    pub fn almanach_info(asset_server: &AssetServer) -> BuildingInfo {
        BuildingInfo {
            name: "Field Tower".to_string(),
            description: "Projects a field that slows every wisp inside it. Deals no damage.".to_string(),
            sprite: asset_server.load("buildings/tower_field.png"),
            top_sprite: None,
            grid_imprint: GridImprint::Plus { extents: 1 },
            attack_range: AttackRangeType(RangeType::Circle).into(),
            cost: vec![ResourceAmount::new(ResourceType::DarkOre, 500)],
            baseline: HashMap::from([
                (ModifierType::MaxIntegrityPoints, 100.),
                (ModifierType::MaxAttackRange, FIELD_RANGE_CELLS),
            ]),
            sockets: vec![
                ("field_range".into(), ShardSocket::new(ShardType::Reach, "Field range", ModifierType::MaxAttackRange, [2., 4., 6.])),
                ("slow_strength".into(), ShardSocket::without_contributions(ShardType::Strength, "Slow strength (no effect yet)")),
            ],
            validate: building_validator,
            annotate: annotate_non_empty,
            placement: PlacementModes::default(),
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

    pub fn on_builder_add_spawn_tower_field(
        trigger: On<Add<BuilderTowerField>>,
        mut commands: Commands,
        almanach: Res<Almanach>,
        builders: Query<&BuilderTowerField>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return; };

        let building_info = almanach.get_building_info(BuildingType::Tower(TowerType::Field));

        let area = GridArea::new(builder.grid_position, building_info.grid_imprint);
        commands.entity(entity)
            .remove::<BuilderTowerField>()
            .insert_some(building_info.attack_range)
            .insert_some(builder.integrity_points)
            .insert_some(builder.disabled_by_player)
            .insert((
                TowerField,
                building_info.world_sprite(),
                area,
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
            .observe(Self::on_insert_max_attack_range_resize_force_field)
            .observe(on_technical_state_changed_recompute_operational)
            .observe(Self::on_add_is_operational_unfold_force_field)
            .observe(Self::on_remove_is_operational_fold_force_field);
        commands.spawn(BuilderForceField::new(entity, area.world_center()))
            .observe(Self::on_field_entered_apply_effect)
            .observe(Self::on_field_exited_remove_effect);
        for (content_id, socket) in &building_info.sockets {
            commands.trigger(ShardSocketUpsert::new(entity, content_id.clone(), socket.clone()));
        }
        commands.trigger(TechnicalStateChanged { entity, kind: TechnicalChange::JustSpawned });
    }

    fn on_insert_max_attack_range_resize_force_field(
        trigger: On<Insert<MaxAttackRange>>,
        towers: Query<(&GeneratedField, &MaxAttackRange), With<IsOperational>>,
        mut fields: Query<&mut Field>,
    ) {
        let Ok((generated_field, attack_range)) = towers.get(trigger.entity) else { return; };
        let Ok(mut field) = fields.get_mut(generated_field.field()) else { return; };
        field.target_radius = attack_range.in_world_units();
    }

    fn on_add_is_operational_unfold_force_field(
        trigger: On<Add<IsOperational>>,
        towers: Query<(&GeneratedField, &MaxAttackRange)>,
        mut fields: Query<&mut Field>,
    ) {
        let Ok((generated_field, attack_range)) = towers.get(trigger.entity) else { return; };
        let Ok(mut field) = fields.get_mut(generated_field.field()) else { return; };
        field.target_radius = attack_range.in_world_units();
    }

    fn on_remove_is_operational_fold_force_field(
        trigger: On<Remove<IsOperational>>,
        towers: Query<&GeneratedField>,
        mut fields: Query<&mut Field>,
    ) {
        let Ok(generated_field) = towers.get(trigger.entity) else { return; };
        let Ok(mut field) = fields.get_mut(generated_field.field()) else { return; };
        field.target_radius = 0.;
    }

    fn on_field_entered_apply_effect(
        trigger: On<FieldEntered>,
        mut commands: Commands,
    ) {
        commands.spawn(
            BuilderSlowEffect::new(trigger.target, SLOW_AMOUNT).with_source(trigger.field),
        );
    }

    fn on_field_exited_remove_effect(
        trigger: On<FieldExited>,
        mut commands: Commands,
        sources: Query<&EffectSourceOf>,
        effects: Query<&EffectTarget, With<FieldEffect>>,
    ) {
        let Ok(sourced) = sources.get(trigger.field) else { return; };
        for effect_entity in sourced.iter() {
            if effects.get(effect_entity).is_ok_and(|effect_target| effect_target.0 == trigger.target) {
                commands.entity(effect_entity).despawn();
            }
        }
    }
}

fn on_tower_field_place_request_do_so(
    trigger: On<PlaceRequest>,
    mut commands: Commands,
    mut placement: BuildingPlacementManager,
) {
    let PlaceRequest(MapObject::Building(BuildingType::Tower(TowerType::Field))) = *trigger else { return };
    let Some(coords) = placement.claim(BuildingType::Tower(TowerType::Field)) else { return };
    commands.spawn(BuilderTowerField::new(coords));
}

#[log_tags(Tag::GameSave)]
fn collect_tower_fields(
    towers: Query<(Entity, &GridArea, &IntegrityPoints, Has<DisabledByPlayer>), With<TowerField>>,
    mut save: SaveWriter,
) {
    if towers.is_empty() { return; }

    #[debug_dev("Saving {} tower fields", rows.len())]
    let rows: Vec<(u32, GridCoords, f32, bool)> = towers
        .iter()
        .map(|(entity, area, integrity_points, disabled_by_player)| {
            (
                entity.index_u32(),
                area.origin,
                integrity_points.current,
                disabled_by_player,
            )
        })
        .collect();
    save.submit(move |ctx| {
        for (id, coords, integrity_points, disabled_by_player) in rows {
            ctx.save_marker("tower_fields", id)?;
            ctx.save_grid_coords(id, coords)?;
            ctx.save_integrity_points(id, integrity_points)?;
            if disabled_by_player {
                ctx.save_disabled_by_player(id)?;
            }
        }
        Ok(())
    });
}

fn load_tower_fields(ctx: &mut LoadContext) -> LoadResult {
    ctx.for_each_entity("SELECT id FROM tower_fields", |ctx, old_id, entity, _| {
        let grid_position = ctx.grid_coords(old_id)?;
        let integrity_points = ctx.integrity_points(old_id)?;
        let disabled_by_player = ctx.disabled_by_player(old_id)?;
        let builder = BuilderTowerField::new(grid_position)
            .with_integrity_points(integrity_points)
            .with_disabled_by_player(disabled_by_player);
        ctx.insert(entity, builder);
        Ok(())
    })
}
