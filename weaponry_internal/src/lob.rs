//! # Lob Flight and Presentation
//!
//! Advances [`Lob`] entities along their [`LobArc`] and updates their visual children.
//!
//! The parent entity's [`Transform`] tracks the ground-plane projection (`LobSample::ground_position`)
//! and triggers [`LobLanded`] when [`LobArcProgress`] reaches `1.0`. Visual children are updated
//! relative to the parent each frame:
//! - [`LobBody`] scales up with altitude and shifts outward from the chord midpoint along the flight
//!   line to simulate top-down perspective, keeping the screen trajectory straight while foreshortening
//!   motion near launch and landing.
//! - [`DropShadow`] projects the shadow onto cell top surfaces using the landing position as its
//!   elevation reference so the body and shadow meet on impact.
//!
//! [`SaveLobExtension`] and [`LoadLobExtension`] persist [`LobArc`] and [`LobArcProgress`] in the
//! shared `lobs` table.

use bevy::prelude::*;

use game_core::prelude::{DrawLevel, ZDepth};
use persistence::{prelude::*, rusqlite};
use states::prelude::GameState;
use visuals::drop_shadow::DropShadow;
use weaponry::lob::{Lob, LobArc, LobArcProgress, LobLanded};

use crate::projectile_launch::ProjectileLaunchSystems;

pub(crate) struct LobPlugin;
impl Plugin for LobPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Update, (
                advance_lobs.after(ProjectileLaunchSystems).run_if(in_state(GameState::Running)),
                update_lob_visuals,
            ).chain())
            .add_observer(on_add_lob_place_on_arc);
    }
}

// Perspective, mirrored in assets/shaders/weaponry/lob_preview_body.wesl
/// Simulated camera height in world units. Lower values exaggerate projectile growth.
const CAMERA_HEIGHT: f32 = 320.;
/// Scales the perspective offset along the flight line, measured from the flight's midpoint.
/// Excessive stretch makes projectiles appear to reverse near launch and landing.
const PARALLAX_STRETCH: f32 = 0.5;
/// Visual height limit as a fraction of `CAMERA_HEIGHT`. Keeps perspective scaling finite.
const MAX_VISUAL_HEIGHT: f32 = 0.85;

/// Visible projectile body. Child of a lobbed projectile.
#[derive(Component)]
#[require(ZDepth::Projectile, DrawLevel::Airborne)]
pub(crate) struct LobBody;
impl LobBody {
    /// Perspective scale of a body `height` world units above the ground.
    pub(crate) fn scale_at(height: f32) -> f32 {
        CAMERA_HEIGHT / (CAMERA_HEIGHT - height.min(CAMERA_HEIGHT * MAX_VISUAL_HEIGHT))
    }
}

/// Places a new or loaded lob at its current progress before its first frame.
fn on_add_lob_place_on_arc(
    trigger: On<Add<Lob>>,
    mut lobs: Query<(&LobArc, &LobArcProgress, &mut Transform)>,
) {
    let Ok((arc, progress, mut transform)) = lobs.get_mut(trigger.entity) else { return; };
    transform.translation = arc.sample(progress.0).ground_position.extend(transform.translation.z);
}

pub(crate) fn advance_lobs(
    mut commands: Commands,
    time: Res<Time>,
    mut lobs: Query<(Entity, &LobArc, &mut LobArcProgress, &mut Transform), With<Lob>>,
) {
    for (entity, arc, mut progress, mut transform) in lobs.iter_mut() {
        if progress.is_at_target() { continue; }
        progress.advance(arc.progress_over(time.delta_secs()));
        transform.translation = arc.sample(progress.0).ground_position.extend(transform.translation.z);
        if progress.is_at_target() {
            commands.trigger(LobLanded(entity));
        }
    }
}

fn update_lob_visuals(
    lobs: Query<(&LobArc, &LobArcProgress, &Children), With<Lob>>,
    mut bodies: Query<&mut Transform, With<LobBody>>,
    mut shadows: Query<(&mut DropShadow, &mut Transform), Without<LobBody>>,
) {
    for (arc, progress, children) in lobs.iter() {
        let sample = arc.sample(progress.0);
        let body_scale = LobBody::scale_at(sample.height);
        let body_offset = (sample.ground_position - arc.origin.midpoint(arc.target)) * PARALLAX_STRETCH * (body_scale - 1.);
        for child in children.iter() {
            if let Ok(mut transform) = bodies.get_mut(child) {
                transform.translation = body_offset.extend(transform.translation.z);
                transform.scale = Vec3::splat(body_scale);
            } else if let Ok((mut shadow, mut transform)) = shadows.get_mut(child) {
                shadow.cast(sample.ground_position, sample.height);
                shadow.place(&mut transform);
            }
        }
    }
}

pub(crate) trait SaveLobExtension {
    /// Persists `arc` and `progress` into the `lobs` table for an already-registered `entity_id`.
    fn save_lob(&self, entity_id: u32, arc: LobArc, progress: LobArcProgress) -> rusqlite::Result<()>;
}
impl SaveLobExtension for SaveContext<'_> {
    fn save_lob(&self, entity_id: u32, arc: LobArc, progress: LobArcProgress) -> rusqlite::Result<()> {
        self.tx.prepare_cached("INSERT OR REPLACE INTO lobs (entity_id, origin_x, origin_y, target_x, target_y, duration, apex, progress) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)")?
            .execute(rusqlite::params![entity_id, arc.origin.x, arc.origin.y, arc.target.x, arc.target.y, arc.duration, arc.apex, progress.0])?;
        Ok(())
    }
}

pub(crate) trait LoadLobExtension {
    fn lob(&self, old_id: u32) -> LoadResult<(LobArc, LobArcProgress)>;
}
impl LoadLobExtension for LoadContext<'_> {
    fn lob(&self, old_id: u32) -> LoadResult<(LobArc, LobArcProgress)> {
        self.conn.prepare_cached("SELECT origin_x, origin_y, target_x, target_y, duration, apex, progress FROM lobs WHERE entity_id = ?1")
            .and_then(|mut statement| statement.query_row([old_id], |row| Ok((
                LobArc::new(
                    Vec2::new(row.get(0)?, row.get(1)?),
                    Vec2::new(row.get(2)?, row.get(3)?),
                    row.get(4)?,
                    row.get(5)?,
                ),
                LobArcProgress(row.get(6)?),
            ))))
            .map_err(LoadError::table_read("lobs"))
    }
}
