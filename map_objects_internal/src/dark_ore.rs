use bevy::prelude::*;

use almanach::{Almanach, DarkOreInfo, prelude::AlmanachAppExt};
use game_core::prelude::{GridArea, GridCoords, GridImprint, MapObject, MapObjectTooltipRequest, SSS};
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
use states::prelude::{InGame, MapLoadingStage};

pub(crate) struct DarkOrePlugin;
impl Plugin for DarkOrePlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Update, despawn_depleted_dark_ores)
            .add_observer(BuilderDarkOre::on_builder_add_spawn_dark_ore)
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
        }
    }

    pub fn new(grid_position: GridCoords, amount: u32) -> Self {
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
    let rows: Vec<(u32, GridCoords, u32)> = dark_ores
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
    #[debug_dev("Dark ore placed at {}", area.origin)]
    commands.spawn(BuilderDarkOre::new(area.origin, almanach.dark_ore.max_field_saturation));
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
    let coords = placer.origin;
    #[debug_dev("Dark ore removed at {coords}")]
    if let Some(entity) = grids.obstacle_grid.get(coords).and_then(|cell| cell.dark_ore) {
        commands.entity(entity).despawn();
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
        let ore_entities_in_range = obstacle_grid.query_area_element(scanner.range, |cell| cell.dark_ore);
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
            if scanner.range.contains(dark_ore_area.origin)
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
            if scanner.range.contains(dark_ore_area.origin)
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
