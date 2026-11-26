//! # Laser Beam
//!
//! Damage and light for `LaserBeam` entities. A locked beam deals its generator's `AttackDamage`
//! per second, every frame, to the target it is locked onto. The generator decides when the beam
//! locks on and switches off.
//!
//! Not persisted; recreated by the owning tower on load.
//!
//! ## Light
//!
//! `LaserBeamLight` is a lane visual on a child of the beam. Its shader animates the snap on
//! lock-on and the barrel heat from two game clock instants, stamped only when the beam locks on or
//! switches off. The beam goes dark the instant it switches off. While locked, the quad runs from
//! the muzzle to the target. While off, it keeps its last length and follows the turret, so the
//! cooling barrel glow stays on the muzzle.

use bevy::{prelude::*, reflect::TypePath, render::render_resource::{BlendState, ShaderType}};

use alteration::modifiers::prelude::AttackDamage;
use game_core::prelude::{DamageMessage, DrawLevel, Property, ZDepth};
use session::GameClock;
use states::prelude::GameState;
use visuals::{
    clock::ClockInstant,
    lane::{LanePlugin, LaneVisual},
    shader::{ADDITIVE_BLENDING, seed_from_entity},
};
use weaponry::laser_beam::{LaserBeam, LaserBeamGeneratedBy, LaserBeamSystems};

pub(crate) struct LaserBeamPlugin;
impl Plugin for LaserBeamPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins(LanePlugin::<LaserBeamLight>::default())
            .add_systems(Update, (
                damage_laser_beam_targets.in_set(LaserBeamSystems).run_if(in_state(GameState::Running)),
                update_laser_beam_lights.after(LaserBeamSystems),
            ))
            .add_observer(on_add_laser_beam_spawn_light);
    }
}

// Light
/// Quad overhang past each end of the beam, in world units.
const LIGHT_MESH_PAD: f32 = 48.;
/// Quad half-width across the beam, in world units.
const LIGHT_MESH_HALF_WIDTH: f32 = 56.;
/// Seconds a switched-off beam stays drawn. Covers the barrel cooling (`COOL`) in
/// `assets/shaders/weaponry/laser_beam.wesl`.
const AFTERGLOW_DURATION: f64 = 3.;

/// Beam, contact and barrel glow of a `LaserBeam`. Child of the beam. Field order and types mirror
/// `Instance` in `assets/shaders/weaponry/laser_beam.wesl`.
#[derive(Component, ShaderType, TypePath, Clone, Default, PartialEq)]
#[require(ZDepth::LaserBeam, DrawLevel::OnElevation, LaserBeamLightState)]
struct LaserBeamLight {
    /// Game clock instant the beam last locked on.
    engaged: ClockInstant,
    /// Game clock instant the beam last switched off; at or before `engaged` while it fires.
    released: ClockInstant,
    seed: f32,
    /// Muzzle to the contact, in world units.
    length: f32,
    pad: f32,
    half_width: f32,
}
impl LaneVisual for LaserBeamLight {
    const LABEL: &'static str = "laser_beam";
    const LOOK: &'static str = "shaders::weaponry::laser_beam";

    fn blend() -> BlendState {
        ADDITIVE_BLENDING
    }
}

/// The beam state `LaserBeamLight` last saw, to stamp its instants when it changes.
#[derive(Component, Default)]
struct LaserBeamLightState {
    locked: Option<Entity>,
    /// Game time the beam switched off, or `None` while locked or before it ever fired.
    released_at: Option<f64>,
}

fn on_add_laser_beam_spawn_light(
    trigger: On<Add<LaserBeam>>,
    mut commands: Commands,
) {
    commands.entity(trigger.entity).insert(children![(
        LaserBeamLight {
            seed: seed_from_entity(trigger.entity),
            pad: LIGHT_MESH_PAD,
            half_width: LIGHT_MESH_HALF_WIDTH,
            ..default()
        },
        Visibility::Hidden,
    )]);
}

fn damage_laser_beam_targets(
    mut damage_messages: MessageWriter<DamageMessage>,
    time: Res<Time>,
    beams: Query<(&LaserBeam, &LaserBeamGeneratedBy)>,
    generators: Query<&AttackDamage>,
) {
    for (beam, generated_by) in beams.iter() {
        let Some(target) = beam.target() else { continue; };
        let Ok(attack_damage) = generators.get(generated_by.0) else { continue; };
        damage_messages.write(DamageMessage { target, amount: attack_damage.get() * time.delta_secs() });
    }
}

fn update_laser_beam_lights(
    clock: Res<GameClock>,
    mut lights: Query<(&ChildOf, &mut LaserBeamLight, &mut LaserBeamLightState, &mut Transform, &mut Visibility)>,
    beams: Query<&LaserBeam>,
    targets: Query<&Transform, Without<LaserBeamLight>>,
) {
    let now = clock.now();
    for (child_of, mut light, mut state, mut transform, mut visibility) in lights.iter_mut() {
        let Ok(beam) = beams.get(child_of.parent()) else { continue; };
        let mut next = light.clone();

        let target = beam.target();
        if target != state.locked {
            if target.is_some() {
                next.engaged = ClockInstant::at(now);
                state.released_at = None;
            } else {
                next.released = ClockInstant::at(now);
                state.released_at = Some(now);
            }
            state.locked = target;
        }

        let shown = state.locked.is_some() || state.released_at.is_some_and(|released_at| now - released_at < AFTERGLOW_DURATION);
        visibility.set_if_neq(if shown { Visibility::Inherited } else { Visibility::Hidden });
        if shown {
            let contact = target.and_then(|target| targets.get(target).ok()).map(|target_transform| target_transform.translation.xy());
            let direction = match contact {
                Some(point) => {
                    let to_contact = point - beam.muzzle;
                    next.length = to_contact.length();
                    to_contact.normalize_or(beam.direction)
                }
                None => beam.direction,
            };
            transform.set_if_neq(Transform {
                translation: (beam.muzzle + direction * next.length * 0.5).extend(transform.translation.z),
                rotation: Quat::from_rotation_z(direction.to_angle()),
                scale: Vec3::new(next.length + 2. * LIGHT_MESH_PAD, 2. * LIGHT_MESH_HALF_WIDTH, 1.),
            });
        }
        light.set_if_neq(next);
    }
}
