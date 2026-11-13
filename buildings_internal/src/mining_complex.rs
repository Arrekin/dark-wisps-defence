use bevy::{
    ecs::system::SystemParam,
    platform::collections::HashMap,
    prelude::*,
};
use nanorand::Rng;

use almanach::{BuildingInfo, prelude::*};
use alteration::{
    effects::prelude::*,
    modifiers::prelude::*,
};
use buildings::prelude::*;
use game_core::prelude::*;
use grids::placement::{CellHighlight, PlacementFeedback, PlacementObject, PlacementSession, PlacementValidity, PlacementFeedbackSystems, PlaceRequest};
use hud::prelude::{IndicatorDisplay, IndicatorType, Indicators};
use logging::prelude::*;
use map_objects::{
    DarkOre,
    prelude::*,
};
use persistence::prelude::*;
use resources::prelude::*;
use states::prelude::*;

use crate::common::*;

pub(crate) struct MiningComplexPlugin;
impl Plugin for MiningComplexPlugin {
    fn build(&self, app: &mut App) {
        let almanach_info = BuilderMiningComplex::almanach_info(app.world().resource::<AssetServer>());
        app
            .add_systems(Update, (
                mine_ore.run_if(in_state(GameState::Running)),
                MiningComplexPlacementRules::update_feedback.in_set(PlacementFeedbackSystems),
            ))
            .add_observer(BuilderMiningComplex::on_builder_add_spawn_mining_complex)
            .add_observer(MiningComplexPlacementRules::on_place_request_do_so)
            .add_systems(CollectSave, collect_mining_complexes)
            .register_loader(MapLoadingStage::SpawnMapElements, "mining_complexes", load_mining_complexes)
            .register_building(BuildingType::MiningComplex, almanach_info);
    }
}

/// Validates and places mining complexes on sites containing dark ore.
#[derive(SystemParam)]
pub(crate) struct MiningComplexPlacementRules<'w, 's> {
    building_rules: BuildingPlacementRules<'w, 's>,
}
impl MiningComplexPlacementRules<'_, '_> {
    /// Requires unreserved flat ground without structures or quantum fields, and at least one
    /// dark ore cell. Highlights ore on valid sites and checks power and affordability.
    fn feedback(&self) -> PlacementFeedback {
        let rules = &self.building_rules;
        let area = rules.placement_area();
        let feedback = rules.with_cost_check(PlacementFeedback::every_cell(area, &rules.reserved_coords, |coords| {
            rules.obstacle_grid.get(coords).is_some_and(|cell| !cell.has_structure() && !cell.is_within_quantum_field() && cell.has_flat_ground())
        }));
        if feedback.validity == PlacementValidity::Invalid { return feedback; }

        let ore_cells: Vec<_> = area.iter()
            .filter(|coords| rules.obstacle_grid.cell(*coords).has_dark_ore())
            .map(|coords| (coords, CellHighlight::Positive))
            .collect();
        if ore_cells.is_empty() { return feedback.with_validity(PlacementValidity::Invalid); }
        rules.with_power_check(feedback.with_highlights(ore_cells))
    }

    fn update_feedback(
        mut rules: MiningComplexPlacementRules,
        placement_session: Single<(Entity, &PlacementObject), With<PlacementSession>>,
    ) {
        let (placement_session, object) = *placement_session;
        if object.0 != MapObject::Building(BuildingType::MiningComplex) { return; }
        let feedback = rules.feedback();
        rules.building_rules.commands.entity(placement_session).insert_if_neq(feedback);
    }

    fn on_place_request_do_so(
        trigger: On<PlaceRequest>,
        mut rules: MiningComplexPlacementRules,
    ) {
        let PlaceRequest(MapObject::Building(BuildingType::MiningComplex)) = *trigger else { return };
        let Some(coords) = rules.building_rules.claim(rules.feedback().validity) else { return };
        rules.building_rules.commands.spawn(BuilderMiningComplex::new(coords));
    }
}

#[derive(Component)]
pub(crate) struct MiningComplexDeliveryTimer(pub Timer);

#[derive(Component, SSS)]
pub(crate) struct BuilderMiningComplex {
    pub grid_position: GridPosition,
    /// Custom integrity points, or `None` to use the fresh-spawn baseline.
    pub integrity_points: Option<IntegrityPoints>,
    /// Set when the player disabled this building. `None` on fresh spawn.
    pub disabled_by_player: Option<DisabledByPlayer>,
}
impl BuilderMiningComplex {
    pub fn almanach_info(asset_server: &AssetServer) -> BuildingInfo {
        BuildingInfo {
            name: "Mining Complex".to_string(),
            description: "Extracts dark ore from deposits in range.".to_string(),
            sprite: asset_server.load("buildings/mining_complex.png"),
            top_sprite: None,
            grid_imprint: GridImprint::square(3),
            attack_range: None,
            cost: vec![ResourceAmount::new(ResourceType::DarkOre, 100)],
            baseline: HashMap::from([(ModifierType::MaxIntegrityPoints, 100.)]),
            sockets: vec![],
        }
    }

    pub fn new(grid_position: GridPosition) -> Self {
        Self { grid_position, integrity_points: None, disabled_by_player: None }
    }
    pub fn with_integrity_points(mut self, integrity_points: f32) -> Self {
        self.integrity_points = Some(IntegrityPoints::new(integrity_points));
        self
    }
    pub fn with_disabled_by_player(mut self, disabled_by_player: bool) -> Self {
        self.disabled_by_player = disabled_by_player.then_some(DisabledByPlayer);
        self
    }

    pub fn on_builder_add_spawn_mining_complex(
        trigger: On<Add<BuilderMiningComplex>>,
        mut commands: Commands,
        almanach: Res<Almanach>,
        builders: Query<&BuilderMiningComplex>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return; };

        let building_info = almanach.get_building_info(BuildingType::MiningComplex);
        let area = GridArea::new(builder.grid_position, building_info.grid_imprint);

        commands.entity(entity)
            .remove::<BuilderMiningComplex>()
            .insert_some(builder.integrity_points)
            .insert_some(builder.disabled_by_player)
            .insert((
                MiningComplex,
                building_info.world_sprite(),
                area,
                NeedsPower,
                DarkOreAreaScanner { range: area },
                MiningComplexDeliveryTimer(Timer::from_seconds(1.0, TimerMode::Repeating)),
                related![Indicators[
                    IndicatorType::NoPower,
                    IndicatorType::OreDepleted,
                    IndicatorType::DisabledByPlayer,
                ]],
                related![EffectInstances[
                    (ModifierContributions(building_info.baseline.clone()), BaselineEffect),
                ]],
                children![
                    IndicatorDisplay::default(),
                ],
            ))
            .observe(on_technical_state_changed_recompute_operational);
        commands.trigger(TechnicalStateChanged { entity, kind: TechnicalChange::JustSpawned });
    }
}

#[log_tags(Tag::GameSave)]
fn collect_mining_complexes(
    mining_complexes: Query<(Entity, &GridArea, &IntegrityPoints, Has<DisabledByPlayer>), With<MiningComplex>>,
    mut save: SaveWriter,
) {
    if mining_complexes.is_empty() { return; }

    #[debug_dev("Saving {} mining complexes", rows.len())]
    let rows: Vec<(u32, GridPosition, f32, bool)> = mining_complexes
        .iter()
        .map(|(entity, area, integrity_points, disabled_by_player)| {
            (
                entity.index_u32(),
                area.origin,
                integrity_points.current,
                disabled_by_player,
            )
        })
        .collect();
    save.submit(move |ctx| {
        for (id, grid_position, integrity_points, disabled_by_player) in rows {
            ctx.save_marker("mining_complexes", id)?;
            ctx.save_grid_position(id, grid_position)?;
            ctx.save_integrity_points(id, integrity_points)?;
            if disabled_by_player {
                ctx.save_disabled_by_player(id)?;
            }
        }
        Ok(())
    });
}

fn load_mining_complexes(ctx: &mut LoadContext) -> LoadResult {
    ctx.for_each_entity("SELECT id FROM mining_complexes", |ctx, old_id, entity, _| {
        let grid_position = ctx.grid_position(old_id)?;
        let integrity_points = ctx.integrity_points(old_id)?;
        let disabled_by_player = ctx.disabled_by_player(old_id)?;
        let builder = BuilderMiningComplex::new(grid_position)
            .with_integrity_points(integrity_points)
            .with_disabled_by_player(disabled_by_player);
        ctx.insert(entity, builder);
        Ok(())
    })
}

fn mine_ore(
    mut stock: ResMut<Stock>,
    time: Res<Time>,
    mut mining_complexes: Query<(&mut MiningComplexDeliveryTimer, &DarkOreInRange), (With<MiningComplex>, With<IsOperational>)>,
    mut dark_ores: Query<&mut DarkOre>,
) {
    let mut rng = nanorand::tls_rng();
    for (mut timer, ore_in_range) in mining_complexes.iter_mut() {
        let ore_in_range = &ore_in_range.0;
        if ore_in_range.is_empty() { continue; }
        timer.0.tick(time.delta());
        if timer.0.just_finished() {
            let ore_index = rng.generate_range(0..ore_in_range.len());
            let ore_entity = ore_in_range[ore_index];
            if let Ok(mut dark_ore) = dark_ores.get_mut(ore_entity) {
                let mined_amount = std::cmp::min(dark_ore.amount, 100);
                stock.add((ResourceType::DarkOre, mined_amount));
                dark_ore.amount -= mined_amount;
            }
        }
    }
}
