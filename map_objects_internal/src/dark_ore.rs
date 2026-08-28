use bevy::prelude::*;

use almanach::prelude::AlmanachAppExt;
use almanach::{Almanach, DarkOreInfo};
use game_core::prelude::{GridCoords, GridImprint, MapObject, SSS};
use grids::placement::{annotate_non_empty, GridObjectPlacer, GridsCollectionParam, PlacementChannel, PlacementMode, PlacementValidity, PlaceRequest, RemoveRequest, validator_all_empty};
use grids::prelude::ObstacleGrid;
use logging::prelude::*;
use map_objects::prelude::*;
use persistence::{
    prelude::{AppGameLoadSaveExtension, CollectSave, GameDbHelpers, LoadContext, SaveWriter},
    rusqlite,
};
use states::prelude::MapLoadingStage;

pub struct DarkOrePlugin;
impl Plugin for DarkOrePlugin {
    fn build(&self, app: &mut App) {
        let almanach_info = BuilderDarkOre::almanach_info(app.world().resource::<AssetServer>());
        app
            .add_systems(Update, remove_empty)
            .add_observer(BuilderDarkOre::on_builder_add_spawn_dark_ore)
            .add_observer(dark_ore_area_scanner::on_add_init_scanner)
            .add_observer(dark_ore_area_scanner::on_remove_dark_ore_sync_scanners)
            .add_observer(dark_ore_area_scanner::on_add_dark_ore_sync_scanners)
            .add_observer(on_dark_ore_place_request_do_so)
            .add_observer(on_dark_ore_remove_request_do_so)
            .add_systems(CollectSave, collect_dark_ores)
            .register_loader(MapLoadingStage::SpawnMapElements, "dark_ores", load_dark_ores)
            .register_dark_ore(almanach_info)
            ;
    }
}

pub(crate) const DARK_ORE_GRID_IMPRINT: GridImprint = GridImprint::Rectangle { width: 1, height: 1 };



#[derive(Component, SSS)]
pub(crate) struct BuilderDarkOre {
    pub grid_position: GridCoords,
    pub amount: u32,
}
impl BuilderDarkOre {
    pub fn almanach_info(asset_server: &AssetServer) -> DarkOreInfo {
        DarkOreInfo {
            name: "Dark Ore".to_string(),
            grid_imprint: DARK_ORE_GRID_IMPRINT,
            sprite: asset_server.load("map_objects/dark_ore_1.png"),
            max_field_saturation: 1000,
            validate: validator_all_empty,
            annotate: annotate_non_empty,
            placement: PlacementChannel::of::<DarkOre>().with_modes(PlacementMode::OnPress),
        }
    }

    pub fn new(grid_position: GridCoords, amount: u32) -> Self {
        Self { grid_position, amount }
    }

    fn on_builder_add_spawn_dark_ore(
        trigger: On<Add, BuilderDarkOre>,
        mut commands: Commands,
        builders: Query<&BuilderDarkOre>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return; };

        commands.entity(entity)
            .remove::<BuilderDarkOre>()
            .insert((
            builder.grid_position,
            DarkOre { amount: builder.amount as i32 },
            DARK_ORE_GRID_IMPRINT,
        ));
    }
}

fn collect_dark_ores(
    dark_ores: Query<(Entity, &GridCoords, &DarkOre)>,
    mut save: SaveWriter,
) {
    if dark_ores.is_empty() { return; }
    let rows: Vec<(i64, i32, i32, u32)> = dark_ores
        .iter()
        .map(|(entity, coords, dark_ore)| {
            (
                entity.index_u32() as i64,
                coords.x,
                coords.y,
                dark_ore.amount as u32,
            )
        })
        .collect();
    Log::debug().dev().tag(Tag::GameSave).message(format!("Saving {} dark ores", rows.len()));
    save.submit(move |tx| {
        for (id, gx, gy, amount) in rows {
            tx.register_entity(id)?;
            tx.execute(
                "INSERT OR REPLACE INTO dark_ores (id, amount) VALUES (?1, ?2)",
                (id, amount),
            )?;
            tx.save_grid_coords(id, GridCoords { x: gx, y: gy })?;
        }
        Ok(())
    });
}

fn load_dark_ores(ctx: &mut LoadContext) -> rusqlite::Result<()> {
    let mut stmt = ctx.conn.prepare("SELECT id, amount FROM dark_ores")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let old_id: i64 = row.get(0)?;
        let amount: u32 = row.get(1)?;
        let grid_position = ctx.conn.get_grid_coords(old_id)?;

        let Some(entity) = ctx.entity(old_id) else {
            Log::warn().dev().tag(Tag::GameLoad).message(format!("DarkOre with old ID {old_id} has no corresponding new entity"));
            continue;
        };
        ctx.insert(entity, BuilderDarkOre::new(grid_position, amount));
    }
    Ok(())
}

fn remove_empty(
    mut commands: Commands,
    dark_ores: Query<(Entity, &DarkOre, &GridCoords), Changed<DarkOre>>,
) {
    for (entity, dark_ore, coords) in dark_ores.iter() {
        if dark_ore.amount <= 0 {
            Log::debug().dev().tag(Tag::Resources).message(format!("Dark ore at ({}, {}) depleted", coords.x, coords.y));
            commands.entity(entity).despawn();
        }
    }
}

fn on_dark_ore_place_request_do_so(
    _trigger: On<PlaceRequest<DarkOre>>,
    mut commands: Commands,
    almanach: Res<Almanach>,
    mut grids: GridsCollectionParam,
    placer: Single<(&GridCoords, &GridImprint), With<GridObjectPlacer>>,
) {
    let (coords, grid_imprint) = placer.into_inner();
    let validity = {
        (almanach.dark_ore.validate)(MapObject::DarkOre, *coords, *grid_imprint, &grids)
    };
    if validity == PlacementValidity::Invalid { return; }
    commands.spawn(BuilderDarkOre::new(*coords, almanach.dark_ore.max_field_saturation));
    grids.reserved_coords.reserve(*coords, *grid_imprint);
}

fn on_dark_ore_remove_request_do_so(
    _trigger: On<RemoveRequest<DarkOre>>,
    mut commands: Commands,
    grids: GridsCollectionParam,
    placer: Single<&GridCoords, With<GridObjectPlacer>>,
) {
    let coords = placer.into_inner();
    if let Some(entity) = grids.obstacle_grid[*coords].dark_ore {
        commands.entity(entity).despawn();
    }
}

pub(crate) mod dark_ore_area_scanner {
    use super::*;

    pub fn on_add_init_scanner(
        trigger: On<Add, DarkOreAreaScanner>,
        mut commands: Commands,
        scanners: Query<&DarkOreAreaScanner>,
    ) {
        let entity = trigger.entity;
        let scanner = scanners.get(entity).unwrap();
        commands.entity(entity)
            .observe(scan_on_change)
            .insert(scanner.clone()); // Reinsert self to trigger initial scan; TODO: improve once Bevy introduces compound triggers
    }

    // Local triggers when entity that is interested in scanner info changes by moving or changing the scanner range
    fn scan_on_change(
        trigger: On<Insert, (DarkOreAreaScanner, GridCoords)>,
        mut commands: Commands,
        obstacle_grid: Res<ObstacleGrid>,
        mut scanners: Query<(&DarkOreAreaScanner, &GridCoords, &mut DarkOreInRange)>,
    ) {
        let entity = trigger.entity;
        let Ok((scanner, grid_coords, mut dark_ore_in_range)) = scanners.get_mut(entity) else { return; };
        let ore_entities_in_range = obstacle_grid.query_imprint_element(*grid_coords, scanner.range_imprint, |field| field.dark_ore);
        if ore_entities_in_range.is_empty() {
            commands.entity(entity).insert(NoOreInScannerRange).remove::<HasOreInScannerRange>();
        } else {
            commands.entity(entity).insert(HasOreInScannerRange).remove::<NoOreInScannerRange>();
        }
        dark_ore_in_range.0 = ore_entities_in_range;
    }

    // Global trigger reacting to any dark ore removal to keep DarkOreinRange in sync
    pub fn on_remove_dark_ore_sync_scanners(
        trigger: On<Remove, DarkOre>,
        mut commands: Commands,
        dark_ores: Query<&GridCoords, With<DarkOre>>,
        mut scanners: Query<(Entity, &DarkOreAreaScanner, &mut DarkOreInRange, &GridCoords)>,
    ) {
        let entity = trigger.entity;
        let dark_ore_grid_coords = dark_ores.get(entity).unwrap();
        for (scanner_entity, scanner, mut dark_ore_in_range, scanner_grid_coords) in scanners.iter_mut() {
            // TODO: This won't work when we want to implement Mining Complex range expansion, as the GridCoords won't match ScannerImprint coords
            // Ie, the expected mining range coords will shift in relation to the MiningComplex own's coords as they start in bottom left corner.
            if scanner.range_imprint.covers_coords(*scanner_grid_coords, *dark_ore_grid_coords)
                && let Some(index) = dark_ore_in_range.0.iter().position(|&x| x == entity) {
                    dark_ore_in_range.0.swap_remove(index);
                }
            if dark_ore_in_range.0.is_empty() {
                commands.entity(scanner_entity).insert(NoOreInScannerRange).remove::<HasOreInScannerRange>();
            }
        }
    }

    pub fn on_add_dark_ore_sync_scanners(
        trigger: On<Add, DarkOre>,
        mut commands: Commands,
        dark_ores: Query<&GridCoords, With<DarkOre>>,
        mut scanners: Query<(Entity, &DarkOreAreaScanner, &mut DarkOreInRange, &GridCoords)>,
    ) {
        let entity = trigger.entity;
        let Ok(dark_ore_grid_coords) = dark_ores.get(entity) else { return; };

        for (scanner_entity, scanner, mut dark_ore_in_range, scanner_grid_coords) in scanners.iter_mut() {
            if scanner.range_imprint.covers_coords(*scanner_grid_coords, *dark_ore_grid_coords)
                && !dark_ore_in_range.0.contains(&entity) {
                    let was_empty = dark_ore_in_range.0.is_empty();
                    dark_ore_in_range.0.push(entity);
                    if was_empty {
                        commands.entity(scanner_entity).insert(HasOreInScannerRange).remove::<NoOreInScannerRange>();
                    }
                }
        }
    }
}

