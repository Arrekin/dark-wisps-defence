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
    energy_supply::{GeneratorEnergy, SupplierEnergy},
    placement::{annotate_non_empty, PlacementChannel},
};
use logging::prelude::*;
use persistence::{
    creating_new_map,
    prelude::{AppGameLoadSaveExtension, CollectSave, GameDbHelpers, LoadContext, SaveWriter},
    rusqlite,
};
use states::prelude::*;
use viewport::MainCamera;

use crate::common::*;


pub struct MainBasePlugin;
impl Plugin for MainBasePlugin {
    fn build(&self, app: &mut App) {
        let almanach_info = BuilderMainBase::almanach_info(app.world().resource::<AssetServer>());
        app
            .add_observer(BuilderMainBase::on_builder_add_spawn_main_base)
            .add_systems(CollectSave, collect_main_bases)
            .register_loader(MapLoadingStage::SpawnMapElements, "main_bases", load_main_bases)
            .add_systems(OnEnter(MapLoadingStage::SpawnMapElements), seed_main_base.run_if(creating_new_map))
            .add_systems(OnEnter(MapLoadingStage::Ready), center_camera_on_main_base)
            .register_building(BuildingType::MainBase, almanach_info)
            ;
    }
}

/// Spawn one `BuilderMainBase` at map center on a new map.
fn seed_main_base(mut commands: Commands, map_info: Res<MapInfo>) {
    let center = GridCoords {
        x: map_info.grid_width / 2,
        y: map_info.grid_height / 2,
    };
    commands.spawn(BuilderMainBase::new(center));
}

/// Center the main camera on the `MainBase` at the end of every map build,
/// falling back to map center when there is no main base.
fn center_camera_on_main_base(
    map_info: Res<MapInfo>,
    main_base: Option<Single<&GlobalTransform, With<MainBase>>>,
    camera: Single<&mut Transform, With<MainCamera>>,
) {
    let (x, y) = match main_base {
        Some(base) => {
            let translation = base.into_inner().translation();
            (translation.x, translation.y)
        }
        None => (map_info.world_width / 2.0, map_info.world_height / 2.0),
    };
    let translation = &mut camera.into_inner().translation;
    translation.x = x;
    translation.y = y;
}



#[derive(Component, SSS)]
pub(crate) struct BuilderMainBase {
    pub grid_position: GridCoords,
    /// Saved integrity points. `None` ⇒ defer to baseline (fresh spawn);
    /// `Some` ⇒ override with saved value (restore).
    pub integrity_points: Option<f32>,
}
impl BuilderMainBase {
    pub fn almanach_info(asset_server: &AssetServer) -> BuildingInfo {
        BuildingInfo {
            name: "Main Base".to_string(),
            sprite: asset_server.load("buildings/main_base.png"),
            top_sprite: None,
            grid_imprint: GridImprint::Rectangle { width: 6, height: 6 },
            cost: vec![],
            baseline: HashMap::from([
                (ModifierType::MaxIntegrityPoints, 10000.),
                (ModifierType::EnergySupplyRange, 15.),
            ]),
            validate: building_validator,
            annotate: annotate_non_empty,
            placement: PlacementChannel::of::<Building>(),
        }
    }

    pub fn new(grid_position: GridCoords) -> Self {
        Self { grid_position, integrity_points: None }
    }
    pub fn with_integrity_points(mut self, integrity_points: f32) -> Self {
        self.integrity_points = Some(integrity_points);
        self
    }

    pub fn on_builder_add_spawn_main_base(
        trigger: On<Add, BuilderMainBase>,
        mut commands: Commands,
        almanach: Res<Almanach>,
        builders: Query<&BuilderMainBase>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return; };

        let building_info = almanach.get_building_info(BuildingType::MainBase);
        let grid_imprint = building_info.grid_imprint;
        
        let mut entity_commands = commands.entity(entity);
        if let Some(ip) = builder.integrity_points {
            entity_commands.insert(IntegrityPoints::new(ip));
        }
        // Common
        entity_commands
            .remove::<BuilderMainBase>()
            .insert((
                Sprite {
                    image: building_info.sprite.clone(),
                    custom_size: Some(grid_imprint.world_size()),
                    ..Default::default()
                },
                MainBase,
                builder.grid_position,
                grid_imprint,
                EmitterEnergy(FloodEmissionsDetails {
                    emissions_type: EmissionsType::Energy,
                    range: usize::MAX,
                    evaluator: FloodEmissionsEvaluator::ExponentialDecay { start_value: 100., decay: 0.1 },
                    mode: FloodEmissionsMode::Increase,
                }),
                GeneratorEnergy,
                SupplierEnergy,
                related![EffectInstances[
                    (ModifierContributions(building_info.baseline.clone()), BaselineEffect),
                ]],
            ))
            .observe(on_technical_state_changed_recompute_operational)
            ;
        commands.trigger(TechnicalStateChanged { entity, kind: TechnicalChange::JustSpawned });
    }
}

fn collect_main_bases(
    main_base: Query<(Entity, &GridCoords, &IntegrityPoints), With<MainBase>>,
    mut save: SaveWriter,
) {
    if let Ok((entity, coords, integrity_points)) = main_base.single() {
        let id = entity.index_u32() as i64;
        let gx = coords.x;
        let gy = coords.y;
        let ip = integrity_points.get_current();
        save.submit(move |tx| {
            tx.save_marker("main_bases", id)?;
            tx.save_grid_coords(id, GridCoords { x: gx, y: gy })?;
            tx.save_integrity_points(id, ip)?;
            Ok(())
        });
    }
}

fn load_main_bases(ctx: &mut LoadContext) -> rusqlite::Result<()> {
    let mut stmt = ctx.conn.prepare("SELECT id FROM main_bases")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let old_id: i64 = row.get(0)?;
        let grid_position = ctx.conn.get_grid_coords(old_id)?;
        let integrity_points = ctx.conn.get_integrity_points(old_id)?;

        let Some(entity) = ctx.entity(old_id) else {
            Log::warn().dev().tag(Tag::GameLoad).message(format!("MainBase with old ID {old_id} has no corresponding new entity"));
            continue;
        };
        let builder = BuilderMainBase::new(grid_position)
            .with_integrity_points(integrity_points);
        ctx.insert(entity, builder);
    }
    Ok(())
}