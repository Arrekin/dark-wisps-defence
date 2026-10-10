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
use weaponry::{
    force_field::{ForceFieldEntered, ForceFieldExited, GeneratedForceField},
    prelude::*,
};

use crate::{common::*, tooltip::building_tooltip};


pub(crate) struct TowerFieldPlugin;
impl Plugin for TowerFieldPlugin {
    fn build(&self, app: &mut App) {
        let almanach_info = BuilderTowerField::almanach_info(app.world().resource::<AssetServer>());
        app
            .add_observer(BuilderTowerField::on_builder_add_spawn_tower_field)
            .add_observer(on_tower_field_place_request_do_so)
            .add_observer(on_tower_field_despawn_shrink_orphaned_force_field)
            .add_systems(CollectSave, collect_tower_fields)
            .register_loader(MapLoadingStage::SpawnMapElements, "tower_fields", load_tower_fields)
            .register_building(BuildingType::Tower(TowerType::Field), almanach_info);
    }
}

const FIELD_RANGE_CELLS: f32 = 7.0;
const SLOW_AMOUNT: f32 = 40.0; // world units per second reduction in MovementSpeed

#[derive(Component, SSS)]
pub(crate) struct BuilderTowerField {
    pub grid_position: GridCoords,
    /// Saved integrity points. `None` ⇒ defer to baseline (fresh spawn);
    /// `Some` ⇒ override with saved value (restore).
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
            cost: vec![ResourceAmount::new(ResourceType::DarkOre, 500)],
            baseline: HashMap::from([
                (ModifierType::MaxIntegrityPoints, 100.),
                (ModifierType::AttackRange, FIELD_RANGE_CELLS),
            ]),
            sockets: vec![
                ("field_range".into(), ShardSocket::new(ShardType::Reach, "Field range", ModifierType::AttackRange, [2., 4., 6.])),
                ("slow_strength".into(), ShardSocket::without_contributions(ShardType::Strength, "Slow strength (no effect yet)")),
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

    pub fn on_builder_add_spawn_tower_field(
        trigger: On<Add<BuilderTowerField>>,
        mut commands: Commands,
        almanach: Res<Almanach>,
        builders: Query<&BuilderTowerField>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return; };

        let building_info = almanach.get_building_info(BuildingType::Tower(TowerType::Field));

        commands.entity(entity)
            .remove::<BuilderTowerField>()
            .insert_some(builder.integrity_points)
            .insert_some(builder.disabled_by_player)
            .insert((
                TowerField,
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
            .observe(Self::on_insert_attack_range_resize_force_field)
            .observe(on_technical_state_changed_recompute_operational)
            .observe(Self::on_add_is_operational_grow_force_field)
            .observe(Self::on_remove_is_operational_shrink_force_field);
        for (content_id, socket) in &building_info.sockets {
            commands.trigger(ShardSocketUpsert::new(entity, content_id.clone(), socket.clone()));
        }
        commands.trigger(TechnicalStateChanged { entity, kind: TechnicalChange::JustSpawned });
    }

    fn on_insert_attack_range_resize_force_field(
        trigger: On<Insert<AttackRange>>,
        towers: Query<(Option<&GeneratedForceField>, &AttackRange), With<TowerField>>,
        mut fields: Query<&mut ForceField>,
    ) {
        let Ok((generated_field, attack_range)) = towers.get(trigger.entity) else { return; };
        let Some(generated_field) = generated_field else { return; };
        let Ok(mut field) = fields.get_mut(generated_field.field()) else { return; };
        field.radius = attack_range.get() * CELL_SIZE;
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
    towers: Query<(Entity, &GridCoords, &IntegrityPoints, Has<DisabledByPlayer>), With<TowerField>>,
    mut save: SaveWriter,
) {
    if towers.is_empty() { return; }

    #[debug_dev("Saving {} tower fields", rows.len())]
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

impl BuilderTowerField {
    fn on_add_is_operational_grow_force_field(
        trigger: On<Add<IsOperational>>,
        mut commands: Commands,
        towers: Query<(Option<&GeneratedForceField>, &AttackRange, &Transform), With<TowerField>>,
    ) {
        let tower_entity = trigger.entity;
        let Ok((generated_field, attack_range, transform)) = towers.get(tower_entity) else { return; };

        if let Some(generated_field) = generated_field {
            commands.entity(generated_field.field()).insert(ForceFieldState::Growing);
        } else {
            let radius = attack_range.get() * CELL_SIZE;
            commands.spawn(BuilderForceField::new(radius, tower_entity, transform.translation))
                .observe(Self::on_field_entered_apply_effect)
                .observe(Self::on_field_exited_remove_effect)
                .observe(Self::on_field_despawn_remove_all_effects);
        }
    }

    fn on_remove_is_operational_shrink_force_field(
        trigger: On<Remove<IsOperational>>,
        mut commands: Commands,
        towers: Query<&GeneratedForceField, With<TowerField>>,
    ) {
        let Ok(generated_field) = towers.get(trigger.entity) else { return; };
        commands.entity(generated_field.field()).insert(ForceFieldState::Shrinking);
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
        trigger: On<Despawn<ForceField>>,
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
    trigger: On<Despawn<TowerField>>,
    mut commands: Commands,
    towers: Query<&GeneratedForceField>,
) {
    let tower_entity = trigger.entity;
    let Ok(generated_field) = towers.get(tower_entity) else { return; };
    // Begin shrinking the orphaned force field — it will self-despawn when progress reaches 0.
    commands.entity(generated_field.field()).insert(ForceFieldState::Shrinking);
}
