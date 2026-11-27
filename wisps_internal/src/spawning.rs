use bevy::{ecs::system::SystemParam, prelude::*};

use alteration::{effects::prelude::*, modifiers::prelude::*};
use game_core::prelude::*;
use grids::placement::{
    BeginPlacing, PlaceRequest, PlaceResult, PlacementFeedback, PlacementModes, PlacementObject, PlacementSession, PlacementValidity,
    RemoveRequest, RemoveResult, ReservedCoords, UiPlacementSession,
};
use indexes::{prelude::ObstacleGrid, wisps::WispsGrid};
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
    pub grid_position: GridPosition,
    /// Integrity override for loading a saved wisp; `None` uses the baseline.
    pub integrity_points: Option<IntegrityPoints>,
    /// Exact world position for loading a moving wisp; `None` uses the center of its grid area.
    pub world_position: Option<Vec2>,
}

impl BuilderWisp {
    pub fn new(wisp_type: WispType, grid_position: GridPosition) -> Self {
        Self { wisp_type, grid_position, integrity_points: None, world_position: None }
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

        let area = GridArea::new(builder.grid_position, WISP_GRID_IMPRINT);
        let translation = builder.world_position
            .unwrap_or_else(|| area.world_center())
            .extend(0.);

        match builder.wisp_type {
            WispType::Fire => entity_commands.insert((WispFireType, DeathLoot(vec![ResourceAmount::new(EssenceType::Fire, 1)]))),
            WispType::Water => entity_commands.insert((WispWaterType, DeathLoot(vec![ResourceAmount::new(EssenceType::Water, 1)]))),
            WispType::Light => entity_commands.insert((WispLightType, DeathLoot(vec![ResourceAmount::new(EssenceType::Light, 1)]))),
            WispType::Electric => entity_commands.insert((WispElectricType, DeathLoot(vec![ResourceAmount::new(EssenceType::Electric, 1)]))),
        };
        entity_commands
            .insert((
                area,
                Transform::from_translation(translation),
                DrawLevel::from(builder.grid_position.level),
                Wisp,
                builder.wisp_type,
                related![EffectInstances[
                    (ModifierContributions::from([
                        (ModifierType::MaxIntegrityPoints, 10.),
                        (ModifierType::MaxAttackRange, 1.),
                        (ModifierType::MovementSpeedFlat, 60.),
                    ]), BaselineEffect),
                ]],
            ))
            .trigger(move |entity| ObjectFaceRequest::world(entity, MapObject::Wisp(builder.wisp_type)));
        wisps_grid.wisp_add(builder.grid_position, entity);
    }
}

#[log_tags(Tag::GameSave)]
pub(crate) fn collect_wisps(
    wisps: Query<(Entity, &WispType, &GridArea, &IntegrityPoints, &Transform, &WispState), With<Wisp>>,
    mut save: SaveWriter,
) {
    if wisps.is_empty() { return; }

    #[debug_dev("Saving {} wisps", rows.len())]
    let rows: Vec<(u32, WispType, GridPosition, f32, Vec2)> = wisps
        .iter()
        .map(|(entity, wisp_type, area, integrity_points, transform, wisp_state)| {
            // TODO: Once the wisps logic is mature, save the full wisp state properly. Right now we
            // are ignoring some states (for example, attacking) and simply allow wisp to retarget on
            // spawn, and continue from there.
            let world_position = if matches!(wisp_state, WispState::Attacking) {
                area.world_center()
            } else {
                transform.translation.xy()
            };
            (
                entity.index_u32(),
                *wisp_type,
                area.origin,
                integrity_points.current,
                world_position,
            )
        })
        .collect();
    save.submit(move |ctx| {
        for (id, wisp_type, grid_position, integrity_points, position) in rows {
            ctx.register_entity(id)?;
            ctx.save_world_position(id, position)?;
            ctx.save_grid_position(id, grid_position)?;
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
        let grid_position = ctx.grid_position(old_id)?;
        let integrity_points = ctx.integrity_points(old_id)?;
        let world_position = ctx.world_position(old_id)?;
        let builder = BuilderWisp::new(wisp_type, grid_position)
            .with_integrity_points(integrity_points)
            .with_world_position(world_position);
        ctx.insert(entity, builder);
        Ok(())
    })
}

/// Initializes a wisp placement session at `trigger.origin` with drag placement enabled.
pub(crate) fn on_begin_placing_set_up_wisp_placement_session(
    trigger: On<BeginPlacing>,
    mut commands: Commands,
    placement_objects: Query<&PlacementObject>,
) {
    let Ok(&PlacementObject(MapObject::Wisp(_))) = placement_objects.get(trigger.placement_session) else { return };
    commands.entity(trigger.placement_session).insert((GridArea::new(trigger.origin, WISP_GRID_IMPRINT), PlacementModes::on_press()));
}

/// Validates, places, and removes wisps.
#[derive(SystemParam)]
pub(crate) struct WispPlacementRules<'w, 's> {
    commands: Commands<'w, 's>,
    obstacle_grid: Res<'w, ObstacleGrid>,
    wisps_grid: ResMut<'w, WispsGrid>,
    reserved_coords: Res<'w, ReservedCoords>,
}
impl WispPlacementRules<'_, '_> {
    /// Requires every cell to be unreserved with a free top level.
    fn feedback(&self, area: GridArea) -> PlacementFeedback {
        PlacementFeedback::every_cell(area, &self.reserved_coords, |coords| self.can_take_wisp(coords))
    }

    /// Checks that no structure and no wisp occupies the cell's top level.
    fn can_take_wisp(&self, coords: GridCoords) -> bool {
        if !self.obstacle_grid.bounds.contains(coords) { return false; }
        let top = self.obstacle_grid.top_position(coords);
        self.obstacle_grid[top].is_empty() && self.wisps_grid.wisps_at(top).is_empty()
    }

    #[log_tags(Tag::Wave)]
    fn place(&mut self, area: GridArea, wisp_type: WispType) -> Result<(), PlaceResult> {
        if self.feedback(area).validity == PlacementValidity::Invalid { return Err(PlaceResult::Invalid); }
        let position = self.obstacle_grid.top_position(area.origin.coords);
        #[debug_player("{} wisp placed at {}", wisp_type.as_ref(), position)]
        self.commands.spawn(BuilderWisp::new(wisp_type, position));
        Ok(())
    }

    #[log_tags(Tag::Wave)]
    fn remove(&mut self, coords: GridCoords, wisps: &Query<&GridArea, With<Wisp>>) -> Result<(), RemoveResult> {
        if !self.wisps_grid.bounds.contains(coords) { return Err(RemoveResult::NothingToRemove); }
        let wisp_entities: Vec<Entity> = self.wisps_grid.wisps_in_cell(coords).collect();
        let mut removed_any = false;
        for wisp_entity in wisp_entities {
            let Ok(wisp_area) = wisps.get(wisp_entity) else { continue };
            #[debug_player("Wisp removed at {coords}")]
            self.wisps_grid.wisp_remove(wisp_area.origin, wisp_entity);
            self.commands.entity(wisp_entity).despawn();
            removed_any = true;
        }
        if removed_any { Ok(()) } else { Err(RemoveResult::NothingToRemove) }
    }

    pub(crate) fn update_feedback(
        mut rules: WispPlacementRules,
        ui_placement_session: Single<(Entity, &PlacementObject, &GridArea), With<UiPlacementSession>>,
    ) {
        let (ui_placement_session_entity, &PlacementObject(MapObject::Wisp(_)), &area) = *ui_placement_session else { return };
        let feedback = rules.feedback(area);
        rules.commands.entity(ui_placement_session_entity).insert_if_neq(feedback);
    }

    pub(crate) fn on_place_request_do_so(
        trigger: On<PlaceRequest>,
        mut rules: WispPlacementRules,
        placement_sessions: Query<(&PlacementObject, Option<&GridArea>), With<PlacementSession>>,
    ) {
        let Ok((&PlacementObject(MapObject::Wisp(wisp_type)), area)) = placement_sessions.get(trigger.placement_session) else { return };
        let outcome = area.ok_or(PlaceResult::InvalidSession).and_then(|&area| rules.place(area, wisp_type));
        trigger.report(&mut rules.commands, outcome);
    }

    pub(crate) fn on_remove_request_do_so(
        trigger: On<RemoveRequest>,
        mut rules: WispPlacementRules,
        placement_sessions: Query<(&PlacementObject, Option<&GridArea>), With<PlacementSession>>,
        wisps: Query<&GridArea, With<Wisp>>,
    ) {
        let Ok((&PlacementObject(MapObject::Wisp(_)), area)) = placement_sessions.get(trigger.placement_session) else { return };
        let outcome = area.ok_or(RemoveResult::InvalidSession).and_then(|area| rules.remove(area.origin.coords, &wisps));
        trigger.report(&mut rules.commands, outcome);
    }
}
