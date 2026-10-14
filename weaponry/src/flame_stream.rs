use std::ops::RangeInclusive;

use bevy::prelude::*;

use game_core::prelude::{GridCoords, MapBound};

// Stream shape
const CONE_HALF_ANGLE: f32 = 3.5_f32.to_radians();
const NOZZLE_HALF_WIDTH: f32 = 2.;

// Fuel
/// Extra distance beyond `length` that `tail` travels before burnout completes.
const BURNOUT_DISTANCE: f32 = 40.;

// ── Relationships ──────────────────────────────────────────────────────────────────────

#[derive(Component)]
#[relationship(relationship_target = GeneratedFlameStream)]
pub struct FlameStreamGeneratedBy(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = FlameStreamGeneratedBy, linked_spawn)]
pub struct GeneratedFlameStream(Entity);
impl GeneratedFlameStream {
    pub fn stream(&self) -> Entity {
        self.0
    }
}

// ── Events ─────────────────────────────────────────────────────────────────────────────

/// Triggered on the generator entity when a cut-off stream finishes burning out.
#[derive(EntityEvent, Clone, Copy)]
pub struct FlameStreamBurntOut {
    #[event_target]
    pub tower: Entity,
}

// ── System sets ────────────────────────────────────────────────────────────────────────

/// Advances stream fuel and computes `FlameStream::hit`. Update stream geometry before this set
/// and read hits after it.
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct FlameStreamSystems;

// ── FlameStream ────────────────────────────────────────────────────────────────────────

/// Continuous cone stream that stops at the first wisp hit. Kept at world origin
/// (`Transform::IDENTITY`) so child visuals use world coordinates directly.
#[derive(Component)]
#[require(MapBound, Transform, Visibility)]
pub struct FlameStream {
    state: FlameStreamState,
    /// Nozzle world position, updated before `FlameStreamSystems`.
    pub nozzle: Vec2,
    /// Unit aim direction, updated before `FlameStreamSystems`.
    pub direction: Vec2,
    /// Max reach from `nozzle` in world units.
    pub length: f32,
    /// Nearest wisp hit this frame, computed in `FlameStreamSystems`.
    pub hit: Option<FlameHit>,
}

#[derive(Clone, Copy)]
enum FlameStreamState {
    Unarmed,
    /// Armed with no fuel in flight.
    Armed,
    /// Actively spraying; `head` is the leading fuel distance from `nozzle`.
    Firing { head: f32 },
    /// Cut off while fuel is still in flight (`tail..=head`). Transitions to `Armed` (or `Unarmed`
    /// if `disarmed`) once `tail >= length + BURNOUT_DISTANCE`.
    Venting { head: f32, tail: f32, disarmed: bool },
}

/// First wisp intersection along a `FlameStream`.
pub struct FlameHit {
    /// Grid cell of the hit wisp.
    pub coords: GridCoords,
    /// Distance from `nozzle` along `direction`.
    pub distance: f32,
}

impl Default for FlameStream {
    fn default() -> Self {
        Self {
            state: FlameStreamState::Unarmed,
            nozzle: Vec2::ZERO,
            direction: Vec2::X,
            length: 0.,
            hit: None,
        }
    }
}

impl FlameStream {
    pub fn arm(&mut self) {
        self.state = match self.state {
            FlameStreamState::Unarmed => FlameStreamState::Armed,
            FlameStreamState::Venting { head, tail, .. } => FlameStreamState::Venting { head, tail, disarmed: false },
            state => state,
        };
    }

    /// Disarms the stream. If firing, cuts off fuel and transitions to `Unarmed` after burnout.
    pub fn disarm(&mut self) {
        self.state = match self.state {
            FlameStreamState::Armed => FlameStreamState::Unarmed,
            FlameStreamState::Firing { head } => FlameStreamState::Venting { head, tail: 0., disarmed: true },
            FlameStreamState::Venting { head, tail, .. } => FlameStreamState::Venting { head, tail, disarmed: true },
            FlameStreamState::Unarmed => FlameStreamState::Unarmed,
        };
    }

    pub fn ignite(&mut self) {
        if let FlameStreamState::Armed = self.state {
            self.state = FlameStreamState::Firing { head: 0. };
        }
    }

    pub fn cut_off(&mut self) {
        if let FlameStreamState::Firing { head } = self.state {
            self.state = FlameStreamState::Venting { head, tail: 0., disarmed: false };
        }
    }

    /// Advances fuel by `travel` world units. Returns `true` when a venting stream completes burnout.
    pub fn advance(&mut self, travel: f32) -> bool {
        let length = self.length;
        match self.state {
            FlameStreamState::Unarmed | FlameStreamState::Armed => false,
            FlameStreamState::Firing { head } => {
                self.state = FlameStreamState::Firing { head: (head + travel).min(length) };
                false
            }
            FlameStreamState::Venting { head, tail, disarmed } => {
                let tail = tail + travel;
                let burnt_out = tail >= length + BURNOUT_DISTANCE;
                self.state = match (burnt_out, disarmed) {
                    (false, _) => FlameStreamState::Venting { head: (head + travel).min(length), tail, disarmed },
                    (true, false) => FlameStreamState::Armed,
                    (true, true) => FlameStreamState::Unarmed,
                };
                burnt_out
            }
        }
    }

    /// Ready to ignite (`Armed` with no fuel in flight).
    pub fn is_armed(&self) -> bool {
        matches!(self.state, FlameStreamState::Armed)
    }

    pub fn is_firing(&self) -> bool {
        matches!(self.state, FlameStreamState::Firing { .. })
    }

    pub fn is_venting(&self) -> bool {
        matches!(self.state, FlameStreamState::Venting { .. })
    }

    /// Active fuel span (`tail..=head`) measured from `nozzle`, or `None` if no fuel is in flight.
    pub fn fuel(&self) -> Option<RangeInclusive<f32>> {
        match self.state {
            FlameStreamState::Unarmed | FlameStreamState::Armed => None,
            FlameStreamState::Firing { head } => Some(0.0..=head),
            FlameStreamState::Venting { head, tail, .. } => Some(tail..=head),
        }
    }

    pub fn half_width_at(&self, distance: f32) -> f32 {
        NOZZLE_HALF_WIDTH + distance * self.spread()
    }

    /// Lateral expansion per unit of axial distance (`tan(CONE_HALF_ANGLE)`).
    pub fn spread(&self) -> f32 {
        CONE_HALF_ANGLE.tan()
    }
}
