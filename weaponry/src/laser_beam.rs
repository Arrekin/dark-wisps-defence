use bevy::prelude::*;

use game_core::prelude::MapBound;

// ── Relationships ──────────────────────────────────────────────────────────────────────

#[derive(Component)]
#[relationship(relationship_target = GeneratedLaserBeam)]
pub struct LaserBeamGeneratedBy(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = LaserBeamGeneratedBy, linked_spawn)]
pub struct GeneratedLaserBeam(Entity);
impl GeneratedLaserBeam {
    pub fn beam(&self) -> Entity {
        self.0
    }
}

// ── System sets ────────────────────────────────────────────────────────────────────────

/// Deals the damage of locked beams. Lock beams on or switch them off, and update `muzzle` and
/// `direction`, before this set; read beams for visuals after it.
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct LaserBeamSystems;

// ── LaserBeam ──────────────────────────────────────────────────────────────────────────

/// Continuous beam of light locked onto a single target. While locked it deals its generator's
/// `AttackDamage` per second to that target only. Kept at world origin (`Transform::IDENTITY`) so
/// child visuals use world coordinates directly.
#[derive(Component)]
#[require(MapBound, Transform, Visibility)]
pub struct LaserBeam {
    target: Option<Entity>,
    /// Muzzle world position, updated by the generator every frame.
    pub muzzle: Vec2,
    /// Unit direction the turret faces, updated by the generator every frame.
    pub direction: Vec2,
}
impl Default for LaserBeam {
    fn default() -> Self {
        Self { target: None, muzzle: Vec2::ZERO, direction: Vec2::X }
    }
}
impl LaserBeam {
    pub fn lock_on(&mut self, target: Entity) {
        self.target = Some(target);
    }

    pub fn switch_off(&mut self) {
        self.target = None;
    }

    /// The entity the beam is locked onto, or `None` while off.
    pub fn target(&self) -> Option<Entity> {
        self.target
    }
}
