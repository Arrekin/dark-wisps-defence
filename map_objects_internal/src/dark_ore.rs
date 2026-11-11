use bevy::prelude::*;

use almanach::{Almanach, DarkOreInfo, prelude::AlmanachAppExt};
use game_core::prelude::{AltitudeLevel, GridArea, GridImprint, GridPosition, MapObject, MapObjectTooltipRequest, SSS};
use grids::{ObstacleChangeSystems, placement::{PlacementModes, PlacementValidity, PlaceRequest, RemoveRequest}};
use hud::prelude::BuilderSideMenuItemTooltip;
use indexes::{
    obstacles::GridStructure,
    placement::{annotate_not_open_ground, GridObjectPlacer, GridsCollectionParam, validator_open_ground},
    prelude::ObstacleGrid,
};
use logging::prelude::*;
use map_objects::prelude::*;
use persistence::prelude::*;
use states::prelude::{InGame, MapLoadingStage};

pub(crate) struct DarkOrePlugin;
impl Plugin for DarkOrePlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Update, despawn_depleted_dark_ores.in_set(ObstacleChangeSystems))
            .add_observer(BuilderDarkOre::on_builder_add_spawn_dark_ore)
            .add_observer(on_insert_dark_ore_imprint_on_obstacle_grid)
            .add_observer(on_discard_dark_ore_deprint_from_obstacle_grid)
            .add_observer(dark_ore_area_scanner::on_insert_dark_ore_area_scanner_rescan)
            .add_observer(dark_ore_area_scanner::on_remove_dark_ore_sync_scanners.run_if(in_state(InGame)))
            .add_observer(dark_ore_area_scanner::on_add_dark_ore_sync_scanners)
            .add_observer(on_dark_ore_place_request_do_so)
            .add_observer(on_dark_ore_remove_request_do_so)
            .add_observer(on_map_object_tooltip_request_spawn_dark_ore_tooltip)
            .add_systems(CollectSave, collect_dark_ores)
            .register_loader(MapLoadingStage::SpawnMapElements, "dark_ores", load_dark_ores)
            .register_dark_ore(BuilderDarkOre::almanach_info());
    }
}

const DARK_ORE_GRID_IMPRINT: GridImprint = GridImprint::square(1);

#[derive(Component, SSS)]
pub(crate) struct BuilderDarkOre {
    pub grid_position: GridPosition,
    pub amount: u32,
}
impl BuilderDarkOre {
    pub fn almanach_info() -> DarkOreInfo {
        DarkOreInfo {
            name: "Dark Ore".to_string(),
            description: "A deposit of dark ore. A mining complex in range extracts it over time.".to_string(),
            grid_imprint: DARK_ORE_GRID_IMPRINT,
            max_field_saturation: 1000,
            validate: validator_open_ground,
            annotate: annotate_not_open_ground,
            placement: PlacementModes::on_press(),
        }
    }

    pub fn new(grid_position: GridPosition, amount: u32) -> Self {
        Self { grid_position, amount }
    }

    fn on_builder_add_spawn_dark_ore(
        trigger: On<Add<BuilderDarkOre>>,
        mut commands: Commands,
        builders: Query<&BuilderDarkOre>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return; };

        commands.entity(entity)
            .remove::<BuilderDarkOre>()
            .insert((
                GridArea::new(builder.grid_position, DARK_ORE_GRID_IMPRINT),
                DarkOre { amount: builder.amount as i32 },
            ));
    }
}

#[log_tags(Tag::GameSave)]
fn collect_dark_ores(
    dark_ores: Query<(Entity, &GridArea, &DarkOre)>,
    mut save: SaveWriter,
) {
    if dark_ores.is_empty() { return; }

    #[debug_dev("Saving {} dark ores", rows.len())]
    let rows: Vec<(u32, GridPosition, u32)> = dark_ores
        .iter()
        .map(|(entity, area, dark_ore)| {
            (
                entity.index_u32(),
                area.origin,
                dark_ore.amount as u32,
            )
        })
        .collect();
    save.submit(move |ctx| {
        for (id, grid_position, amount) in rows {
            ctx.register_entity(id)?;
            ctx.tx.execute(
                "INSERT OR REPLACE INTO dark_ores (id, amount) VALUES (?1, ?2)",
                (id, amount),
            )?;
            ctx.save_grid_position(id, grid_position)?;
        }
        Ok(())
    });
}

fn load_dark_ores(ctx: &mut LoadContext) -> LoadResult {
    ctx.for_each_entity("SELECT id, amount FROM dark_ores", |ctx, old_id, entity, row| {
        let amount: u32 = row.get(1)?;
        let grid_position = ctx.grid_position(old_id)?;
        ctx.insert(entity, BuilderDarkOre::new(grid_position, amount));
        Ok(())
    })
}

#[log_tags(Tag::MapObjects)]
fn on_insert_dark_ore_imprint_on_obstacle_grid(
    trigger: On<Insert<DarkOre>>,
    mut obstacle_grid: ResMut<ObstacleGrid>,
    dark_ores: Query<&GridArea, With<DarkOre>>,
) {
    let entity = trigger.entity;
    #[warn_dev("Dark ore {entity} has no area; not imprinted on the obstacle grid")]
    let Ok(area) = dark_ores.get(entity) else { return; };
    obstacle_grid.imprint_dark_ore(*area, entity);
}

fn on_discard_dark_ore_deprint_from_obstacle_grid(
    trigger: On<Discard<DarkOre>>,
    mut obstacle_grid: ResMut<ObstacleGrid>,
    dark_ores: Query<&GridArea, With<DarkOre>>,
) {
    let Ok(area) = dark_ores.get(trigger.entity) else { return; };
    obstacle_grid.deprint(*area);
}

#[log_tags(Tag::Resources)]
fn despawn_depleted_dark_ores(
    mut commands: Commands,
    dark_ores: Query<(Entity, &DarkOre, &GridArea), Changed<DarkOre>>,
) {
    for (entity, dark_ore, area) in dark_ores.iter() {
        if dark_ore.amount <= 0 {
            #[debug_dev("Dark ore at {} depleted", area.origin)]
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
    placer: Single<&GridArea, With<GridObjectPlacer>>,
) {
    let PlaceRequest(MapObject::DarkOre) = *trigger else { return };
    let area = *placer.into_inner();
    let validity = (almanach.dark_ore.validate)(MapObject::DarkOre, area, &grids);
    if validity == PlacementValidity::Invalid { return; }
    #[debug_player("Dark ore placed at {}", area.origin)]
    commands.spawn(BuilderDarkOre::new(area.origin.coords.at(AltitudeLevel::Underground), almanach.dark_ore.max_field_saturation));
    grids.reserved_coords.reserve(area);
}

#[log_tags(Tag::MapObjects)]
fn on_dark_ore_remove_request_do_so(
    trigger: On<RemoveRequest>,
    mut commands: Commands,
    grids: GridsCollectionParam,
    placer: Single<&GridArea, With<GridObjectPlacer>>,
) {
    let RemoveRequest(MapObject::DarkOre) = *trigger else { return };
    let coords = placer.origin.coords;
    #[debug_player("Dark ore removed at {coords}")]
    if let Some(GridStructure::DarkOre { slot }) = grids.obstacle_grid.get(coords).map(|cell| cell.underground) {
        commands.entity(grids.obstacle_grid.entry(slot).entity).despawn();
    }
}

pub(crate) mod dark_ore_area_scanner {
    use super::*;

    /// Rescans when a scanner spawns or its range changes.
    pub fn on_insert_dark_ore_area_scanner_rescan(
        trigger: On<Insert<DarkOreAreaScanner>>,
        mut commands: Commands,
        obstacle_grid: Res<ObstacleGrid>,
        mut scanners: Query<(&DarkOreAreaScanner, &mut DarkOreInRange)>,
    ) {
        let entity = trigger.entity;
        let Ok((scanner, mut dark_ore_in_range)) = scanners.get_mut(entity) else { return; };
        let ore_entities_in_range: Vec<Entity> = scanner.range.iter_in_bounds(obstacle_grid.bounds)
            .filter_map(|coords| match obstacle_grid.cell(coords).underground { GridStructure::DarkOre { slot } => Some(obstacle_grid.entry(slot).entity), _ => None })
            .collect();
        if ore_entities_in_range.is_empty() {
            commands.entity(entity).remove::<HasOreInScannerRange>();
        } else {
            commands.entity(entity).insert(HasOreInScannerRange);
        }
        dark_ore_in_range.0 = ore_entities_in_range;
    }

    /// Keeps every scanner's `DarkOreInRange` in sync when any dark ore is removed.
    pub fn on_remove_dark_ore_sync_scanners(
        trigger: On<Remove<DarkOre>>,
        mut commands: Commands,
        dark_ores: Query<&GridArea, With<DarkOre>>,
        mut scanners: Query<(Entity, &DarkOreAreaScanner, &mut DarkOreInRange)>,
    ) {
        let entity = trigger.entity;
        let Ok(dark_ore_area) = dark_ores.get(entity) else { return; };
        for (scanner_entity, scanner, mut dark_ore_in_range) in scanners.iter_mut() {
            if scanner.range.contains(dark_ore_area.origin.coords)
                && let Some(index) = dark_ore_in_range.0.iter().position(|&ore| ore == entity)
            {
                dark_ore_in_range.0.swap_remove(index);
                if dark_ore_in_range.0.is_empty() {
                    commands.entity(scanner_entity).remove::<HasOreInScannerRange>();
                }
            }
        }
    }

    pub fn on_add_dark_ore_sync_scanners(
        trigger: On<Add<DarkOre>>,
        mut commands: Commands,
        dark_ores: Query<&GridArea, With<DarkOre>>,
        mut scanners: Query<(Entity, &DarkOreAreaScanner, &mut DarkOreInRange)>,
    ) {
        let entity = trigger.entity;
        let Ok(dark_ore_area) = dark_ores.get(entity) else { return; };

        for (scanner_entity, scanner, mut dark_ore_in_range) in scanners.iter_mut() {
            if scanner.range.contains(dark_ore_area.origin.coords)
                && !dark_ore_in_range.0.contains(&entity)
            {
                let was_empty = dark_ore_in_range.0.is_empty();
                dark_ore_in_range.0.push(entity);
                if was_empty {
                    commands.entity(scanner_entity).insert(HasOreInScannerRange);
                }
            }
        }
    }
}

fn on_map_object_tooltip_request_spawn_dark_ore_tooltip(
    trigger: On<MapObjectTooltipRequest>,
    mut commands: Commands,
    almanach: Res<Almanach>,
) {
    let MapObjectTooltipRequest { entity, object: MapObject::DarkOre } = *trigger else { return };

    let info = &almanach.dark_ore;
    commands.spawn(
        BuilderSideMenuItemTooltip::new(entity)
            .with_name(info.name.clone())
            .with_description(info.description.clone())
            .with_fact(info.grid_imprint.label()),
    );
}
