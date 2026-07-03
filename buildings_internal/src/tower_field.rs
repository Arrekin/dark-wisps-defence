use bevy::{
    platform::collections::HashMap,
    prelude::*,
};

use alteration::{
    effects::{
        prelude::*,
        slow::BuilderSlowEffect,
    },
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
use persistence::{
    prelude::*,
    rusqlite,
};
use resources::prelude::*;
use shards::prelude::*;
use states::prelude::*;
use weaponry::{
    force_field::{ForceFieldEntered, ForceFieldExited, GeneratedForceField},
    prelude::*,
};

use crate::common::*;


pub struct TowerFieldPlugin;
impl Plugin for TowerFieldPlugin {
    fn build(&self, app: &mut App) {
        let almanach_info = BuilderTowerField::almanach_info(app.world().resource::<AssetServer>());
        app
            .add_observer(BuilderTowerField::on_builder_add_spawn_tower_field)
            .add_observer(on_tower_field_despawn_shrink_orphaned_force_field)
            .register_db_loader::<BuilderTowerField>(MapLoadingStage::SpawnMapElements)
            .register_db_saver(BuilderTowerField::on_game_save_collect_tower_field)
            .register_building(BuildingType::Tower(TowerType::Field), almanach_info)
            ;
    }
}

const FIELD_RANGE_CELLS: f32 = 7.0;
const SLOW_AMOUNT: f32 = 40.0; // world units per second reduction in MovementSpeed

#[derive(Clone, Debug)]
pub(crate) struct TowerFieldSaveData {
    entity: Entity,
    integrity_points: f32,
    disabled_by_player: bool,
}

#[derive(Component, SSS)]
pub(crate) struct BuilderTowerField {
    grid_position: GridCoords,
    save_data: Option<TowerFieldSaveData>,
}

impl Saveable for BuilderTowerField {
    fn save(self, tx: &rusqlite::Transaction) -> rusqlite::Result<()> {
        let save_data = self.save_data.expect("BuilderTowerField for saving must have save_data");
        let entity_index = save_data.entity.index_u32() as i64;

        tx.save_marker("tower_fields", entity_index)?;
        tx.save_grid_coords(entity_index, self.grid_position)?;
        tx.save_integrity_points(entity_index, save_data.integrity_points)?;
        if save_data.disabled_by_player {
            tx.save_disabled_by_player(entity_index)?;
        }
        Ok(())
    }
}

impl Loadable for BuilderTowerField {
    fn load(ctx: &mut LoadContext) -> rusqlite::Result<LoadResult> {
        let mut stmt = ctx.conn.prepare("SELECT id FROM tower_fields LIMIT ?1 OFFSET ?2")?;
        let mut rows = stmt.query(ctx.pagination.as_params())?;

        let mut count = 0;
        while let Some(row) = rows.next()? {
            let old_id: i64 = row.get(0)?;
            let grid_position = ctx.conn.get_grid_coords(old_id)?;
            let integrity_points = ctx.conn.get_integrity_points(old_id)?;
            let disabled_by_player = ctx.conn.get_disabled_by_player(old_id)?;

            if let Some(new_entity) = ctx.get_new_entity_for_old(old_id) {
                let save_data = TowerFieldSaveData { entity: new_entity, integrity_points, disabled_by_player };
                ctx.commands.entity(new_entity).insert(BuilderTowerField::new_for_saving(grid_position, save_data));
            }
            count += 1;
        }
        Ok(count.into())
    }
}

impl BuilderTowerField {
    pub fn almanach_info(asset_server: &AssetServer) -> BuildingInfo {
        BuildingInfo {
            name: "Field Tower".to_string(),
            sprite: asset_server.load("buildings/tower_field.png"),
            top_sprite: None,
            grid_imprint: GridImprint::Plus { extents: 1 },
            cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 500 }],
            baseline: HashMap::from([
                (ModifierType::MaxIntegrityPoints, 100.),
                (ModifierType::AttackRange, FIELD_RANGE_CELLS),
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

    pub fn new(grid_position: GridCoords) -> Self { Self { grid_position, save_data: None } }
    pub fn new_for_saving(grid_position: GridCoords, save_data: TowerFieldSaveData) -> Self {
        Self { grid_position, save_data: Some(save_data) }
    }

    fn on_game_save_collect_tower_field(
        mut commands: Commands,
        towers: Query<(Entity, &GridCoords, &IntegrityPoints, Has<DisabledByPlayer>), With<TowerField>>,
    ) {
        if towers.is_empty() { return; }
        let batch = towers.iter().map(|(entity, coords, integrity_points, disabled_by_player)| {
            let save_data = TowerFieldSaveData {
                entity,
                integrity_points: integrity_points.get_current(),
                disabled_by_player,
            };
            BuilderTowerField::new_for_saving(*coords, save_data)
        }).collect::<SaveableBatchCommand<_>>();
        commands.queue(batch);
    }

    pub fn on_builder_add_spawn_tower_field(
        trigger: On<Add, BuilderTowerField>,
        mut commands: Commands,
        almanach: Res<Almanach>,
        builders: Query<&BuilderTowerField>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return; };

        let building_info = almanach.get_building_info(BuildingType::Tower(TowerType::Field));

        let mut entity_commands = commands.entity(entity);
        if let Some(save_data) = &builder.save_data {
            entity_commands.insert(IntegrityPoints::new(save_data.integrity_points));
            if save_data.disabled_by_player {
                entity_commands.insert(DisabledByPlayer);
            }
        }

        entity_commands
            .remove::<BuilderTowerField>()
            .insert((
                TowerField,
                Sprite {
                    image: building_info.sprite.clone(),
                    custom_size: Some(building_info.grid_imprint.world_size()),
                    ..Default::default()
                },
                builder.grid_position,
                building_info.grid_imprint,
                NeedsPower::default(),
                ShardSlots::new(2),
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
            .observe(ManageForceField::observe_and_trigger::<Insert, HasPower>(entity))
            .observe(ManageForceField::observe_and_trigger::<Remove, HasPower>(entity))
            .observe(ManageForceField::observe_and_trigger::<Insert, DisabledByPlayer>(entity))
            .observe(ManageForceField::observe_and_trigger::<Remove, DisabledByPlayer>(entity))
            .observe(Self::on_attack_range_change_resize_force_field)
            .observe(Self::on_shard_apply_do_so)
            .observe(ManageForceField::on_trigger_manage_force_field_state);
        commands.trigger(ManageForceField { entity });
    }

    fn on_attack_range_change_resize_force_field(
        trigger: On<Insert, AttackRange>,
        towers: Query<(Option<&GeneratedForceField>, &AttackRange), With<TowerField>>,
        mut fields: Query<&mut ForceField>,
    ) {
        let Ok((has_field, attack_range)) = towers.get(trigger.entity) else { return; };
        let new_radius = attack_range.get() * CELL_SIZE;
        let Some(has_field) = has_field else { return; }; // If it has field
        let Some(field_entity) = has_field.iter().next() else { return; }; // In 1:1 relation
        let Ok(mut field) = fields.get_mut(field_entity) else { return; }; // And the field's entity exists
        field.radius = new_radius;
    }

    fn on_shard_apply_do_so(
        trigger: On<ShardApplyEvent>,
        mut commands: Commands,
    ) {
        match trigger.shard_type {
            ShardType::Range => {
                commands.spawn(ShardEffect::from_modifiers(
                    trigger.shard_target,
                    HashMap::from([(ModifierType::AttackRange, 2.0)]),
                ));
            }
            ShardType::Damage | ShardType::Speed | ShardType::Fire | ShardType::Water | ShardType::Light | ShardType::Electric => {}
        }
    }

}

#[derive(EntityEvent)]
struct ManageForceField {
    entity: Entity,
}
impl ManageForceField {
    fn observe_and_trigger<E: Event, B: Bundle>(trigger_entity: Entity) -> impl Fn(On<E, B>, Commands) {
        move |_trigger: On<E, B>, mut commands: Commands| {
            commands.trigger(Self { entity: trigger_entity });
        }
    }

    fn on_trigger_manage_force_field_state(
        trigger: On<ManageForceField>,
        mut commands: Commands,
        towers: Query<(Option<&GeneratedForceField>, &AttackRange, &Transform, Has<HasPower>, Has<DisabledByPlayer>), With<TowerField>>,
    ) {
        let tower_entity = trigger.entity;
        let Ok((generated_field, attack_range, transform, has_power, is_disabled)) = towers.get(tower_entity) else { return; };

        if has_power && !is_disabled {
            if let Some(generated_field) = generated_field {
                let field_entity = generated_field.collection();
                commands.entity(*field_entity).insert(ForceFieldState::Growing);
            } else {
                let radius = attack_range.get() * CELL_SIZE;
                commands.spawn(BuilderForceField::new(radius, tower_entity, transform.translation))
                    .observe(Self::on_field_entered_apply_effect)
                    .observe(Self::on_field_exited_remove_effect)
                    .observe(Self::on_field_despawn_remove_all_effects);
            }
        } else if let Some(generated_field) = generated_field {
            let field_entity = generated_field.collection();
            commands.entity(*field_entity).insert(ForceFieldState::Shrinking);
        }
    }

    fn on_field_entered_apply_effect(
        trigger: On<ForceFieldEntered>,
        mut commands: Commands,
    ) {
        commands.spawn(
            BuilderSlowEffect::new(trigger.target, SLOW_AMOUNT).with_source(trigger.field),
        );
    }

    fn on_field_exited_remove_effect(
        trigger: On<ForceFieldExited>,
        mut commands: Commands,
        sources: Query<&EffectSourceOf>,
        effects: Query<&EffectTarget, With<FieldEffect>>,
    ) {
        let field_entity = trigger.field;
        let target_entity = trigger.target;
        let Ok(sourced) = sources.get(field_entity) else { return; };
        for effect_entity in sourced.iter() {
            let Ok(effect_target) = effects.get(effect_entity) else { continue; };
            if effect_target.0 == target_entity {
                commands.entity(effect_entity).despawn();
            }
        }
    }

    fn on_field_despawn_remove_all_effects(
        trigger: On<Despawn, ForceField>,
        mut commands: Commands,
        sources: Query<&EffectSourceOf>,
        effects: Query<(), With<FieldEffect>>,
    ) {
        let field_entity = trigger.entity;
        let Ok(sourced) = sources.get(field_entity) else { return; };
        // Despawn every FieldEffect this field spawned, regardless of which target it was on.
        for effect_entity in sourced.iter() {
            if effects.contains(effect_entity) {
                commands.entity(effect_entity).despawn();
            }
        }
    }
}

fn on_tower_field_despawn_shrink_orphaned_force_field(
    trigger: On<Despawn, TowerField>,
    mut commands: Commands,
    towers: Query<&GeneratedForceField>,
) {
    let tower_entity = trigger.entity;
    let Ok(generated_field) = towers.get(tower_entity) else { return; };
    let field_entity = generated_field.collection();
    // Begin shrinking the orphaned force field — it will self-despawn when progress reaches 0.
    commands.entity(*field_entity).insert(ForceFieldState::Shrinking);
}
