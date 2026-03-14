use lib_grid::grids::emissions::{EmissionsType, EmitterEnergy, EmitterEnergyEnabled};
use lib_grid::grids::energy_supply::SupplierEnergy;
use lib_grid::search::flooding::{FloodEmissionsDetails, FloodEmissionsEvaluator, FloodEmissionsMode};

use crate::prelude::*;
use crate::ui::indicators::{IndicatorDisplay, IndicatorType, Indicators};

pub struct EnergyRelayPlugin;
impl Plugin for EnergyRelayPlugin {
    fn build(&self, app: &mut App) {
        let almanach_info = BuilderEnergyRelay::almanach_info(app.world().resource::<AssetServer>());
        app
            .add_observer(BuilderEnergyRelay::on_add)
            .register_db_loader::<BuilderEnergyRelay>(MapLoadingStage::SpawnMapElements)
            .register_db_saver(BuilderEnergyRelay::on_game_save)
            .register_building(BuildingType::EnergyRelay, almanach_info)
            ;
    }
}

#[derive(Clone, Copy, Debug)]
pub struct EnergyRelaySaveData {
    pub entity: Entity,
    pub integrity_points: f32,
    pub disabled_by_player: bool,
}

#[derive(Component, SSS)]
pub struct BuilderEnergyRelay {
    pub grid_position: GridCoords,
    pub save_data: Option<EnergyRelaySaveData>,
}
impl Saveable for BuilderEnergyRelay {
    fn save(self, tx: &rusqlite::Transaction) -> rusqlite::Result<()> {
        let save_data = self.save_data.expect("BuilderEnergyRelay for saving purpose must have save_data");
        let entity_index = save_data.entity.index_u32() as i64;

        tx.save_marker("energy_relays", entity_index)?;
        tx.save_grid_coords(entity_index, self.grid_position)?;
        tx.save_integrity_points(entity_index, save_data.integrity_points)?;
        if save_data.disabled_by_player {
            tx.save_disabled_by_player(entity_index)?;
        }
        Ok(())
    }
}
impl Loadable for BuilderEnergyRelay {
    fn load(ctx: &mut LoadContext) -> rusqlite::Result<LoadResult> {
        let mut stmt = ctx.conn.prepare("SELECT id FROM energy_relays LIMIT ?1 OFFSET ?2")?;
        let mut rows = stmt.query(ctx.pagination.as_params())?;
        
        let mut count = 0;
        while let Some(row) = rows.next()? {
            let old_id: i64 = row.get(0)?;
            let grid_position = ctx.conn.get_grid_coords(old_id)?;
            let integrity_points = ctx.conn.get_integrity_points(old_id)?;
            let disabled_by_player = ctx.conn.get_disabled_by_player(old_id)?;
            
            if let Some(new_entity) = ctx.get_new_entity_for_old(old_id) {
                let save_data = EnergyRelaySaveData { entity: new_entity, integrity_points, disabled_by_player };
                ctx.commands.entity(new_entity).insert(BuilderEnergyRelay::new_for_saving(grid_position, save_data));
            } else {
                Log::warn().dev().tag(Tag::GameLoad).message(format!("EnergyRelay with old ID {old_id} has no corresponding new entity"));
            }
            count += 1;
        }

        Ok(count.into())
    }
}
impl BuilderEnergyRelay {
    pub fn almanach_info(asset_server: &AssetServer) -> BuildingInfo {
        BuildingInfo {
            name: "Energy Relay".to_string(),
            sprite: asset_server.load("buildings/energy_relay.png"),
            top_sprite: None,
            grid_imprint: GridImprint::Rectangle { width: 2, height: 2 },
            cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 300 }],
            baseline: HashMap::from([
                (ModifierType::MaxIntegrityPoints, 100.),
                (ModifierType::EnergySupplyRange, 12.),
            ]),
            validate: building_validator,
            annotate: annotate_non_empty,
        }
    }

    pub fn new(grid_position: GridCoords) -> Self {
        Self { grid_position, save_data: None }
    }
    pub fn new_for_saving(grid_position: GridCoords, save_data: EnergyRelaySaveData) -> Self {
        Self { grid_position, save_data: Some(save_data) }
    }

    fn on_game_save(
        mut commands: Commands,
        relays: Query<(Entity, &GridCoords, &IntegrityPoints, Has<DisabledByPlayer>), With<EnergyRelay>>,
    ) {
        if relays.is_empty() { return; }
        let batch = relays.iter().map(|(entity, coords, integrity_points, disabled_by_player)| {
            let save_data = EnergyRelaySaveData {
                entity,
                integrity_points: integrity_points.get_current(),
                disabled_by_player,
            };
            BuilderEnergyRelay::new_for_saving(*coords, save_data)
        }).collect::<SaveableBatchCommand<_>>();
        Log::debug().dev().tag(Tag::GameSave).message(format!("Saving {} energy relays", batch.len()));
        commands.queue(batch);
    }

    pub fn on_add(
        trigger: On<Add, BuilderEnergyRelay>,
        mut commands: Commands,
        builders: Query<&BuilderEnergyRelay>,
        almanach: Res<Almanach>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return; };

        let building_info = almanach.get_building_info(BuildingType::EnergyRelay);

        let mut entity_commands = commands.entity(entity);
        if let Some(save_data) = &builder.save_data {
            // Save data
            entity_commands.insert(IntegrityPoints::new(save_data.integrity_points));
            if save_data.disabled_by_player {
                entity_commands.insert(DisabledByPlayer);
            }
        }

        entity_commands
            .remove::<BuilderEnergyRelay>()
            .insert((
                EnergyRelay,
                Sprite {
                    image: building_info.sprite.clone(),
                    custom_size: Some(building_info.grid_imprint.world_size()),
                    color: Color::hsla(0., 0.2, 1.0, 1.0), // 1.6 is a good value if the pulsation is off.
                    ..Default::default()
                },
                builder.grid_position,
                building_info.grid_imprint,
                NeedsPower::default(),
                EmitterEnergy(FloodEmissionsDetails {
                    emissions_type: EmissionsType::Energy,
                    range: usize::MAX,
                    evaluator: FloodEmissionsEvaluator::ExponentialDecay{start_value: 100., decay: 0.1},
                    mode: FloodEmissionsMode::Increase,
                }),
                SupplierEnergy,
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
            .observe(|trigger: On<Insert, HasPower>, mut commands: Commands| { commands.trigger(RequestTechnicalStateUpdate{ entity: trigger.entity }); })
            .observe(|trigger: On<Insert, NoPower>, mut commands: Commands| { commands.trigger(RequestTechnicalStateUpdate{ entity: trigger.entity }); })
            .observe(|trigger: On<Insert, DisabledByPlayer>, mut commands: Commands| { commands.trigger(RequestTechnicalStateUpdate{ entity: trigger.entity }); })
            .observe(|trigger: On<Remove, DisabledByPlayer>, mut commands: Commands| { commands.trigger(RequestTechnicalStateUpdate{ entity: trigger.entity }); })
            .observe(RequestTechnicalStateUpdate::on_trigger)
            ;
        commands.trigger(RequestTechnicalStateUpdate{ entity });
    }
}

#[derive(EntityEvent)]
struct RequestTechnicalStateUpdate { entity: Entity }
impl RequestTechnicalStateUpdate {
    fn on_trigger(
        trigger: On<RequestTechnicalStateUpdate>,
        mut commands: Commands,
        relays: Query<(Has<DisabledByPlayer>, Has<NoPower>), With<EnergyRelay>>,
    ) {
        let entity = trigger.entity;
        let Ok((has_disabled_by_player, has_no_power)) = relays.get(entity) else { return; };
        let mut entity_commands = commands.entity(entity);
        if has_disabled_by_player {
            entity_commands.remove::<SupplierEnergy>().remove::<EmitterEnergyEnabled>().remove::<ColorPulsation>();
        }
        else if has_no_power {
            entity_commands.remove::<EmitterEnergyEnabled>().remove::<ColorPulsation>().try_insert(SupplierEnergy);
        } else {
            entity_commands.try_insert(SupplierEnergy).try_insert(EmitterEnergyEnabled).try_insert(ColorPulsation::new(1.0, 1.8, 3.0));
        }
    }
}