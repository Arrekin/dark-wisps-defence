use bevy::{
    platform::collections::HashMap,
    prelude::*,
};
use nanorand::Rng;

use alteration::{
    effects::prelude::*,
    modifiers::prelude::*,
};
use almanach::prelude::*;
use buildings::prelude::*;
use game_core::prelude::*;
use grids::{
    placement::{CellHighlight, PlacementValidity},
    prelude::*,
};
use hud::prelude::{IndicatorDisplay, IndicatorType, Indicators};
use logging::prelude::*;
use map_objects::{
    DarkOre,
    prelude::*,
};
use persistence::{
    prelude::*,
    rusqlite,
};
use resources::prelude::*;
use states::prelude::*;

use crate::common::*;

fn mining_complex_validator(_: MapObject, coords: GridCoords, imprint: GridImprint, map_data: &GridsCollectionParam) -> PlacementValidity {
    if !coords.is_imprint_in_bounds(&imprint, map_data.obstacle_grid.bounds()) {
        return PlacementValidity::Invalid;
    }
    if map_data.reserved_coords.any_reserved(coords, imprint) {
        return PlacementValidity::Invalid;
    }
    let mut has_ore = false;
    for cell in imprint.iter(coords) {
        let field = &map_data.obstacle_grid[cell];
        if field.has_structure() || field.is_within_quantum_field() {
            return PlacementValidity::Invalid;
        }
        if field.has_dark_ore() { has_ore = true; }
    }
    if !has_ore {
        return PlacementValidity::Invalid;
    }
    if !map_data.energy_supply_grid.is_imprint_powered(coords, imprint) {
        return PlacementValidity::ValidUnpowered;
    }
    PlacementValidity::Valid
}

fn mining_complex_annotator(_: MapObject, coords: GridCoords, imprint: GridImprint, validity: PlacementValidity, map_data: &GridsCollectionParam) -> Vec<(GridCoords, CellHighlight)> {
    match validity {
        PlacementValidity::Invalid => imprint.iter(coords)
            .filter(|c| {
                !c.is_in_bounds(map_data.obstacle_grid.bounds())
                    || map_data.obstacle_grid[*c].has_structure()
                    || map_data.obstacle_grid[*c].is_within_quantum_field()
            })
            .map(|c| (c, CellHighlight::Negative))
            .collect(),
        _ => imprint.iter_in_bounds(coords, map_data.obstacle_grid.bounds())
            .filter(|c| map_data.obstacle_grid[*c].has_dark_ore())
            .map(|c| (c, CellHighlight::Positive))
            .collect(),
    }
}

pub struct MiningComplexPlugin;
impl Plugin for MiningComplexPlugin {
    fn build(&self, app: &mut App) {
        let almanach_info = BuilderMiningComplex::almanach_info(app.world().resource::<AssetServer>());
        app
            .add_systems(Update, (
                mine_ore_system.run_if(in_state(GameState::Running)),
            ))
            .add_observer(BuilderMiningComplex::on_builder_add_spawn_mining_complex)
            .register_db_loader::<BuilderMiningComplex>(MapLoadingStage::SpawnMapElements)
            .register_db_saver(BuilderMiningComplex::on_game_save_collect_mining_complex)
            .register_building(BuildingType::MiningComplex, almanach_info)
            ;
    }
}


#[derive(Component)]
pub(crate) struct MiningComplexDeliveryTimer(pub Timer);

#[derive(Clone, Copy, Debug)]
pub(crate) struct MiningComplexSaveData {
    pub entity: Entity,
    pub integrity_points: f32,
    pub disabled_by_player: bool,
}

#[derive(Component, SSS)]
pub(crate) struct BuilderMiningComplex {
    pub grid_position: GridCoords,
    pub save_data: Option<MiningComplexSaveData>,
}
impl Saveable for BuilderMiningComplex {
    fn save(self, tx: &rusqlite::Transaction) -> rusqlite::Result<()> {
        let save_data = self.save_data.expect("BuilderMiningComplex for saving purpose must have save_data");
        let entity_index = save_data.entity.index_u32() as i64;

        tx.save_marker("mining_complexes", entity_index)?;
        tx.save_grid_coords(entity_index, self.grid_position)?;
        tx.save_integrity_points(entity_index, save_data.integrity_points)?;
        if save_data.disabled_by_player {
            tx.save_disabled_by_player(entity_index)?;
        }
        Ok(())
    }
}
impl Loadable for BuilderMiningComplex {
    fn load(ctx: &mut LoadContext) -> rusqlite::Result<LoadResult> {
        let mut stmt = ctx.conn.prepare("SELECT id FROM mining_complexes LIMIT ?1 OFFSET ?2")?;
        let mut rows = stmt.query(ctx.pagination.as_params())?;
        
        let mut count = 0;
        while let Some(row) = rows.next()? {
            let old_id: i64 = row.get(0)?;
            let grid_position = ctx.conn.get_grid_coords(old_id)?;
            let integrity_points = ctx.conn.get_integrity_points(old_id)?;
            let disabled_by_player = ctx.conn.get_disabled_by_player(old_id)?;
            
            if let Some(new_entity) = ctx.get_new_entity_for_old(old_id) {
                let save_data = MiningComplexSaveData { entity: new_entity, integrity_points, disabled_by_player };
                ctx.commands.entity(new_entity).insert(BuilderMiningComplex::new_for_saving(grid_position, save_data));
            } else {
                Log::warn().dev().tag(Tag::GameLoad).message(format!("MiningComplex with old ID {old_id} has no corresponding new entity"));
            }
            count += 1;
        }

        Ok(count.into())
    }
}
impl BuilderMiningComplex {
    pub fn almanach_info(asset_server: &AssetServer) -> BuildingInfo {
        BuildingInfo {
            name: "Mining Complex".to_string(),
            sprite: asset_server.load("buildings/mining_complex.png"),
            top_sprite: None,
            grid_imprint: GridImprint::Rectangle { width: 3, height: 3 },
            cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 100 }],
            baseline: HashMap::from([(ModifierType::MaxIntegrityPoints, 100.)]),
            validate: mining_complex_validator,
            annotate: mining_complex_annotator,
            place_emitter: building_place_emitter,
            remove_emitter: None,
            begin_placing_emitter: None,
            place_mode: PlacementMode::OnRelease,
            remove_mode: PlacementMode::OnRelease,
        }
    }

    pub fn new(grid_position: GridCoords) -> Self {
        Self { grid_position, save_data: None }
    }
    pub fn new_for_saving(grid_position: GridCoords, save_data: MiningComplexSaveData) -> Self {
        Self { grid_position, save_data: Some(save_data) }
    }

    fn on_game_save_collect_mining_complex(
        mut commands: Commands,
        mining_complexes: Query<(Entity, &GridCoords, &IntegrityPoints, Has<DisabledByPlayer>), With<MiningComplex>>,
    ) {
        if mining_complexes.is_empty() { return; }
        let batch = mining_complexes.iter().map(|(entity, coords, integrity_points, disabled_by_player)| {
            let save_data = MiningComplexSaveData {
                entity,
                integrity_points: integrity_points.get_current(),
                disabled_by_player,
            };
            BuilderMiningComplex::new_for_saving(*coords, save_data)
        }).collect::<SaveableBatchCommand<_>>();
        Log::debug().dev().tag(Tag::GameSave).message(format!("Saving {} mining complexes", batch.len()));
        commands.queue(batch);
    }

    pub fn on_builder_add_spawn_mining_complex(
        trigger: On<Add, BuilderMiningComplex>,
        mut commands: Commands,
        almanach: Res<Almanach>,
        builders: Query<&BuilderMiningComplex>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return; };
        
        let building_info = almanach.get_building_info(BuildingType::MiningComplex);
        let grid_imprint = building_info.grid_imprint;
        
        let mut entity_commands = commands.entity(entity);
        if let Some(save_data) = &builder.save_data {
            // Save data
            entity_commands.insert(IntegrityPoints::new(save_data.integrity_points));
            if save_data.disabled_by_player {
                entity_commands.insert(DisabledByPlayer);
            }
        }

        entity_commands
            .remove::<BuilderMiningComplex>()
            .insert((
                MiningComplex,
                Sprite {
                    image: building_info.sprite.clone(),
                    custom_size: Some(grid_imprint.world_size()),
                    ..Default::default()
                },
                builder.grid_position,
                grid_imprint,
                NeedsPower::default(),
                DarkOreAreaScanner{range_imprint: grid_imprint},
                MiningComplexDeliveryTimer(Timer::from_seconds(1.0, TimerMode::Repeating)),
                related![Indicators[
                    IndicatorType::NoPower,
                    IndicatorType::OreDepleted,
                    IndicatorType::DisabledByPlayer,
                ]],
                related![EffectInstances[
                    (ModifierContributions(building_info.baseline.clone()), BaselineEffect),
                ]],
                children![
                    IndicatorDisplay::default(),
                ],
            ));
    }
}

fn mine_ore_system(
    mut stock: ResMut<Stock>,
    time: Res<Time>,
    mut mining_complexes: Query<(&mut MiningComplexDeliveryTimer, &DarkOreInRange), (With<MiningComplex>, With<HasPower>, Without<DisabledByPlayer>)>,
    mut dark_ores: Query<&mut DarkOre>,
) {
    let mut rng = nanorand::tls_rng();
    for (mut timer, ore_in_range) in mining_complexes.iter_mut() {
        let ore_in_range = &ore_in_range.0;
        if ore_in_range.is_empty() { continue; }
        timer.0.tick(time.delta());
        if timer.0.just_finished() {
            let ore_index = rng.generate_range(0..ore_in_range.len());
            let ore_entity = ore_in_range[ore_index];
            if let Ok(mut dark_ore) = dark_ores.get_mut(ore_entity) {
                let mined_amount = std::cmp::min(dark_ore.amount, 100);
                stock.add(ResourceType::DarkOre, mined_amount);
                dark_ore.amount -= mined_amount;
            }
        }
    }
}