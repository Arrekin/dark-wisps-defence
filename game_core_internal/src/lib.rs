use bevy::{ecs::schedule::IntoScheduleConfigs, prelude::*, transform::TransformSystems};

use alteration::modifiers::prelude::IncomingDamageMultiplier;
use game_core::{motion::MotionSystems, prelude::*};

pub struct GameCorePlugin;
impl Plugin for GameCorePlugin {
    fn build(&self, app: &mut App) {
        app
            .add_observer(on_insert_zdepth_apply_zdepth)
            // Technical-state global observers: fold primitive component
            // events into one shared `TechnicalStateChanged` event. They fire
            // for every entity; the event is a no-op unless the target entity
            // has a local observer for `TechnicalStateChanged`.
            .add_observer(on_insert_is_powered_emit_technical_state_changed)
            .add_observer(on_remove_is_powered_emit_technical_state_changed)
            .add_observer(on_insert_disabled_by_player_emit_technical_state_changed)
            .add_observer(on_remove_disabled_by_player_emit_technical_state_changed)
            .add_systems(
                PostUpdate,
                track_locomotion
                    .in_set(MotionSystems::Track)
                    .after(TransformSystems::Propagate),
            )
            .add_message::<DamageMessage>()
            .add_systems(PostUpdate, (
                process_damage.run_if(on_message::<DamageMessage>),
            ));
    }
}

fn on_insert_zdepth_apply_zdepth(
    trigger: On<Insert, ZDepth>,
    mut transforms: Query<(&mut Transform, &ZDepth)>,
) {
    let entity = trigger.entity;
    let Ok((mut transform, z_depth)) = transforms.get_mut(entity) else { return; };
    transform.translation.z = z_depth.0;
}

fn track_locomotion(
    time: Res<Time>,
    mut movers: Query<(&GlobalTransform, &mut Locomotion)>,
) {
    let dt = time.delta_secs();
    for (global_transform, mut locomotion) in movers.iter_mut() {
        locomotion.advance(global_transform.translation().truncate(), dt);
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

fn on_insert_is_powered_emit_technical_state_changed(trigger: On<Insert, IsPowered>, mut commands: Commands) {
    commands.trigger(TechnicalStateChanged { entity: trigger.entity, kind: TechnicalChange::PowerGained });
}
fn on_remove_is_powered_emit_technical_state_changed(trigger: On<Remove, IsPowered>, mut commands: Commands) {
    commands.trigger(TechnicalStateChanged { entity: trigger.entity, kind: TechnicalChange::PowerLost });
}
fn on_insert_disabled_by_player_emit_technical_state_changed(trigger: On<Insert, DisabledByPlayer>, mut commands: Commands) {
    commands.trigger(TechnicalStateChanged { entity: trigger.entity, kind: TechnicalChange::PlayerDisabled });
}
fn on_remove_disabled_by_player_emit_technical_state_changed(trigger: On<Remove, DisabledByPlayer>, mut commands: Commands) {
    commands.trigger(TechnicalStateChanged { entity: trigger.entity, kind: TechnicalChange::PlayerEnabled });
}
