use std::str::FromStr;

use lib_grid::grids::wisps::WispsGrid;
use lib_core::wisps::{WispFireType, WispWaterType, WispLightType, WispElectricType};

use crate::prelude::*;
use crate::ui::grid_object_placer::GridObjectPlacer;

use super::components::{Wisp, WispState};
use super::materials::WispMaterial;

pub const WISP_GRID_IMPRINT: GridImprint = GridImprint::Rectangle { width: 1, height: 1 };

#[derive(Clone, Copy, Debug)]
pub struct WispSaveData {
    pub entity: Entity,
    pub health: f32,
    pub world_position: Vec2,
}

#[derive(Component, SSS)]
pub struct BuilderWisp {
    pub wisp_type: WispType,
    pub grid_coords: GridCoords,
    pub save_data: Option<WispSaveData>,
}

impl Saveable for BuilderWisp {
    fn save(self, tx: &rusqlite::Transaction) -> rusqlite::Result<()> {
        let save_data = self.save_data.expect("BuilderWisp for saving must have save_data");
        let entity_index = save_data.entity.index_u32() as i64;

        tx.register_entity(entity_index)?;
        tx.save_world_position(entity_index, save_data.world_position)?;
        tx.save_grid_coords(entity_index, self.grid_coords)?;
        tx.save_health(entity_index, save_data.health)?;

        let type_str = self.wisp_type.as_ref();
        tx.execute(
            "INSERT OR REPLACE INTO wisps (id, wisp_type) VALUES (?1, ?2)",
            rusqlite::params![entity_index, type_str],
        )?;
        Ok(())
    }
}

impl Loadable for BuilderWisp {
    fn load(ctx: &mut LoadContext) -> rusqlite::Result<LoadResult> {
        let mut stmt = ctx.conn.prepare("SELECT id, wisp_type FROM wisps LIMIT ?1 OFFSET ?2")?;
        let mut rows = stmt.query(ctx.pagination.as_params())?;
        
        let mut count = 0;
        while let Some(row) = rows.next()? {
            let old_id: i64 = row.get(0)?;
            let type_str: String = row.get(1)?;
            
            let Ok(wisp_type) = WispType::from_str(&type_str) else {
                Log::warn().dev().tag(Tag::GameLoad).message(format!("Unknown WispType '{type_str}'"));
                continue;
            };

            let grid_coords = ctx.conn.get_grid_coords(old_id)?;
            let health = ctx.conn.get_health(old_id)?;
            let world_position = ctx.conn.get_world_position(old_id)?;

            if let Some(new_entity) = ctx.get_new_entity_for_old(old_id) {
                let save_data = WispSaveData { entity: new_entity, health, world_position };
                ctx.commands.entity(new_entity).insert(BuilderWisp::new_for_saving(wisp_type, grid_coords, save_data));
            }
            count += 1;
        }
        Ok(count.into())
    }
}

impl BuilderWisp {
    pub fn new(wisp_type: WispType, grid_coords: GridCoords) -> Self {
        Self { wisp_type, grid_coords, save_data: None }
    }
    pub fn new_for_saving(wisp_type: WispType, grid_coords: GridCoords, save_data: WispSaveData) -> Self {
        Self { wisp_type, grid_coords, save_data: Some(save_data) }
    }

    pub fn on_game_save(
        mut commands: Commands,
        wisps: Query<(Entity, &WispType, &GridCoords, &Health, &Transform, &WispState), With<Wisp>>,
    ) {
        if wisps.is_empty() { return; }
        let batch = wisps.iter().map(|(entity, wisp_type, coords, health, transform, wisp_state)| {
            // TODO: Once the wisps logic is mature, save the full wisp state properly. Right now we are ignoring some states(for exmple, attacking) and simply allow wisp to retarget on spawn, and continue from there.
            let world_position = if matches!(wisp_state, WispState::Attacking) {
                coords.to_world_position_centered(WISP_GRID_IMPRINT)
            } else {
                transform.translation.xy()
            };
            
            let save_data = WispSaveData {
                entity,
                health: health.get_current(),
                world_position,
            };
            BuilderWisp::new_for_saving(*wisp_type, *coords, save_data)
        }).collect::<SaveableBatchCommand<_>>();
        commands.queue(batch);
    }

    pub fn on_add(
        trigger: On<Add, BuilderWisp>,
        mut commands: Commands,
        mut wisps_grid: ResMut<WispsGrid>,
        builders: Query<&BuilderWisp>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return; };
        
        let mut rng = nanorand::tls_rng();
        let mut entity_commands = commands.entity(entity);
        
        if let Some(save_data) = &builder.save_data {
             entity_commands.insert(Health::new(save_data.health));
        }

        let translation = if let Some(save_data) = &builder.save_data {
             save_data.world_position.extend(Z_WISP)
        } else {
             builder.grid_coords.to_world_position_centered(WISP_GRID_IMPRINT).extend(Z_WISP)
        };

        let wisp_type_bundle = match builder.wisp_type {
            WispType::Fire => entity_commands.insert((WispFireType, EssencesContainer::from(EssenceContainer::new(EssenceType::Fire, 1)))),
            WispType::Water => entity_commands.insert((WispWaterType, EssencesContainer::from(EssenceContainer::new(EssenceType::Water, 1)))),
            WispType::Light => entity_commands.insert((WispLightType, EssencesContainer::from(EssenceContainer::new(EssenceType::Light, 1)))),
            WispType::Electric => entity_commands.insert((WispElectricType, EssencesContainer::from(EssenceContainer::new(EssenceType::Electric, 1)))),
        };
        wisp_type_bundle
            .remove::<BuilderWisp>()
            .insert((
                builder.grid_coords,
                Transform {
                    translation,
                    rotation: Quat::from_rotation_z(rng.generate::<f32>() * 2. * std::f32::consts::PI),
                    ..default()
                },
                Wisp,
                builder.wisp_type,
                related![EffectInstances[
                    (ModifierContributions(HashMap::from([
                        (ModifierType::MaxHealth, 10.),
                        (ModifierType::AttackRange, 1.),
                        (ModifierType::MovementSpeed, 60.),
                    ])), BaselineEffect),
                ]],
            ));
        wisps_grid.wisp_add(builder.grid_coords, entity);
    }
}

pub fn wisp_validator(
    _: MapObject,
    coords: GridCoords,
    imprint: GridImprint,
    grids: &GridsCollection,
) -> PlacementValidationResult {
    if !coords.is_in_bounds(grids.obstacle_grid.bounds()) {
        return PlacementValidationResult::invalid();
    }
    if !grids.obstacle_grid.query_imprint_all(coords, imprint, |f| f.is_empty()) {
        return PlacementValidationResult::invalid();
    }
    if !grids.wisps_grid[coords].is_empty() {
        return PlacementValidationResult::invalid();
    }
    PlacementValidationResult::valid()
}

pub fn on_wisp_spawn_attach_material<WispT: Component, MaterialT: Asset + WispMaterial>(
    trigger: On<Add, WispT>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<MaterialT>>,
    wisps: Query<(), With<WispT>>,
) {
    let entity = trigger.entity;
    if !wisps.contains(entity) { return; }
    let wisp_world_size = WISP_GRID_IMPRINT.world_size();
    let mesh = meshes.add(Rectangle::new(wisp_world_size.x, wisp_world_size.y));
    let material = materials.add(MaterialT::make(&asset_server));
    commands.entity(entity).insert((
        Mesh2d(mesh),
        MeshMaterial2d(material),
    ));
}

pub fn on_wisp_place_request(
    _trigger: On<lib_core::placement::PlaceRequest<WispType>>,
    mut commands: Commands,
    obstacle_grid: Res<lib_grid::grids::obstacles::ObstacleGrid>,
    wisps_grid: Res<WispsGrid>,
    placer: Single<(&GridObjectPlacer, &GridCoords, &GridImprint)>,
) {
    let (grid_object_placer, coords, grid_imprint) = placer.into_inner();
    let Some(active_placement) = &grid_object_placer.active_placement else { return };
    let MapObject::Wisp(wisp_type) = active_placement.map_object else { return };
    
    if !coords.is_in_bounds(obstacle_grid.bounds()) { return; }
    if obstacle_grid.query_imprint_all(*coords, *grid_imprint, |field| field.is_empty()) && wisps_grid[*coords].is_empty()
    {
        commands.spawn(BuilderWisp::new(wisp_type, *coords));
    }
}

pub fn on_wisp_remove_request(
    _trigger: On<lib_core::placement::RemoveRequest<WispType>>,
    mut commands: Commands,
    mut wisps_grid: ResMut<WispsGrid>,
    placer: Single<&GridCoords, With<GridObjectPlacer>>,
    wisps: Query<Entity, With<Wisp>>,
) {
    let coords = placer.into_inner();
    let wisp_entities = wisps_grid[*coords].clone();
    for wisp_entity in wisp_entities {
        if wisps.contains(wisp_entity) {
            wisps_grid.wisp_remove(*coords, wisp_entity);
            commands.entity(wisp_entity).despawn();
        }
    }
}