//! # Field
//!
//! Generic field systems, shared by every field type.
//!
//! Fields resize at a fixed speed toward the target radius set by their generator. When the
//! generator despawns, its field shrinks to zero. The field survives one more update so its
//! members can exit before it despawns.

use bevy::prelude::*;
use bytemuck::{Pod, Zeroable};

use fields::{
    field::{FieldEntered, FieldExited, FieldGeneratedBy},
    prelude::*,
};
use game_core::prelude::MapInfo;
use grids::GridVersion;
use states::prelude::{GameState, MapLoadingStage};
use visuals::shared_gpu::{SharedGpuBinding, SharedGpuWriter};

pub(crate) struct FieldPlugin;
impl Plugin for FieldPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<FieldIndex>()
            .add_systems(OnExit(MapLoadingStage::LoadMapInfo), |mut commands: Commands, map_info: Res<MapInfo>| { commands.insert_resource(FieldIndex::new_with_size(map_info.grid_bounds)); })
            .add_systems(Update, (
                resize_fields.run_if(in_state(GameState::Running)),
                sync_field_index,
                track_field_members.run_if(in_state(GameState::Running)),
            ).chain())
            .add_systems(PostUpdate, upload_gpu_field_index.run_if(resource_changed::<FieldIndex>))
            .add_observer(on_add_field_join_index)
            .add_observer(on_remove_field_slot_leave_index)
            .add_observer(on_remove_field_generated_by_fold_field);
    }
}

// Resize
/// Radius change per second, in world pixels.
const RESIZE_SPEED: f32 = 180.;

// ── Index ──────────────────────────────────────────────────────────────────────────────

fn on_add_field_join_index(
    trigger: On<Add<Field>>,
    mut commands: Commands,
    mut index: ResMut<FieldIndex>,
    fields: Query<(&Field, &Transform)>,
) {
    let Ok((field, transform)) = fields.get(trigger.entity) else { return; };
    let slot = index.insert(trigger.entity, transform.translation.xy(), field);
    commands.entity(trigger.entity).insert(slot);
}

/// Rewrites the slots of fields that changed or moved.
fn sync_field_index(
    mut index: ResMut<FieldIndex>,
    changed_fields: Query<(&Field, &Transform, &FieldSlot), Or<(Changed<Field>, Changed<Transform>)>>,
) {
    for (field, transform, slot) in changed_fields.iter() {
        index.update(*slot, transform.translation.xy(), field);
    }
}

fn on_remove_field_slot_leave_index(
    trigger: On<Remove<FieldSlot>>,
    mut index: ResMut<FieldIndex>,
    slots: Query<&FieldSlot>,
) {
    let Ok(slot) = slots.get(trigger.entity) else { return; };
    index.remove(*slot, trigger.entity);
}

// ── GPU ────────────────────────────────────────────────────────────────────────────────

/// One field table row. Field order and types mirror `FieldShape` in
/// `assets/shaders/fields/fields.wesl`.
#[derive(Clone, Copy, Default, Pod, Zeroable)]
#[repr(C)]
struct GpuFieldShape {
    center: Vec2,
    /// 0 for a free slot.
    current_radius: f32,
    strength: f32,
    field_type: u32,
    _padding: u32,
}

/// Uploads the field table and any changed cell lists together to keep the GPU index consistent.
fn upload_gpu_field_index(
    mut shared_gpu: SharedGpuWriter,
    index: Res<FieldIndex>,
    mut uploaded_cells_version: Local<GridVersion>,
) {
    shared_gpu.write_with_capacity(SharedGpuBinding::FieldShapes, index.slots().iter().map(|slot| match slot {
        Some(field) => GpuFieldShape {
            center: field.center,
            current_radius: field.current_radius,
            strength: field.strength,
            field_type: field.field_type as u32,
            _padding: 0,
        },
        None => GpuFieldShape::default(),
    }));

    // A replacement index may reuse the previous version number; upload it regardless.
    let cells = index.cells();
    if !index.is_added() && cells.version == *uploaded_cells_version { return; }
    *uploaded_cells_version = cells.version;
    let mut offsets = Vec::with_capacity(cells.grid.len() + 1);
    let mut entries = Vec::new();
    for cell in cells.grid.iter() {
        offsets.push(entries.len() as u32);
        entries.extend(cell.iter().map(|&slot| slot as u32));
    }
    offsets.push(entries.len() as u32);
    shared_gpu.write(SharedGpuBinding::FieldCellOffsets, offsets);
    shared_gpu.write_with_capacity(SharedGpuBinding::FieldCellEntries, entries);
}

// ── Field tracking ─────────────────────────────────────────────────────────────────────

fn track_field_members(
    mut commands: Commands,
    index: Res<FieldIndex>,
    mut targets: Query<(Entity, &Transform, &mut FieldAffectable)>,
) {
    for (entity, transform, mut affected) in targets.iter_mut() {
        let new_field = index.owner_at(transform.translation.xy());
        if new_field == affected.current_field { continue; }

        if let Some(old_field) = affected.current_field {
            commands.trigger(FieldExited { field: old_field, target: entity });
        }
        if let Some(new_field) = new_field {
            commands.trigger(FieldEntered { field: new_field, target: entity, exited_field: affected.current_field });
        }
        affected.current_field = new_field;
    }
}

// ── Resize and lifetime ────────────────────────────────────────────────────────────────

/// Starts shrinking the field when it loses its generator, including when the generator despawns.
fn on_remove_field_generated_by_fold_field(
    trigger: On<Remove<FieldGeneratedBy>>,
    mut fields: Query<&mut Field>,
) {
    let Ok(mut field) = fields.get_mut(trigger.entity) else { return; };
    field.target_radius = 0.;
}

/// Moves each field's current radius toward its target. Despawns orphans that reached 0.
pub(crate) fn resize_fields(
    mut commands: Commands,
    time: Res<Time>,
    mut fields: Query<(Entity, &mut Field, Has<FieldGeneratedBy>)>,
) {
    for (entity, mut field, has_owner) in fields.iter_mut() {
        // An orphan despawns one frame after reaching 0, so its members exit while it still exists.
        if !has_owner && field.current_radius <= 0. {
            commands.entity(entity).despawn();
            continue;
        }
        // Avoid marking settled fields as changed and triggering unnecessary index updates.
        if field.current_radius != field.target_radius {
            field.step_toward_target(RESIZE_SPEED * time.delta_secs());
        }
    }
}
