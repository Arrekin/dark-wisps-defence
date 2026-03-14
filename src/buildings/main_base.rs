use lib_grid::grids::emissions::{EmissionsType, EmitterEnergy};
use lib_grid::grids::energy_supply::{GeneratorEnergy, SupplierEnergy};
use lib_grid::search::flooding::{FloodEmissionsDetails, FloodEmissionsEvaluator, FloodEmissionsMode};

use crate::prelude::*;


pub struct MainBasePlugin;
impl Plugin for MainBasePlugin {
    fn build(&self, app: &mut App) {
        let almanach_info = BuilderMainBase::almanach_info(app.world().resource::<AssetServer>());
        app
            .add_observer(BuilderMainBase::on_add)
            .register_db_loader::<BuilderMainBase>(MapLoadingStage::SpawnMapElements)
            .register_db_saver(BuilderMainBase::on_game_save)
            .register_building(BuildingType::MainBase, almanach_info)
            ;
    }
}



#[derive(Clone, Copy, Debug)]
pub struct MainBaseSaveData {
    pub entity: Entity,
    pub integrity_points: f32,
}

#[derive(Component, SSS)]
pub struct BuilderMainBase {
    pub grid_position: GridCoords,
    pub save_data: Option<MainBaseSaveData>,
}
impl Saveable for BuilderMainBase {
    fn save(self, tx: &rusqlite::Transaction) -> rusqlite::Result<()> {
        let save_data = self.save_data.expect("BuilderMainBase for saving purpose must have save_data");
        let entity_index = save_data.entity.index_u32() as i64;

        tx.save_marker("main_bases", entity_index)?;
        tx.save_grid_coords(entity_index, self.grid_position)?;
        tx.save_integrity_points(entity_index, save_data.integrity_points)?;
        Ok(())
    }
}
impl Loadable for BuilderMainBase {
    fn load(ctx: &mut LoadContext) -> rusqlite::Result<LoadResult> {
        let mut stmt = ctx.conn.prepare("SELECT id FROM main_bases LIMIT ?1 OFFSET ?2")?;
        let mut rows = stmt.query(ctx.pagination.as_params())?;
        
        let mut count = 0;
        while let Some(row) = rows.next()? {
            let old_id: i64 = row.get(0)?;
            let grid_position = ctx.conn.get_grid_coords(old_id)?;
            let integrity_points = ctx.conn.get_integrity_points(old_id)?;
            
            if let Some(new_entity) = ctx.get_new_entity_for_old(old_id) {
                let save_data = MainBaseSaveData { entity: new_entity, integrity_points };
                ctx.commands.entity(new_entity).insert(BuilderMainBase::new_for_saving(grid_position, save_data));
            } else {
                Log::warn().dev().tag(Tag::GameLoad).message(format!("MainBase with old ID {old_id} has no corresponding new entity"));
            }
            count += 1;
        }

        Ok(count.into())
    }
}
impl BuilderMainBase {
    pub fn almanach_info(asset_server: &AssetServer) -> BuildingInfo {
        BuildingInfo {
            name: "Main Base".to_string(),
            sprite: asset_server.load("buildings/main_base.png"),
            top_sprite: None,
            grid_imprint: GridImprint::Rectangle { width: 6, height: 6 },
            cost: vec![],
            baseline: HashMap::from([
                (ModifierType::MaxIntegrityPoints, 10000.),
                (ModifierType::EnergySupplyRange, 15.),
            ]),
            validate: building_validator,
            annotate: annotate_non_empty,
        }
    }

    pub fn new_for_saving(grid_position: GridCoords, save_data: MainBaseSaveData) -> Self {
        Self { grid_position, save_data: Some(save_data) }
    }

    fn on_game_save(
        mut commands: Commands,
        main_base: Query<(Entity, &GridCoords, &IntegrityPoints), With<MainBase>>,
    ) {
        if let Ok((entity, coords, integrity_points)) = main_base.single() {
            let save_data = MainBaseSaveData {
                entity,
                integrity_points: integrity_points.get_current(),
            };
            commands.queue(SaveableBatchCommand::from_single(BuilderMainBase::new_for_saving(*coords, save_data)));
        }
    }

    pub fn on_add(
        trigger: On<Add, BuilderMainBase>,
        mut commands: Commands,
        builders: Query<&BuilderMainBase>,
        almanach: Res<Almanach>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return; };

        let building_info = almanach.get_building_info(BuildingType::MainBase);
        let grid_imprint = building_info.grid_imprint;
        
        let mut entity_commands = commands.entity(entity);
        if let Some(save_data) = &builder.save_data {
            // Save data
            entity_commands
                .insert((
                    IntegrityPoints::new(save_data.integrity_points),
                ));
        }
        // Common
        entity_commands
            .remove::<BuilderMainBase>()
            .insert((
                Sprite {
                    image: building_info.sprite.clone(),
                    custom_size: Some(grid_imprint.world_size()),
                    ..Default::default()
                },
                MainBase,
                builder.grid_position,
                grid_imprint,
                EmitterEnergy(FloodEmissionsDetails {
                    emissions_type: EmissionsType::Energy,
                    range: usize::MAX,
                    evaluator: FloodEmissionsEvaluator::ExponentialDecay { start_value: 100., decay: 0.1 },
                    mode: FloodEmissionsMode::Increase,
                }),
                GeneratorEnergy,
                SupplierEnergy,
                related![EffectInstances[
                    (ModifierContributions(building_info.baseline.clone()), BaselineEffect),
                ]],
            ));
    }
}