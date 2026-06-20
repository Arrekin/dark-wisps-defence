use crate::lib_prelude::*;
use crate::grids::base::BaseGrid;
use crate::grids::obstacles::ObstacleGrid;
use crate::search::flooding::{flood_emissions, FloodEmissionsDetails};


pub struct EmissionsPlugin;
impl Plugin for EmissionsPlugin {
    fn build(&self, app: &mut App) {
        app
            .insert_resource(EmissionsGrid::new_empty())
            .init_resource::<EmissionsEnergyRecalculateAll>()
            .add_message::<EmitterChangedEvent>()
            .add_systems(OnExit(MapLoadingStage::LoadMapInfo), |mut commands: Commands, map_info: Res<MapInfo>| { commands.insert_resource(EmissionsGrid::new_with_size(map_info.grid_width, map_info.grid_height)); })
            .add_systems(PostUpdate, (
                emissions_calculations_system,
            ))
            .add_observer(EmitterEnergy::on_add)
            .add_observer(EmitterEnergy::on_remove)
            .add_observer(EmitterEnergy::on_spread_affector_insert)
            .add_observer(EmitterEnergy::on_spread_affector_remove)
            ;
    }
}

/// Companion component to EmitterEnergy. Use it to mark wheter Emitter is functional.
#[derive(Component, Default)]
pub struct EmitterEnergyEnabled;
#[derive(Component)]
pub struct EmitterEnergy(pub FloodEmissionsDetails);
impl EmitterEnergy {
    fn on_add(
        trigger: On<Add, EmitterEnergy>,
        mut commands: Commands,
    ) {
        let entity = trigger.entity;
        commands.entity(entity)
            .observe(Self::on_enable_or_insert)
            .observe(Self::on_disable_or_replace)
            .insert(EmitterEnergyEnabled);
    }
    fn on_remove(
        trigger: On<Remove, EmitterEnergy>,
        mut commands: Commands,
    ) {
        let observer = trigger.observer();
        commands.entity(observer).despawn();
    }
    fn on_enable_or_insert(
        trigger: On<Insert, (GridCoords, GridImprint, EmitterEnergyEnabled)>,
        mut events: MessageWriter<EmitterChangedEvent>,
        suppliers: Query<(&GridCoords, &GridImprint, &EmitterEnergy)>,
    ) {
        let entity = trigger.entity;
        let Ok((grid_coords, grid_imprint, emitter)) = suppliers.get(entity) else { return; };
        events.write(EmitterChangedEvent {
            emitter_entity: entity,
            imprint: *grid_imprint,
            grid_coords: *grid_coords,
            emissions_details: vec![emitter.0.clone()],
        });
    }
    fn on_disable_or_replace(
        trigger: On<Discard, (GridCoords, GridImprint, EmitterEnergyEnabled)>,
        mut events: MessageWriter<EmitterChangedEvent>,
        suppliers: Query<(&GridCoords, &GridImprint, &EmitterEnergy), With<EmitterEnergyEnabled>>,
    ) {
        let entity = trigger.entity;
        let Ok((grid_coords, grid_imprint, emitter)) = suppliers.get(entity) else { return; };
        events.write(EmitterChangedEvent {
            emitter_entity: entity,
            imprint: *grid_imprint,
            grid_coords: *grid_coords,
            emissions_details: vec![emitter.0.cloned_with_reversed_mode()],
        });
    }
    fn on_spread_affector_insert(
        _trigger: On<Insert, EmissionsGridSpreadAffector>,
        mut recalculate_all: ResMut<EmissionsEnergyRecalculateAll>,
    ) {
        recalculate_all.0 = true;
    }
    fn on_spread_affector_remove(
        _trigger: On<Remove, EmissionsGridSpreadAffector>,
        mut recalculate_all: ResMut<EmissionsEnergyRecalculateAll>,
    ) {
        recalculate_all.0 = true;
    }
}

#[derive(Message, Debug)]
pub struct EmitterChangedEvent {
    pub emitter_entity: Entity,
    pub imprint: GridImprint,
    pub grid_coords: GridCoords,
    pub emissions_details: Vec<FloodEmissionsDetails>,
}

#[derive(Resource, Default)]
pub struct EmissionsEnergyRecalculateAll(pub bool);

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum EmissionsType {
    Energy,
}

#[derive(Clone, Default, Debug)]
pub struct Emissions {
    pub energy: f32,
}
#[derive(Default)]
pub struct EmissionsGridVersion {
    pub energy: GridVersion,
}

pub type EmissionsGrid = BaseGrid<Emissions, EmissionsGridVersion>;

impl EmissionsGrid {
    pub fn add_energy(&mut self, coords: GridCoords, energy: f32) {
        self[coords].energy += energy;
        if self[coords].energy.abs() < 0.0001 { self[coords].energy = 0.; }
        self.version.energy = self.version.energy.wrapping_add(1);
    }
    pub fn reset_energy_emissions(&mut self) {
        self.grid.iter_mut().for_each(|emissions| {
            emissions.energy = 0.;
        });
        self.version.energy = self.version.energy.wrapping_add(1);
    }
}

fn emissions_calculations_system(
    mut recalculate_all: ResMut<EmissionsEnergyRecalculateAll>,
    mut events: MessageReader<EmitterChangedEvent>,
    mut emissions_grid: ResMut<EmissionsGrid>,
    obstacle_grid: Res<ObstacleGrid>,
    emitters_buildings: Query<(&EmitterEnergy, &GridImprint, &GridCoords), With<EmitterEnergyEnabled>>,
) {
    if recalculate_all.0 {
        recalculate_all.0 = false;
        emissions_grid.reset_energy_emissions();
        for (emitter, grid_imprint, coords) in emitters_buildings.iter() {
            flood_emissions(
                &mut emissions_grid,
                &obstacle_grid,
                grid_imprint.iter(*coords),
                &vec![emitter.0.clone()],
                |field| !field.has_wall(),
            );
        }
        events.clear();
    } else {
        for event in events.read() {
            flood_emissions(
                &mut emissions_grid,
                &obstacle_grid,
                event.imprint.iter(event.grid_coords),
                &event.emissions_details,
                |field| !field.has_wall(),
            );
        }
    }
}