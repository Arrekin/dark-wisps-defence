use bevy::{platform::collections::HashMap, prelude::*};

use alteration::{effects::prelude::*, modifiers::prelude::*};
use almanach::prelude::*;
use game_core::prelude::*;
use grids::{
    placement::{GridObjectPlacer, GridsCollectionParam, PlacementValidity, PlaceRequest, RemoveRequest},
    wisps::WispsGrid,
};
use logging::prelude::*;
use persistence::{prelude::*, rusqlite};
use resources::prelude::*;
use wisps::{WispElectricType, WispFireType, WispLightType, WispWaterType, prelude::*};

/// Resources added to the stock when the wisp dies.
#[derive(Component)]
pub(crate) struct DeathLoot(pub Vec<ResourceAmount>);

#[derive(Component, SSS)]
pub(crate) struct BuilderWisp {
    pub wisp_type: WispType,
    pub grid_coords: GridCoords,
    /// Saved integrity points. `None` ⇒ defer to baseline (fresh spawn);
    /// `Some` ⇒ override (restore).
    pub integrity_points: Option<IntegrityPoints>,
    /// Saved world position. `None` ⇒ compute from grid_coords (fresh spawn);
    /// `Some` ⇒ use as-is (restore mid-flight wisp).
    pub world_position: Option<Vec2>,
}

impl BuilderWisp {
    pub fn new(wisp_type: WispType, grid_coords: GridCoords) -> Self {
        Self { wisp_type, grid_coords, integrity_points: None, world_position: None }
    }
    pub fn with_integrity_points(mut self, integrity_points: f32) -> Self {
        self.integrity_points = Some(IntegrityPoints::new(integrity_points));
        self
    }
    pub fn with_world_position(mut self, world_position: Vec2) -> Self {
        self.world_position = Some(world_position);
        self
    }

    pub fn on_builder_add_spawn_wisp(
        trigger: On<Add<BuilderWisp>>,
        mut commands: Commands,
        mut wisps_grid: ResMut<WispsGrid>,
        builders: Query<&BuilderWisp>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return; };

        let mut entity_commands = commands.entity(entity);
        entity_commands
            .remove::<BuilderWisp>()
            .insert_some(builder.integrity_points);

        let translation = builder.world_position
            .unwrap_or_else(|| builder.grid_coords.to_world_position_centered(WISP_GRID_IMPRINT))
            .extend(0.);

        match builder.wisp_type {
            WispType::Fire => entity_commands.insert((WispFireType, DeathLoot(vec![ResourceAmount::new(EssenceType::Fire, 1)]))),
            WispType::Water => entity_commands.insert((WispWaterType, DeathLoot(vec![ResourceAmount::new(EssenceType::Water, 1)]))),
            WispType::Light => entity_commands.insert((WispLightType, DeathLoot(vec![ResourceAmount::new(EssenceType::Light, 1)]))),
            WispType::Electric => entity_commands.insert((WispElectricType, DeathLoot(vec![ResourceAmount::new(EssenceType::Electric, 1)]))),
        };
        entity_commands
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
            ))
            .trigger(move |entity| ObjectFaceRequest::world(entity, MapObject::Wisp(builder.wisp_type)));
        wisps_grid.wisp_add(builder.grid_coords, entity);
    }
}

#[log_tags(Tag::GameSave)]
pub(crate) fn collect_wisps(
    wisps: Query<(Entity, &WispType, &GridCoords, &IntegrityPoints, &Transform, &WispState), With<Wisp>>,
    mut save: SaveWriter,
) {
    if wisps.is_empty() { return; }

    #[debug_dev("Saving {} wisps", rows.len())]
    let rows: Vec<(u32, WispType, GridCoords, f32, Vec2)> = wisps
        .iter()
        .map(|(entity, wisp_type, coords, integrity_points, transform, wisp_state)| {
            // TODO: Once the wisps logic is mature, save the full wisp state properly. Right now we are ignoring some states (for example, attacking) and simply allow wisp to retarget on spawn, and continue from there.
            let world_position = if matches!(wisp_state, WispState::Attacking) {
                coords.to_world_position_centered(WISP_GRID_IMPRINT)
            } else {
                transform.translation.xy()
            };
            (
                entity.index_u32(),
                *wisp_type,
                *coords,
                integrity_points.get_current(),
                world_position,
            )
        })
        .collect();
    save.submit(move |ctx| {
        for (id, wisp_type, coords, integrity_points, position) in rows {
            ctx.register_entity(id)?;
            ctx.save_world_position(id, position)?;
            ctx.save_grid_coords(id, coords)?;
            ctx.save_integrity_points(id, integrity_points)?;
            ctx.tx.execute(
                "INSERT OR REPLACE INTO wisps (id, wisp_type) VALUES (?1, ?2)",
                rusqlite::params![id, wisp_type.as_ref()],
            )?;
        }
        Ok(())
    });
}

pub(crate) fn load_wisps(ctx: &mut LoadContext) -> LoadResult {
    ctx.for_each_entity("SELECT id, wisp_type FROM wisps", |ctx, old_id, entity, row| {
        let wisp_type = row.get_parsed::<WispType>(1)?;
        let grid_coords = ctx.grid_coords(old_id)?;
        let integrity_points = ctx.integrity_points(old_id)?;
        let world_position = ctx.world_position(old_id)?;
        let builder = BuilderWisp::new(wisp_type, grid_coords)
            .with_integrity_points(integrity_points)
            .with_world_position(world_position);
        ctx.insert(entity, builder);
        Ok(())
    })
}

pub(crate) fn wisp_validator(
    _: MapObject,
    origin: GridCoords,
    imprint: GridImprint,
    grids: &GridsCollectionParam,
) -> PlacementValidity {
    if !origin.are_in_bounds(grids.obstacle_grid.bounds) {
        return PlacementValidity::Invalid;
    }
    if !grids.obstacle_grid.query_imprint_all(origin, imprint, |field| field.is_empty()) {
        return PlacementValidity::Invalid;
    }
    if !grids.wisps_grid[origin].is_empty() {
        return PlacementValidity::Invalid;
    }
    PlacementValidity::Valid
}

#[log_tags(Tag::Wave)]
pub(crate) fn on_wisp_place_request_do_so(
    trigger: On<PlaceRequest>,
    mut commands: Commands,
    almanach: Res<Almanach>,
    grids: GridsCollectionParam,
    placer: Single<(&GridCoords, &GridImprint), With<GridObjectPlacer>>,
) {
    let PlaceRequest(MapObject::Wisp(wisp_type)) = *trigger else { return };
    let (coords, grid_imprint) = placer.into_inner();

    let validity = (almanach.wisps.validate)(MapObject::Wisp(wisp_type), *coords, *grid_imprint, &grids);
    if validity == PlacementValidity::Invalid { return; }
    #[debug_dev("{} wisp placed at {coords}", wisp_type.as_ref())]
    commands.spawn(BuilderWisp::new(wisp_type, *coords));
}

#[log_tags(Tag::Wave)]
pub(crate) fn on_wisp_remove_request_do_so(
    trigger: On<RemoveRequest>,
    mut commands: Commands,
    mut wisps_grid: ResMut<WispsGrid>,
    wisps: Query<Entity, With<Wisp>>,
    placer: Single<&GridCoords, With<GridObjectPlacer>>,
) {
    let RemoveRequest(MapObject::Wisp(_)) = *trigger else { return };
    let coords = placer.into_inner();
    let wisp_entities = wisps_grid[*coords].clone();
    for wisp_entity in wisp_entities {
        #[debug_dev("Wisp removed at {coords}")]
        if wisps.contains(wisp_entity) {
            wisps_grid.wisp_remove(*coords, wisp_entity);
            commands.entity(wisp_entity).despawn();
        }
    }
}
