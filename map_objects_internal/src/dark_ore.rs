use bevy::prelude::*;

use almanach::{Almanach, DarkOreInfo, ObjectPresentation, prelude::AlmanachAppExt};
use game_core::prelude::{GridCoords, GridImprint, MapObject, SSS};
use grids::{
    placement::{
        annotate_non_empty, GridObjectPlacer, GridsCollectionParam, PlacementModes, PlacementValidity, PlaceRequest,
        RemoveRequest, validator_all_empty,
    },
    prelude::ObstacleGrid,
};
use hud::prelude::BuilderSideMenuItemTooltip;
use logging::prelude::*;
use map_objects::prelude::*;
use persistence::prelude::*;
use states::prelude::MapLoadingStage;

pub(crate) struct DarkOrePlugin;
impl Plugin for DarkOrePlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Update, despawn_depleted_dark_ores)
            .add_observer(BuilderDarkOre::on_builder_add_spawn_dark_ore)
            .add_observer(dark_ore_area_scanner::on_add_dark_ore_area_scanner_init)
            .add_observer(dark_ore_area_scanner::on_remove_dark_ore_sync_scanners)
            .add_observer(dark_ore_area_scanner::on_add_dark_ore_sync_scanners)
            .add_observer(on_dark_ore_place_request_do_so)
            .add_observer(on_dark_ore_remove_request_do_so)
            .add_observer(on_builder_add_spawn_dark_ore_tooltip)
            .add_systems(CollectSave, collect_dark_ores)
            .register_loader(MapLoadingStage::SpawnMapElements, "dark_ores", load_dark_ores)
            .register_dark_ore(BuilderDarkOre::almanach_info());
    }
}

pub(crate) const DARK_ORE_GRID_IMPRINT: GridImprint = GridImprint::Rectangle { width: 1, height: 1 };

#[derive(Component, SSS)]
pub(crate) struct BuilderDarkOre {
    pub grid_position: GridCoords,
    pub amount: u32,
}
impl BuilderDarkOre {
    pub fn almanach_info() -> DarkOreInfo {
        DarkOreInfo {
            name: "Dark Ore".to_string(),
            description: "A deposit of dark ore. A mining complex in range extracts it over time.".to_string(),
            grid_imprint: DARK_ORE_GRID_IMPRINT,
            max_field_saturation: 1000,
            validate: validator_all_empty,
            annotate: annotate_non_empty,
            placement: PlacementModes::on_press(),
            presentation: ObjectPresentation {
                tooltip: Some(dark_ore_tooltip),
            },
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

#[log_tags(Tag::GameSave)]
fn collect_dark_ores(
    dark_ores: Query<(Entity, &GridCoords, &DarkOre)>,
    mut save: SaveWriter,
) {
    if dark_ores.is_empty() { return; }

    #[debug_dev("Saving {} dark ores", rows.len())]
    let rows: Vec<(u32, GridCoords, u32)> = dark_ores
        .iter()
        .map(|(entity, coords, dark_ore)| {
            (
                entity.index_u32(),
                *coords,
                dark_ore.amount as u32,
            )
        })
        .collect();
    save.submit(move |ctx| {
        for (id, coords, amount) in rows {
            ctx.register_entity(id)?;
            ctx.tx.execute(
                "INSERT OR REPLACE INTO dark_ores (id, amount) VALUES (?1, ?2)",
                (id, amount),
            )?;
            ctx.save_grid_coords(id, coords)?;
        }
        Ok(())
    });
}

fn load_dark_ores(ctx: &mut LoadContext) -> LoadResult {
    ctx.for_each_entity("SELECT id, amount FROM dark_ores", |ctx, old_id, entity, row| {
        let amount: u32 = row.get(1)?;
        let grid_position = ctx.grid_coords(old_id)?;
        ctx.insert(entity, BuilderDarkOre::new(grid_position, amount));
        Ok(())
    })
}

#[log_tags(Tag::Resources)]
fn despawn_depleted_dark_ores(
    mut commands: Commands,
    dark_ores: Query<(Entity, &DarkOre, &GridCoords), Changed<DarkOre>>,
) {
    for (entity, dark_ore, coords) in dark_ores.iter() {
        if dark_ore.amount <= 0 {
            #[debug_dev("Dark ore at ({}, {}) depleted", coords.x, coords.y)]
            commands.entity(entity).despawn();
        }
    }
}

#[log_tags(Tag::MapObjects)]
fn on_dark_ore_place_request_do_so(
    trigger: On<PlaceRequest>,
    mut commands: Commands,
    almanach: Res<Almanach>,
    mut grids: GridsCollectionParam,
    placer: Single<(&GridCoords, &GridImprint), With<GridObjectPlacer>>,
) {
    let PlaceRequest(MapObject::DarkOre) = *trigger else { return };
    let (coords, grid_imprint) = placer.into_inner();
    let validity = (almanach.dark_ore.validate)(MapObject::DarkOre, *coords, *grid_imprint, &grids);
    if validity == PlacementValidity::Invalid { return; }
    #[debug_dev("Dark ore placed at ({}, {})", coords.x, coords.y)]
    commands.spawn(BuilderDarkOre::new(*coords, almanach.dark_ore.max_field_saturation));
    grids.reserved_coords.reserve(*coords, *grid_imprint);
}

#[log_tags(Tag::MapObjects)]
fn on_dark_ore_remove_request_do_so(
    trigger: On<RemoveRequest>,
    mut commands: Commands,
    grids: GridsCollectionParam,
    placer: Single<&GridCoords, With<GridObjectPlacer>>,
) {
    let RemoveRequest(MapObject::DarkOre) = *trigger else { return };
    let coords = placer.into_inner();
    #[debug_dev("Dark ore removed at ({}, {})", coords.x, coords.y)]
    if let Some(entity) = grids.obstacle_grid[*coords].dark_ore {
        commands.entity(entity).despawn();
    }
}

pub(crate) mod dark_ore_area_scanner {
    use super::*;

    pub fn on_add_dark_ore_area_scanner_init(
        trigger: On<Add, DarkOreAreaScanner>,
        mut commands: Commands,
        scanners: Query<&DarkOreAreaScanner>,
    ) {
        let entity = trigger.entity;
        let Ok(scanner) = scanners.get(entity) else { return; };
        commands.entity(entity)
            .observe(on_insert_scanner_or_coords_rescan)
            .insert(scanner.clone()); // Reinsert self to trigger initial scan; TODO: improve once Bevy introduces compound triggers
    }

    /// Local observer: rescans when the scanning entity moves or its scanner range changes.
    fn on_insert_scanner_or_coords_rescan(
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

    /// Keeps every scanner's `DarkOreInRange` in sync when any dark ore is removed.
    pub fn on_remove_dark_ore_sync_scanners(
        trigger: On<Remove, DarkOre>,
        mut commands: Commands,
        dark_ores: Query<&GridCoords, With<DarkOre>>,
        mut scanners: Query<(Entity, &DarkOreAreaScanner, &mut DarkOreInRange, &GridCoords)>,
    ) {
        let entity = trigger.entity;
        let Ok(dark_ore_grid_coords) = dark_ores.get(entity) else { return; };
        for (scanner_entity, scanner, mut dark_ore_in_range, scanner_grid_coords) in scanners.iter_mut() {
            // TODO: This won't work when we want to implement Mining Complex range expansion, as the GridCoords won't match ScannerImprint coords
            // Ie, the expected mining range coords will shift in relation to the MiningComplex own's coords as they start in bottom left corner.
            if scanner.range_imprint.covers_coords(*scanner_grid_coords, *dark_ore_grid_coords)
                && let Some(index) = dark_ore_in_range.0.iter().position(|&ore| ore == entity)
            {
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
                && !dark_ore_in_range.0.contains(&entity)
            {
                let was_empty = dark_ore_in_range.0.is_empty();
                dark_ore_in_range.0.push(entity);
                if was_empty {
                    commands.entity(scanner_entity).insert(HasOreInScannerRange).remove::<NoOreInScannerRange>();
                }
            }
        }
    }
}

/// Queues tooltip construction for a dark-ore placement tile.
pub(crate) fn dark_ore_tooltip(commands: &mut Commands, anchor: Entity, _map_object: MapObject) {
    commands.spawn(BuilderDarkOreSideMenuTooltip(anchor));
}

fn on_builder_add_spawn_dark_ore_tooltip(
    trigger: On<Add, BuilderDarkOreSideMenuTooltip>,
    mut commands: Commands,
    almanach: Res<Almanach>,
    builders: Query<&BuilderDarkOreSideMenuTooltip>,
) {
    let entity = trigger.entity;
    let Ok(builder) = builders.get(entity) else { return; };

    let info = &almanach.dark_ore;
    commands.entity(entity)
        .remove::<BuilderDarkOreSideMenuTooltip>()
        .insert(
            BuilderSideMenuItemTooltip::new(builder.0)
                .with_name(info.name.clone())
                .with_description(info.description.clone())
                .with_fact(info.grid_imprint.label()),
        );
}
