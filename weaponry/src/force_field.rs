use bevy::prelude::*;
use game_core::prelude::MapBound;

// ── Relationships ──────────────────────────────────────────────────────────────────────

#[derive(Component)]
#[relationship(relationship_target = GeneratedForceField)]
pub struct ForceFieldGeneratedBy(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = ForceFieldGeneratedBy)]
pub struct GeneratedForceField(Entity);

// ── ForceFieldState ────────────────────────────────────────────────────────────────────

#[derive(Component)]
#[component(immutable)]
pub enum ForceFieldState {
    Growing,
    Shrinking,
}

// ── ForceField Events ────────────────────────────────────────────────────────────────────

/// Triggered on a `ForceField` entity when a tracked entity enters its Voronoi cell.
/// `exited_field` is set when the entity crossed directly from another field (seam crossing),
/// and `None` when entering from outside all fields.
#[derive(EntityEvent, Clone, Copy)]
pub struct ForceFieldEntered {
    #[event_target]
    pub field: Entity,
    pub target: Entity,
    pub exited_field: Option<Entity>,
}

/// Triggered on a `ForceField` entity when a tracked entity leaves its Voronoi cell.
#[derive(EntityEvent, Clone, Copy)]
pub struct ForceFieldExited {
    #[event_target]
    pub field: Entity,
    pub target: Entity,
}

// ── ForceField ─────────────────────────────────────────────────────────────────────────

#[derive(Component)]
#[require(MapBound)]
pub struct ForceField {
    /// Field radius in world units.
    pub radius: f32,
    /// Animated progress 0.0 (gone) → 1.0 (full). Drives both visual size and Voronoi weight.
    pub progress: f32,
    /// Random per-field offset added to global time in the noise function.
    pub visual_noise_offset: f32,
}
impl ForceField {
    pub fn new(radius: f32) -> Self {
        let mut rng = nanorand::tls_rng();
        Self {
            radius,
            progress: 0.0,
            visual_noise_offset: nanorand::Rng::generate::<f32>(&mut rng) * 100.0,
        }
    }
}

// ── BuilderForceField ──────────────────────────────────────────────────────────────────

#[derive(Component)]
pub struct BuilderForceField {
    pub radius: f32,
    pub tower_entity: Entity,
    pub world_position: Vec3,
}
impl BuilderForceField {
    pub fn new(radius: f32, tower_entity: Entity, world_position: Vec3) -> Self {
        Self { radius, tower_entity, world_position }
    }
}
