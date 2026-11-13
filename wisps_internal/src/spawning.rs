use bevy::{ecs::system::SystemParam, platform::collections::HashMap, prelude::*};

use alteration::{effects::prelude::*, modifiers::prelude::*};
use game_core::prelude::*;
use grids::{base::GridCell, placement::{BeginPlacing, PlacementFeedback, PlacementModes, PlacementObject, PlacementSession, PlacementValidity, PlaceRequest, RemoveRequest, ReservedCoords}};
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
    pub grid_position: GridVolumePosition,
    /// Integrity override for loading a saved wisp; `None` uses the baseline.
    pub integrity_points: Option<IntegrityPoints>,
    /// Exact world position for loading a moving wisp; `None` uses the center of its grid area.
    pub world_position: Option<Vec2>,
}

impl BuilderWisp {
    pub fn new(wisp_type: WispType, grid_position: GridVolumePosition) -> Self {
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
                    (ModifierContributions(HashMap::from([
                        (ModifierType::MaxIntegrityPoints, 10.),
                        (ModifierType::MaxAttackRange, 1.),
                        (ModifierType::MovementSpeed, 60.),
                    ])), BaselineEffect),
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
        let grid_position = ctx.grid_position(old_id)?.volume_position();
        let integrity_points = ctx.integrity_points(old_id)?;
        let world_position = ctx.world_position(old_id)?;
        let builder = BuilderWisp::new(wisp_type, grid_position)
            .with_integrity_points(integrity_points)
            .with_world_position(world_position);
        ctx.insert(entity, builder);
        Ok(())
    })
}

pub(crate) fn on_begin_placing_start_wisp_session(
    trigger: On<BeginPlacing>,
    mut commands: Commands,
    placement_session: Single<Entity, With<PlacementSession>>,
) {
    let BeginPlacing { object: MapObject::Wisp(_), origin } = *trigger else { return };
    commands.entity(*placement_session).insert((GridArea::new(origin, WISP_GRID_IMPRINT), PlacementModes::on_press()));
}

/// Validates manual wisp placement and handles placement and removal requests.
#[derive(SystemParam)]
pub(crate) struct WispPlacementRules<'w, 's> {
    commands: Commands<'w, 's>,
    obstacle_grid: Res<'w, ObstacleGrid>,
    wisps_grid: ResMut<'w, WispsGrid>,
    reserved_coords: Res<'w, ReservedCoords>,
    placement_session: Single<'w, 's, &'static GridArea, With<PlacementSession>>,
}
impl WispPlacementRules<'_, '_> {
    fn placement_area(&self) -> GridArea {
        **self.placement_session
    }

    /// Requires every cell to be unreserved, support a wisp and contain none.
    fn feedback(&self) -> PlacementFeedback {
        PlacementFeedback::every_cell(self.placement_area(), &self.reserved_coords, |coords| self.can_take_wisp(coords))
    }

    /// Checks for a supported top position and no existing wisp at any level in the cell.
    fn can_take_wisp(&self, coords: GridCoords) -> bool {
        let Some(cell) = self.obstacle_grid.get(coords) else { return false; };
        let top = cell.top_level();
        let can_hold_wisp = cell.is_placeable_ground()
            || top == VolumeLevel::Underground
            || top.below().is_some_and(|below| cell.at(below).is_ramp());
        can_hold_wisp && self.wisps_grid.wisps_in_cell(coords).next().is_none()
    }

    pub(crate) fn update_feedback(
        mut rules: WispPlacementRules,
        placement_session: Single<(Entity, &PlacementObject), With<PlacementSession>>,
    ) {
        let (placement_session, object) = *placement_session;
        if !matches!(object.0, MapObject::Wisp(_)) { return; }
        let feedback = rules.feedback();
        rules.commands.entity(placement_session).insert_if_neq(feedback);
    }

    #[log_tags(Tag::Wave)]
    pub(crate) fn on_place_request_do_so(
        trigger: On<PlaceRequest>,
        mut rules: WispPlacementRules,
    ) {
        let PlaceRequest(MapObject::Wisp(wisp_type)) = *trigger else { return };
        if rules.feedback().validity == PlacementValidity::Invalid { return; }
        let position = rules.obstacle_grid.top_position(rules.placement_session.origin.coords);
        #[debug_player("{} wisp placed at {}", wisp_type.as_ref(), position)]
        rules.commands.spawn(BuilderWisp::new(wisp_type, position));
    }

    #[log_tags(Tag::Wave)]
    pub(crate) fn on_remove_request_do_so(
        trigger: On<RemoveRequest>,
        mut rules: WispPlacementRules,
        wisps: Query<&GridArea, With<Wisp>>,
    ) {
        let RemoveRequest(MapObject::Wisp(_)) = *trigger else { return };
        let coords = rules.placement_session.origin.coords;
        if !rules.wisps_grid.bounds.contains(coords) { return; }
        let wisp_entities: Vec<Entity> = rules.wisps_grid.wisps_in_cell(coords).collect();
        for wisp_entity in wisp_entities {
            #[debug_player("Wisp removed at {coords}")]
            if let Ok(wisp_area) = wisps.get(wisp_entity) {
                rules.wisps_grid.wisp_remove(wisp_area.origin.volume_position(), wisp_entity);
                rules.commands.entity(wisp_entity).despawn();
            }
        }
    }
}
