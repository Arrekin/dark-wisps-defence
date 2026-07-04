use std::str::FromStr;

use bevy::{platform::collections::HashMap, prelude::*};

use alteration::{effects::prelude::*, modifiers::prelude::*};
use almanach::prelude::*;
use game_core::prelude::*;
use grids::{placement::{GridsCollectionParam, PlacementValidity}, prelude::*};
use logging::prelude::*;
use persistence::{
    prelude::{GameDbHelpers, LoadContext, SaveWriter},
    rusqlite,
};
use resources::prelude::*;
use wisps::{WispElectricType, WispFireType, WispLightType, WispWaterType, prelude::*};

use super::materials::WispMaterial;

#[derive(Component, SSS)]
pub(crate) struct BuilderWisp {
    pub wisp_type: WispType,
    pub grid_coords: GridCoords,
    /// Saved integrity points. `None` ⇒ defer to baseline (fresh spawn);
    /// `Some` ⇒ override (restore).
    pub integrity_points: Option<f32>,
    /// Saved world position. `None` ⇒ compute from grid_coords (fresh spawn);
    /// `Some` ⇒ use as-is (restore mid-flight wisp).
    pub world_position: Option<Vec2>,
}

impl BuilderWisp {
    pub fn new(wisp_type: WispType, grid_coords: GridCoords) -> Self {
        Self { wisp_type, grid_coords, integrity_points: None, world_position: None }
    }
    pub fn with_integrity_points(mut self, integrity_points: f32) -> Self {
        self.integrity_points = Some(integrity_points);
        self
    }
    pub fn with_world_position(mut self, world_position: Vec2) -> Self {
        self.world_position = Some(world_position);
        self
    }

    pub fn on_builder_add_spawn_wisp(
        trigger: On<Add, BuilderWisp>,
        mut commands: Commands,
        mut wisps_grid: ResMut<WispsGrid>,
        builders: Query<&BuilderWisp>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return; };

        let mut entity_commands = commands.entity(entity);

        if let Some(ip) = builder.integrity_points {
            entity_commands.insert(IntegrityPoints::new(ip));
        }

        let translation = if let Some(pos) = builder.world_position {
            pos.extend(Z_WISP)
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
                Transform::from_translation(translation),
                Wisp,
                builder.wisp_type,
                related![EffectInstances[
                    (ModifierContributions(HashMap::from([
                        (ModifierType::MaxIntegrityPoints, 10.),
                        (ModifierType::AttackRange, 1.),
                        (ModifierType::MovementSpeed, 60.),
                    ])), BaselineEffect),
                ]],
            ));
        wisps_grid.wisp_add(builder.grid_coords, entity);
    }
}

pub(crate) fn collect_wisps(
    wisps: Query<(Entity, &WispType, &GridCoords, &IntegrityPoints, &Transform, &WispState), With<Wisp>>,
    mut save: SaveWriter,
) {
    if wisps.is_empty() { return; }
    let rows: Vec<(i64, String, i32, i32, f32, f32, f32)> = wisps
        .iter()
        .map(|(entity, wisp_type, coords, integrity_points, transform, wisp_state)| {
            // TODO: Once the wisps logic is mature, save the full wisp state properly. Right now we are ignoring some states(for exmample, attacking) and simply allow wisp to retarget on spawn, and continue from there.
            let world_position = if matches!(wisp_state, WispState::Attacking) {
                coords.to_world_position_centered(WISP_GRID_IMPRINT)
            } else {
                transform.translation.xy()
            };
            (
                entity.index_u32() as i64,
                wisp_type.as_ref().to_string(),
                coords.x,
                coords.y,
                integrity_points.get_current(),
                world_position.x,
                world_position.y,
            )
        })
        .collect();
    save.submit(move |tx| {
        for (id, type_str, gx, gy, integrity_points, pos_x, pos_y) in rows {
            tx.register_entity(id)?;
            tx.save_world_position(id, Vec2::new(pos_x, pos_y))?;
            tx.save_grid_coords(id, GridCoords { x: gx, y: gy })?;
            tx.save_integrity_points(id, integrity_points)?;
            tx.execute(
                "INSERT OR REPLACE INTO wisps (id, wisp_type) VALUES (?1, ?2)",
                rusqlite::params![id, type_str],
            )?;
        }
        Ok(())
    });
}

pub(crate) fn load_wisps(ctx: &mut LoadContext) -> rusqlite::Result<()> {
    let mut stmt = ctx.conn.prepare("SELECT id, wisp_type FROM wisps")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let old_id: i64 = row.get(0)?;
        let type_str: String = row.get(1)?;

        let Ok(wisp_type) = WispType::from_str(&type_str) else {
            Log::warn().dev().tag(Tag::GameLoad).message(format!("Unknown WispType '{type_str}'"));
            continue;
        };

        let grid_coords = ctx.conn.get_grid_coords(old_id)?;
        let integrity_points = ctx.conn.get_integrity_points(old_id)?;
        let world_position = ctx.conn.get_world_position(old_id)?;

        let Some(entity) = ctx.entity(old_id) else { continue; };
        let builder = BuilderWisp::new(wisp_type, grid_coords)
            .with_integrity_points(integrity_points)
            .with_world_position(world_position);
        ctx.insert(entity, builder);
    }
    Ok(())
}

pub(crate) fn wisp_validator(
    _: MapObject,
    coords: GridCoords,
    imprint: GridImprint,
    grids: &GridsCollectionParam,
) -> PlacementValidity {
    if !coords.is_in_bounds(grids.obstacle_grid.bounds()) {
        return PlacementValidity::Invalid;
    }
    if !grids.obstacle_grid.query_imprint_all(coords, imprint, |f| f.is_empty()) {
        return PlacementValidity::Invalid;
    }
    if !grids.wisps_grid[coords].is_empty() {
        return PlacementValidity::Invalid;
    }
    PlacementValidity::Valid
}

pub(crate) fn on_wisp_spawn_attach_material<WispT: Component, MaterialT: Asset + WispMaterial>(
    trigger: On<Add, WispT>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<MaterialT>>,
    wisps: Query<(), With<WispT>>,
) {
    let entity = trigger.entity;
    if !wisps.contains(entity) { return; }
    let wisp_world_size = WISP_GRID_IMPRINT.world_size() * MaterialT::mesh_scale();
    let mesh = meshes.add(Rectangle::new(wisp_world_size.x, wisp_world_size.y));
    let material = materials.add(MaterialT::make(&asset_server));
    commands.entity(entity).insert((
        Mesh2d(mesh),
        MeshMaterial2d(material),
    ));
}

pub(crate) fn on_wisp_place_request_do_so(
    _trigger: On<PlaceRequest<WispType>>,
    mut commands: Commands,
    almanach: Res<Almanach>,
    grids: GridsCollectionParam,
    placer: Single<(&GridObjectPlacer, &GridCoords, &GridImprint)>,
) {
    let (grid_object_placer, coords, grid_imprint) = placer.into_inner();
    let Some(active_placement) = &grid_object_placer.active_placement else { return };
    let MapObject::Wisp(wisp_type) = active_placement.map_object else { return };

    let validity = (almanach.wisps.validate)(active_placement.map_object, *coords, *grid_imprint, &grids);
    if validity == PlacementValidity::Invalid { return; }
    commands.spawn(BuilderWisp::new(wisp_type, *coords));
}

pub(crate) fn on_wisp_remove_request_do_so(
    _trigger: On<RemoveRequest<WispType>>,
    mut commands: Commands,
    mut wisps_grid: ResMut<WispsGrid>,
    wisps: Query<Entity, With<Wisp>>,
    placer: Single<&GridCoords, With<GridObjectPlacer>>,
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
