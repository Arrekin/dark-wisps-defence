use bevy::{prelude::*, transform::TransformSystems};
use bytemuck::{Pod, Zeroable};

use alteration::modifiers::prelude::IncomingDamageMultiplier;
use game_core::{motion::{Locomotion, MotionSystems}, prelude::*, shared_gpu::{SharedGpuBinding, SharedGpuBuffers, SharedGpuWriter}};
use grids::{energy_supply::EnergySupplyGrid, prelude::{EnergySupplySystems, GridVersion}};

pub struct GameCorePlugin;
impl Plugin for GameCorePlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<SharedGpuBuffers>()
            .add_message::<DamageMessage>()
            .add_systems(PostUpdate, (
                enforce_zdepth_world_z.after(TransformSystems::Propagate),
                track_locomotion.in_set(MotionSystems::Track).after(TransformSystems::Propagate),
                process_damage.run_if(on_message::<DamageMessage>),
                update_power_state.after(EnergySupplySystems),
                upload_gpu_map_info.run_if(resource_changed::<MapInfo>),
            ))
            .add_observer(on_insert_zdepth_set_local_z)
            // Technical-state global observers: fold primitive component
            // events into one shared `TechnicalStateChanged` event. They fire
            // for every entity; the event is a no-op unless the target entity
            // has a local observer for `TechnicalStateChanged`.
            .add_observer(on_insert_is_powered_emit_technical_state_changed)
            .add_observer(on_remove_is_powered_emit_technical_state_changed)
            .add_observer(on_insert_disabled_by_player_emit_technical_state_changed)
            .add_observer(on_remove_disabled_by_player_emit_technical_state_changed)
            .add_observer(on_add_needs_power_init_power_state)
            .add_observer(on_moment_happened_propagate_to_watchers)
            .add_observer(on_add_display_icon_switcher_load_display_icon);
    }
}

/// The GPU side of [`MapInfo`]. Field order and types mirror `GpuMapInfo` in
/// `assets/shaders/common/map_info.wesl`.
#[derive(Clone, Copy, Pod, Zeroable)]
#[repr(C)]
struct GpuMapInfo {
    grid_bounds: UVec2,
}

fn upload_gpu_map_info(mut shared_gpu: SharedGpuWriter, map_info: Res<MapInfo>) {
    shared_gpu.write(SharedGpuBinding::MapInfo, [GpuMapInfo { grid_bounds: map_info.grid_bounds.into() }]);
}

// ============================================================================
// Moment-watching propagator
// ============================================================================

/// Catches `MomentHappened` on a moment entity, walks `MomentWatchers`, fires
/// `MomentHappened` on each watcher. Domain-agnostic — what the watcher does in
/// response is its own domain's business.
fn on_moment_happened_propagate_to_watchers(
    trigger: On<MomentHappened>,
    mut commands: Commands,
    watchers: Query<&MomentWatchers>,
) {
    let moment_entity = trigger.entity;
    let Ok(watcher_list) = watchers.get(moment_entity) else { return; };
    for watcher in watcher_list.iter() {
        commands.trigger(MomentHappened { entity: watcher });
    }
}

fn on_insert_zdepth_set_local_z(
    trigger: On<Insert<ZDepth>>,
    mut transforms: Query<(&mut Transform, &ZDepth)>,
) {
    let entity = trigger.entity;
    let Ok((mut transform, z_depth)) = transforms.get_mut(entity) else { return; };
    transform.translation.z = z_depth.world_z(entity);
}

fn on_add_display_icon_switcher_load_display_icon(
    trigger: On<Add<DisplayIconSwitcher>>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    switchers: Query<&DisplayIconSwitcher>,
) {
    let entity = trigger.entity;
    let Ok(switcher) = switchers.get(entity) else { return; };
    commands.entity(entity).insert(DisplayIcon(asset_server.load(&switcher.0)));
}

/// See `ZDepth` docs. Runs after propagation so it sees final world transforms;
/// the equality guard keeps unchanged entities from dirtying `GlobalTransform`
/// (and re-triggering render extraction) every frame.
fn enforce_zdepth_world_z(mut transforms: Query<(Entity, &mut GlobalTransform, &ZDepth)>) {
    for (entity, mut global_transform, z_depth) in transforms.iter_mut() {
        let world_z = z_depth.world_z(entity);
        let mut affine = global_transform.affine();
        if affine.translation.z != world_z {
            affine.translation.z = world_z;
            *global_transform = GlobalTransform::from(affine);
        }
    }
}

fn track_locomotion(
    time: Res<Time>,
    mut movers: Query<(&GlobalTransform, &mut Locomotion)>,
) {
    let delta_seconds = time.delta_secs();
    for (global_transform, mut locomotion) in movers.iter_mut() {
        locomotion.advance(global_transform.translation().truncate(), delta_seconds);
    }
}

fn process_damage(
    mut reader: MessageReader<DamageMessage>,
    mut targets: Query<(&mut IntegrityPoints, Option<&IncomingDamageMultiplier>)>,
) {
    for message in reader.read() {
        if message.amount <= 0.0 { continue; }
        let Ok((mut integrity_points, incoming_damage_multiplier)) = targets.get_mut(message.target) else { continue; };
        let incoming_damage_multiplier = incoming_damage_multiplier.map_or(1.0, |multiplier| multiplier.get());
        integrity_points.decrease(message.amount * incoming_damage_multiplier);
    }
}

// ============================================================================
// Technical State — global observers
// ============================================================================

fn on_insert_is_powered_emit_technical_state_changed(trigger: On<Insert<IsPowered>>, mut commands: Commands) {
    commands.trigger(TechnicalStateChanged { entity: trigger.entity, kind: TechnicalChange::PowerGained });
}
fn on_remove_is_powered_emit_technical_state_changed(trigger: On<Remove<IsPowered>>, mut commands: Commands) {
    commands.trigger(TechnicalStateChanged { entity: trigger.entity, kind: TechnicalChange::PowerLost });
}
fn on_insert_disabled_by_player_emit_technical_state_changed(trigger: On<Insert<DisabledByPlayer>>, mut commands: Commands) {
    commands.trigger(TechnicalStateChanged { entity: trigger.entity, kind: TechnicalChange::PlayerDisabled });
}
fn on_remove_disabled_by_player_emit_technical_state_changed(trigger: On<Remove<DisabledByPlayer>>, mut commands: Commands) {
    commands.trigger(TechnicalStateChanged { entity: trigger.entity, kind: TechnicalChange::PlayerEnabled });
}

// ============================================================================
// NeedsPower — power state management
// ============================================================================

fn on_add_needs_power_init_power_state(
    trigger: On<Add<NeedsPower>>,
    mut commands: Commands,
    energy_supply_grid: Res<EnergySupplyGrid>,
    consumers: Query<(&GridCoords, &GridImprint), With<NeedsPower>>,
) {
    let entity = trigger.entity;
    let Ok((grid_coords, grid_imprint)) = consumers.get(entity) else { return; };

    let has_power = energy_supply_grid.is_imprint_powered(*grid_coords, *grid_imprint);
    if has_power {
        commands.entity(entity).insert(IsPowered);
    }
    commands.entity(entity).observe(on_insert_needs_power_coords_refresh_power_state);
}

/// Refreshes the power state when the consumer's coords or imprint change.
fn on_insert_needs_power_coords_refresh_power_state(
    trigger: On<Insert<(GridCoords, GridImprint)>>,
    mut commands: Commands,
    energy_supply_grid: Res<EnergySupplyGrid>,
    consumers: Query<(&GridCoords, &GridImprint, Has<IsPowered>), With<NeedsPower>>,
) {
    let entity = trigger.entity;
    let Ok((grid_coords, grid_imprint, has_is_powered)) = consumers.get(entity) else { return; };

    let has_power = energy_supply_grid.is_imprint_powered(*grid_coords, *grid_imprint);
    if has_power != has_is_powered {
        if has_power {
            commands.entity(entity).insert(IsPowered);
        } else {
            commands.entity(entity).remove::<IsPowered>();
        }
    }
}

/// Refreshes every consumer's power state when the energy supply grid's version changes.
fn update_power_state(
    mut commands: Commands,
    energy_supply_grid: Res<EnergySupplyGrid>,
    power_entities: Query<(Entity, &GridCoords, &GridImprint, Has<IsPowered>), With<NeedsPower>>,
    mut current_energy_supply_grid_version: Local<GridVersion>,
) {
    if *current_energy_supply_grid_version == energy_supply_grid.version { return; }
    *current_energy_supply_grid_version = energy_supply_grid.version;

    for (entity, grid_coords, grid_imprint, has_is_powered) in power_entities.iter() {
        let has_power = energy_supply_grid.is_imprint_powered(*grid_coords, *grid_imprint);
        if has_power != has_is_powered {
            if has_power {
                commands.entity(entity).insert(IsPowered);
            } else {
                commands.entity(entity).remove::<IsPowered>();
            }
        }
    }
}
