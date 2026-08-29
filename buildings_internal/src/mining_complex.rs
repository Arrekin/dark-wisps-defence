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
use grids::placement::{CellHighlight, GridsCollectionParam, PlacementChannel, PlacementValidity};
use hud::prelude::{IndicatorDisplay, IndicatorType, Indicators};
use logging::prelude::*;
use map_objects::{
    DarkOre,
    prelude::*,
};
use persistence::{
    prelude::{AppGameLoadSaveExtension, CollectSave, GameDbHelpers, LoadContext, SaveWriter},
    rusqlite,
};
use resources::prelude::*;
use states::prelude::*;

use crate::common::*;

fn mining_complex_validator(_: MapObject, origin: GridCoords, imprint: GridImprint, map_data: &GridsCollectionParam) -> PlacementValidity {
    if !imprint.is_in_bounds(origin, map_data.obstacle_grid.bounds) {
        return PlacementValidity::Invalid;
    }
    if map_data.reserved_coords.any_reserved(origin, imprint) {
        return PlacementValidity::Invalid;
    }
    let mut has_ore = false;
    for coords in imprint.iter(origin) {
        let field = &map_data.obstacle_grid[coords];
        if field.has_structure() || field.is_within_quantum_field() {
            return PlacementValidity::Invalid;
        }
        if field.has_dark_ore() { has_ore = true; }
    }
    if !has_ore {
        return PlacementValidity::Invalid;
    }
    if !map_data.energy_supply_grid.is_imprint_powered(origin, imprint) {
        return PlacementValidity::ValidUnpowered;
    }
    PlacementValidity::Valid
}

fn mining_complex_annotator(_: MapObject, origin: GridCoords, imprint: GridImprint, validity: PlacementValidity, map_data: &GridsCollectionParam) -> Vec<(GridCoords, CellHighlight)> {
    match validity {
        PlacementValidity::Invalid => imprint.iter(origin)
            .filter(|coords| {
                !coords.are_in_bounds(map_data.obstacle_grid.bounds)
                    || map_data.obstacle_grid[*coords].has_structure()
                    || map_data.obstacle_grid[*coords].is_within_quantum_field()
            })
            .map(|coords| (coords, CellHighlight::Negative))
            .collect(),
        _ => imprint.iter_in_bounds(origin, map_data.obstacle_grid.bounds)
            .filter(|coords| map_data.obstacle_grid[*coords].has_dark_ore())
            .map(|coords| (coords, CellHighlight::Positive))
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
            .add_systems(CollectSave, collect_mining_complexes)
            .register_loader(MapLoadingStage::SpawnMapElements, "mining_complexes", load_mining_complexes)
            .register_building(BuildingType::MiningComplex, almanach_info)
            ;
    }
}


#[derive(Component)]
pub(crate) struct MiningComplexDeliveryTimer(pub Timer);

#[derive(Component, SSS)]
pub(crate) struct BuilderMiningComplex {
    pub grid_position: GridCoords,
    /// Saved integrity points. `None` ⇒ defer to baseline (fresh spawn);
    /// `Some` ⇒ override with saved value (restore).
    pub integrity_points: Option<f32>,
    /// Whether the player disabled this building. False on fresh spawn.
    pub disabled_by_player: bool,
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
            placement: PlacementChannel::of::<Building>(),
        }
    }

    pub fn new(grid_position: GridCoords) -> Self {
        Self { grid_position, integrity_points: None, disabled_by_player: false }
    }
    pub fn with_integrity_points(mut self, integrity_points: f32) -> Self {
        self.integrity_points = Some(integrity_points);
        self
    }
    pub fn with_disabled_by_player(mut self) -> Self {
        self.disabled_by_player = true;
        self
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
        if let Some(ip) = builder.integrity_points {
            entity_commands.insert(IntegrityPoints::new(ip));
        }
        if builder.disabled_by_player {
            entity_commands.insert(DisabledByPlayer);
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
                NeedsPower,
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
            ))
            .observe(on_technical_state_changed_recompute_operational);
        commands.trigger(TechnicalStateChanged { entity, kind: TechnicalChange::JustSpawned });
    }
}

fn collect_mining_complexes(
    mining_complexes: Query<(Entity, &GridCoords, &IntegrityPoints, Has<DisabledByPlayer>), With<MiningComplex>>,
    mut save: SaveWriter,
) {
    if mining_complexes.is_empty() { return; }
    let rows: Vec<(i64, i32, i32, f32, bool)> = mining_complexes
        .iter()
        .map(|(entity, coords, integrity_points, disabled_by_player)| {
            (
                entity.index_u32() as i64,
                coords.x,
                coords.y,
                integrity_points.get_current(),
                disabled_by_player,
            )
        })
        .collect();
    Log::debug().dev().tag(Tag::GameSave).message(format!("Saving {} mining complexes", rows.len()));
    save.submit(move |tx| {
        for (id, gx, gy, integrity_points, disabled_by_player) in rows {
            tx.save_marker("mining_complexes", id)?;
            tx.save_grid_coords(id, GridCoords { x: gx, y: gy })?;
            tx.save_integrity_points(id, integrity_points)?;
            if disabled_by_player {
                tx.save_disabled_by_player(id)?;
            }
        }
        Ok(())
    });
}

fn load_mining_complexes(ctx: &mut LoadContext) -> rusqlite::Result<()> {
    let mut stmt = ctx.conn.prepare("SELECT id FROM mining_complexes")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let old_id: i64 = row.get(0)?;
        let grid_position = ctx.conn.get_grid_coords(old_id)?;
        let integrity_points = ctx.conn.get_integrity_points(old_id)?;
        let disabled_by_player = ctx.conn.get_disabled_by_player(old_id)?;

        let Some(entity) = ctx.entity(old_id) else {
            Log::warn().dev().tag(Tag::GameLoad).message(format!("MiningComplex with old ID {old_id} has no corresponding new entity"));
            continue;
        };
        let mut builder = BuilderMiningComplex::new(grid_position)
            .with_integrity_points(integrity_points);
        if disabled_by_player {
            builder = builder.with_disabled_by_player();
        }
        ctx.insert(entity, builder);
    }
    Ok(())
}

fn mine_ore_system(
    mut stock: ResMut<Stock>,
    time: Res<Time>,
    mut mining_complexes: Query<(&mut MiningComplexDeliveryTimer, &DarkOreInRange), (With<MiningComplex>, With<IsOperational>)>,
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
