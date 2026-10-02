use bevy::{
    platform::collections::HashMap,
    prelude::*,
};

use alteration::{
    effects::prelude::*,
    modifiers::prelude::*,
};
use almanach::prelude::*;
use buildings::prelude::*;
use game_core::prelude::*;
use grids::{
    emissions::{EmissionsType, EmitterEnergy, FloodEmissionsDetails, FloodEmissionsEvaluator, FloodEmissionsMode},
    energy_supply::SupplierEnergy,
    placement::{annotate_non_empty, PlacementModes, PlaceRequest},
};
use hud::prelude::{IndicatorDisplay, IndicatorType, Indicators};
use logging::prelude::*;
use persistence::{
    prelude::{AppGameLoadSaveExtension, CollectSave, GameDbHelpers, LoadContext, SaveWriter},
    rusqlite,
};
use resources::prelude::*;
use states::prelude::*;
use visuals::prelude::*;

use crate::{common::*, tooltip::building_tooltip};

pub(crate) struct EnergyRelayPlugin;
impl Plugin for EnergyRelayPlugin {
    fn build(&self, app: &mut App) {
        let almanach_info = BuilderEnergyRelay::almanach_info(app.world().resource::<AssetServer>());
        app
            .add_observer(BuilderEnergyRelay::on_builder_add_spawn_energy_relay)
            .add_observer(on_energy_relay_place_request_do_so)
            .add_systems(CollectSave, collect_energy_relays)
            .register_loader(MapLoadingStage::SpawnMapElements, "energy_relays", load_energy_relays)
            .register_building(BuildingType::EnergyRelay, almanach_info);
    }
}

#[derive(Component, SSS)]
pub(crate) struct BuilderEnergyRelay {
    pub grid_position: GridCoords,
    /// Saved integrity points. `None` ⇒ defer to baseline (fresh spawn);
    /// `Some` ⇒ override with saved value (restore).
    pub integrity_points: Option<f32>,
    /// Whether the player disabled this building. False on fresh spawn.
    pub disabled_by_player: bool,
}
impl BuilderEnergyRelay {
    pub fn almanach_info(asset_server: &AssetServer) -> BuildingInfo {
        BuildingInfo {
            name: "Energy Relay".to_string(),
            description: "Relays energy over the ground it covers, allowing buildings to function far from Main Base.".to_string(),
            sprite: asset_server.load("buildings/energy_relay.png"),
            top_sprite: None,
            grid_imprint: GridImprint::Rectangle { width: 2, height: 2 },
            cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 300 }],
            baseline: HashMap::from([
                (ModifierType::MaxIntegrityPoints, 100.),
                (ModifierType::EnergySupplyRange, 12.),
            ]),
            validate: building_validator,
            annotate: annotate_non_empty,
            placement: PlacementModes::default(),
            presentation: ObjectPresentation {
                tooltip: Some(building_tooltip),
            },
        }
    }

    pub fn new(grid_position: GridCoords) -> Self {
        Self { grid_position, integrity_points: None, disabled_by_player: false }
    }
    pub fn with_integrity_points(mut self, integrity_points: f32) -> Self {
        self.integrity_points = Some(integrity_points);
        self
    }
    pub fn with_disabled_by_player(mut self) -> Self {
        self.disabled_by_player = true;
        self
    }

    pub fn on_builder_add_spawn_energy_relay(
        trigger: On<Add, BuilderEnergyRelay>,
        mut commands: Commands,
        almanach: Res<Almanach>,
        builders: Query<&BuilderEnergyRelay>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return; };

        let building_info = almanach.get_building_info(BuildingType::EnergyRelay);

        let mut entity_commands = commands.entity(entity);
        if let Some(integrity_points) = builder.integrity_points {
            entity_commands.insert(IntegrityPoints::new(integrity_points));
        }
        if builder.disabled_by_player {
            entity_commands.insert(DisabledByPlayer);
        }

        entity_commands
            .remove::<BuilderEnergyRelay>()
            .insert((
                EnergyRelay,
                Sprite {
                    image: building_info.sprite.clone(),
                    custom_size: Some(building_info.grid_imprint.world_size()),
                    color: Color::hsla(0., 0.2, 1.0, 1.0), // 1.6 is a good value if the pulsation is off.
                    ..default()
                },
                builder.grid_position,
                building_info.grid_imprint,
                NeedsPower,
                EmitterEnergy(FloodEmissionsDetails {
                    emissions_type: EmissionsType::Energy,
                    range: usize::MAX,
                    evaluator: FloodEmissionsEvaluator::ExponentialDecay { start_value: 100., decay: 0.1 },
                    mode: FloodEmissionsMode::Increase,
                }),
                SupplierEnergy,
                related![Indicators[
                    IndicatorType::NoPower,
                    IndicatorType::DisabledByPlayer,
                ]],
                related![EffectInstances[
                    (ModifierContributions(building_info.baseline.clone()), BaselineEffect),
                ]],
                children![
                    IndicatorDisplay::default(),
                ],
            ))
            .observe(on_technical_state_changed_recompute_operational)
            .observe(Self::on_add_is_operational_insert_color_pulsation)
            .observe(Self::on_remove_is_operational_remove_color_pulsation);
        commands.trigger(TechnicalStateChanged { entity, kind: TechnicalChange::JustSpawned });
    }
}

fn on_energy_relay_place_request_do_so(
    trigger: On<PlaceRequest>,
    mut commands: Commands,
    mut placement: BuildingPlacementManager,
) {
    let PlaceRequest(MapObject::Building(BuildingType::EnergyRelay)) = *trigger else { return };
    let Some(coords) = placement.claim(BuildingType::EnergyRelay) else { return };
    commands.spawn(BuilderEnergyRelay::new(coords));
}

#[log_tags(Tag::GameSave)]
fn collect_energy_relays(
    relays: Query<(Entity, &GridCoords, &IntegrityPoints, Has<DisabledByPlayer>), With<EnergyRelay>>,
    mut save: SaveWriter,
) {
    if relays.is_empty() { return; }

    #[debug_dev("Saving {} energy relays", rows.len())]
    let rows: Vec<(i64, GridCoords, f32, bool)> = relays
        .iter()
        .map(|(entity, coords, integrity_points, disabled_by_player)| {
            (
                entity.index_u32() as i64,
                *coords,
                integrity_points.get_current(),
                disabled_by_player,
            )
        })
        .collect();
    save.submit(move |tx| {
        for (id, coords, integrity_points, disabled_by_player) in rows {
            tx.save_marker("energy_relays", id)?;
            tx.save_grid_coords(id, coords)?;
            tx.save_integrity_points(id, integrity_points)?;
            if disabled_by_player {
                tx.save_disabled_by_player(id)?;
            }
        }
        Ok(())
    });
}

#[log_tags(Tag::GameLoad)]
fn load_energy_relays(ctx: &mut LoadContext) -> rusqlite::Result<()> {
    let mut stmt = ctx.conn.prepare("SELECT id FROM energy_relays")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let old_id: i64 = row.get(0)?;
        let grid_position = ctx.conn.get_grid_coords(old_id)?;
        let integrity_points = ctx.conn.get_integrity_points(old_id)?;
        let disabled_by_player = ctx.conn.get_disabled_by_player(old_id)?;

        #[warn_dev("EnergyRelay with old ID {old_id} has no corresponding new entity")]
        let Some(entity) = ctx.entity(old_id) else { continue };
        let mut builder = BuilderEnergyRelay::new(grid_position)
            .with_integrity_points(integrity_points);
        if disabled_by_player {
            builder = builder.with_disabled_by_player();
        }
        ctx.insert(entity, builder);
    }
    Ok(())
}

impl BuilderEnergyRelay {
    fn on_add_is_operational_insert_color_pulsation(
        trigger: On<Add, IsOperational>,
        mut commands: Commands,
    ) {
        commands.entity(trigger.entity).insert(ColorPulsation::new(1.0, 1.8, 3.0));
    }
    fn on_remove_is_operational_remove_color_pulsation(
        trigger: On<Remove, IsOperational>,
        mut commands: Commands,
    ) {
        commands.entity(trigger.entity).try_remove::<ColorPulsation>();
    }
}
