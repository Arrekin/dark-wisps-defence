use std::f32::consts::PI;

use lib_core::map_objects::DarkOre;
use lib_grid::grids::obstacles::{ObstacleGrid, ReservedCoords};

use crate::prelude::*;
use crate::ui::grid_object_placer::GridObjectPlacer;

pub struct DarkOrePlugin;
impl Plugin for DarkOrePlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Update, remove_empty)
            .add_observer(BuilderDarkOre::on_add)
            .add_observer(dark_ore_area_scanner::DarkOreAreaScanner::on_add)
            .add_observer(dark_ore_area_scanner::DarkOreAreaScanner::on_remove_dark_ore)
            .add_observer(dark_ore_area_scanner::DarkOreAreaScanner::on_add_dark_ore)
            .add_observer(on_dark_ore_place_request)
            .add_observer(on_dark_ore_remove_request)
            .register_db_loader::<BuilderDarkOre>(MapLoadingStage::SpawnMapElements)
            .register_db_saver(BuilderDarkOre::on_game_save)
            .register_dark_ore(DarkOreInfo {
                name: "Dark Ore".to_string(),
                grid_imprint: DARK_ORE_GRID_IMPRINT,
                sprite_paths: DARK_ORE_BASE_IMAGES.iter().map(|s| s.to_string()).collect(),
                default_amount: 1000,
                validate: validate_empty_placement,
            })
            ;
    }
}

pub const DARK_ORE_GRID_IMPRINT: GridImprint = GridImprint::Rectangle { width: 1, height: 1 };
pub const DARK_ORE_BASE_IMAGES: [&str; 2] = ["map_objects/dark_ore_1.png", "map_objects/dark_ore_2.png"];



#[derive(Clone, Copy, Debug)]
pub struct DarkOreSaveData {
    pub entity: Entity,
}

#[derive(Component, SSS)]
pub struct BuilderDarkOre {
    pub grid_position: GridCoords,
    pub amount: u32,
    pub save_data: Option<DarkOreSaveData>,
}
impl Saveable for BuilderDarkOre {
    fn save(self, tx: &rusqlite::Transaction) -> rusqlite::Result<()> {
        let save_data = self.save_data.expect("BuilderDarkOre for saving purpose must have save_data");
        let entity_index = save_data.entity.index_u32() as i64;

        // 1. Insert into dark_ores table
        tx.register_entity(entity_index)?;
        tx.execute(
            "INSERT OR REPLACE INTO dark_ores (id, amount) VALUES (?1, ?2)",
            (entity_index, self.amount),
        )?;

        // 2. Insert into grid_positions table
        tx.save_grid_coords(entity_index, self.grid_position)?;
        Ok(())
    }
}
impl Loadable for BuilderDarkOre {
    fn load(ctx: &mut LoadContext) -> rusqlite::Result<LoadResult> {
        let mut stmt = ctx.conn.prepare("SELECT id, amount FROM dark_ores LIMIT ?1 OFFSET ?2")?;
        let mut rows = stmt.query(ctx.pagination.as_params())?;

        let mut count = 0;
        while let Some(row) = rows.next()? {
            let old_id: i64 = row.get(0)?;
            let amount: u32 = row.get(1)?;
            let grid_position = ctx.conn.get_grid_coords(old_id)?;

            if let Some(new_entity) = ctx.get_new_entity_for_old(old_id) {
                let save_data = DarkOreSaveData { entity: new_entity };
                ctx.commands.entity(new_entity).insert(BuilderDarkOre::new_for_saving(grid_position, amount, save_data));
            } else {
                eprintln!("Warning: DarkOre with old ID {} has no corresponding new entity", old_id);
            }
            count += 1;
        }

        Ok(count.into())
    }
}
impl BuilderDarkOre {
    pub fn new(grid_position: GridCoords, amount: u32) -> Self {
        Self { grid_position, amount, save_data: None }
    }
    pub fn new_for_saving(grid_position: GridCoords, amount: u32, save_data: DarkOreSaveData) -> Self {
        Self { grid_position, amount, save_data: Some(save_data) }
    }

    fn on_game_save(
        mut commands: Commands,
        dark_ores: Query<(Entity, &GridCoords, &DarkOre)>,
    ) {
        if dark_ores.is_empty() { return; }
        println!("Creating batch of BuilderDarkOre for saving. {} items", dark_ores.iter().count());
        let batch = dark_ores.iter().map(|(entity, coords, dark_ore)| {
                let save_data = DarkOreSaveData { entity };
                BuilderDarkOre::new_for_saving(*coords, dark_ore.amount as u32, save_data)
        }).collect::<SaveableBatchCommand<_>>();
        commands.queue(batch);
    }

    fn on_add(
        trigger: On<Add, BuilderDarkOre>,
        mut commands: Commands,
        builders: Query<&BuilderDarkOre>,
        asset_server: Res<AssetServer>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return; };

        let mut rng = nanorand::tls_rng();
        commands.entity(entity)
            .remove::<BuilderDarkOre>()
            .insert((
            Sprite {
                image: asset_server.load(DARK_ORE_BASE_IMAGES[rng.generate_range(0usize..2usize)]),
                custom_size: Some(DARK_ORE_GRID_IMPRINT.world_size()),
                ..Default::default()
            },
            Transform {
                translation: builder.grid_position.to_world_position_centered(DARK_ORE_GRID_IMPRINT).extend(Z_OBSTACLE),
                // select one of: Left, Up, Right, Down
                rotation: Quat::from_rotation_z([0., PI / 2., PI, 3. * PI / 2.][rng.generate_range(0usize..4usize)] as f32),
                ..default()
            },
            builder.grid_position,
            DarkOre { amount: builder.amount as i32 },
            DARK_ORE_GRID_IMPRINT,
        ));
    }
}

fn remove_empty(
    mut commands: Commands,
    dark_ores: Query<(Entity, &DarkOre), Changed<DarkOre>>,
) {
    for (entity, dark_ore) in dark_ores.iter() {
        if dark_ore.amount <= 0 {
            commands.entity(entity).despawn();
        }
    }
}

fn on_dark_ore_place_request(
    _trigger: On<lib_core::placement::PlaceRequest<DarkOre>>,
    mut commands: Commands,
    mut reserved_coords: ResMut<ReservedCoords>,
    obstacle_grid: Res<ObstacleGrid>,
    placer: Single<(&GridCoords, &GridImprint), With<GridObjectPlacer>>,
) {
    let (coords, grid_imprint) = placer.into_inner();

    if !coords.is_in_bounds(obstacle_grid.bounds()) { return; }
    if obstacle_grid.query_imprint_all(*coords, *grid_imprint, |field| !field.has_dark_ore()) && !reserved_coords.any_reserved(*coords, *grid_imprint)
    {
        commands.spawn(BuilderDarkOre::new(*coords, 1000));
        reserved_coords.reserve(*coords, *grid_imprint);
    }
}

fn on_dark_ore_remove_request(
    _trigger: On<lib_core::placement::RemoveRequest<DarkOre>>,
    mut commands: Commands,
    obstacle_grid: Res<ObstacleGrid>,
    placer: Single<&GridCoords, With<GridObjectPlacer>>,
) {
    let coords = placer.into_inner();
    if let Some(entity) = obstacle_grid[*coords].dark_ore {
        commands.entity(entity).despawn();
    }
}

pub mod dark_ore_area_scanner {
    use super::*;

    #[derive(Component, Default)]
    pub struct HasOreInScannerRange;
    #[derive(Component, Default)]
    pub struct NoOreInScannerRange;

    #[derive(Component, Clone)]
    #[component(immutable)]
    #[require(GridCoords, DarkOreInRange, NoOreInScannerRange)]
    pub struct DarkOreAreaScanner {
        pub range_imprint: GridImprint,
    }
    impl DarkOreAreaScanner {
        pub fn on_add(
            trigger: On<Add, DarkOreAreaScanner>,
            mut commands: Commands,
            scanners: Query<&DarkOreAreaScanner>,
        ) {
            let entity = trigger.entity;
            let scanner = scanners.get(entity).unwrap();
            commands.entity(entity)
                .observe(Self::scan_on_change)
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
        pub fn on_remove_dark_ore(
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
                if scanner.range_imprint.covers_coords(*scanner_grid_coords, *dark_ore_grid_coords) {
                    if let Some(index) = dark_ore_in_range.0.iter().position(|&x| x == entity) {
                        dark_ore_in_range.0.swap_remove(index);
                    }
                }
                if dark_ore_in_range.0.is_empty() {
                    commands.entity(scanner_entity).insert(NoOreInScannerRange).remove::<HasOreInScannerRange>();
                }
            }
        }

        pub fn on_add_dark_ore(
            trigger: On<Add, DarkOre>,
            mut commands: Commands,
            dark_ores: Query<&GridCoords, With<DarkOre>>,
            mut scanners: Query<(Entity, &DarkOreAreaScanner, &mut DarkOreInRange, &GridCoords)>,
        ) {
            let entity = trigger.entity;
            let Ok(dark_ore_grid_coords) = dark_ores.get(entity) else { return; };

            for (scanner_entity, scanner, mut dark_ore_in_range, scanner_grid_coords) in scanners.iter_mut() {
                if scanner.range_imprint.covers_coords(*scanner_grid_coords, *dark_ore_grid_coords) {
                    if !dark_ore_in_range.0.contains(&entity) {
                        let was_empty = dark_ore_in_range.0.is_empty();
                        dark_ore_in_range.0.push(entity);
                        if was_empty {
                            commands.entity(scanner_entity).insert(HasOreInScannerRange).remove::<NoOreInScannerRange>();
                        }
                    }
                }
            }
        }
    }

    #[derive(Component, Default)]
    pub struct DarkOreInRange(pub Vec<Entity>);
}
