use bevy::prelude::*;
use nanorand::Rng;
use strum::{AsRefStr, EnumIter, EnumString};

use game_core::prelude::{Bounds, FromEntity, GridCoords, MapBound, Moment, MomentKind, SSS, WispType};
use grids::prelude::ObstacleGrid;

#[derive(Component, Clone, Debug)]
#[require(MapBound, SummoningRuntime)]
pub struct Summoning {
    pub id_name: String,
    pub wisp_types: Vec<WispType>,
    pub area: SpawnArea,
    pub tempo: SpawnTempo,
    pub limit_count: Option<i32>,
}

// ============================================================================
// State machine
//
// `SummoningState` is the single source of truth — an immutable enum component.
// The derived marker components (`SummoningInactive`, `SummoningActive`,
// `SummoningExhausted`) are never inserted directly; a sync observer swaps
// them on every `Insert<SummoningState>`. Systems query the markers (e.g.
// `With<SummoningActive>`) rather than matching on the enum value.
// ============================================================================

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Default, EnumString, AsRefStr)]
#[component(immutable)]
pub enum SummoningState {
    #[default]
    Inactive,
    Active,
    Exhausted,
}

#[derive(Component, Default)]
pub struct SummoningInactive;
#[derive(Component, Default)]
pub struct SummoningActive;
#[derive(Component, Default)]
pub struct SummoningExhausted;
impl Default for Summoning {
    fn default() -> Self {
        Self {
            id_name: "new_summoning".to_string(),
            wisp_types: vec![WispType::Fire],
            area: SpawnArea::default(),
            tempo: SpawnTempo::default(),
            limit_count: None,
        }
    }
}
impl Summoning {
    pub fn get_random_wisp_type(&self, rng: &mut nanorand::tls::TlsWyRand) -> WispType {
        self.wisp_types[rng.generate_range(0..self.wisp_types.len())]
    }
}

#[derive(Clone, Debug, Default, PartialEq, EnumIter, AsRefStr)]
pub enum SpawnArea {
    Coords { coords: Vec<GridCoords> },
    Rect { origin: GridCoords, width: i32, height: i32 },
    Edge { side: EdgeSide },
    #[default]
    EdgesAll,
}

impl SpawnArea {
    pub fn get_random_coord(
        &self,
        obstacle_grid: &ObstacleGrid,
        rng: &mut nanorand::tls::TlsWyRand,
    ) -> GridCoords {
        match self {
            SpawnArea::Coords { coords } => {
                let idx = rng.generate_range(0..coords.len());
                coords[idx]
            }
            SpawnArea::Rect { origin, width, height } => {
                let x = origin.x + rng.generate_range(0..*width);
                let y = origin.y + rng.generate_range(0..*height);
                GridCoords { x, y }
            }
            SpawnArea::Edge { side } => {
                let Bounds { width, height } = obstacle_grid.bounds;
                match side {
                    EdgeSide::Top => GridCoords { x: rng.generate_range(0..width), y: height - 1 },
                    EdgeSide::Bottom => GridCoords { x: rng.generate_range(0..width), y: 0 },
                    EdgeSide::Left => GridCoords { x: 0, y: rng.generate_range(0..height) },
                    EdgeSide::Right => GridCoords { x: width - 1, y: rng.generate_range(0..height) },
                }
            }
            SpawnArea::EdgesAll => {
                let Bounds { width, height } = obstacle_grid.bounds;
                // Perimeter cells, indexed by walking the border in one pass:
                // bottom row, then right column, then top row, then left column.
                // Corners belong to the rows, so each column contributes
                // `height - 2` cells and the total is `2 * (width + height - 2)`.
                let edge_count = 2 * (width + height - 2);
                let edge_idx = rng.generate_range(0..edge_count);

                if edge_idx < width {
                    GridCoords { x: edge_idx, y: 0 }
                } else if edge_idx < width + height - 2 {
                    GridCoords { x: width - 1, y: edge_idx - width + 1 }
                } else if edge_idx < 2 * width + height - 2 {
                    GridCoords { x: width - 1 - (edge_idx - width - height + 2), y: height - 1 }
                } else {
                    GridCoords { x: 0, y: height - 2 - (edge_idx - 2 * width - height + 2) }
                }
            }
        }
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, EnumIter, EnumString, AsRefStr)]
pub enum EdgeSide {
    #[default]
    Top,
    Bottom,
    Left,
    Right,
}

#[derive(Copy, Clone, Debug, PartialEq, AsRefStr)]
pub enum SpawnTempo {
    /// Spawn `count` wisps every `seconds` (optional jitter). If `count` omitted -> 1.
    Continuous { seconds: f32, jitter: f32, bulk_count: i32 },
}

impl Default for SpawnTempo {
    fn default() -> Self {
        Self::Continuous { seconds: 1.0, jitter: 0.0, bulk_count: 1 }
    }
}

// --------------- SUMMONING RUNTIME ---------------

#[derive(Component, Copy, Clone, Default)]
pub struct SummoningRuntime {
    pub produced: i32,
    pub next_spawn_time: f32,
}

// --------------- ACTIVATION EVENT ---------------

/// A summoning has entered the `Active` state.
#[derive(Debug, Clone, EntityEvent, FromEntity)]
pub struct SummoningActivatedEvent {
    pub entity: Entity,
}

/// A summoning has entered the `Exhausted` state.
#[derive(Debug, Clone, EntityEvent, FromEntity)]
pub struct SummoningExhaustedEvent {
    pub entity: Entity,
}

// ============================================================================
// Summoning moment kinds
//
// Each marker is a moment kind owned by the summoning domain. The `MomentKind`
// derive infers the persistence key from the type name.
// ============================================================================

/// The parent summoning has started (transitioned to `Active`).
#[derive(Component, Default, MomentKind)]
#[require(Moment, Name = Name::new("Summoning Started"))]
pub struct MomentSummoningStarted;

/// The parent summoning has exhausted (reached its production limit).
#[derive(Component, Default, MomentKind)]
#[require(Moment, Name = Name::new("Summoning Exhausted"))]
pub struct MomentSummoningExhausted;

// --------------- BUILDER PATTERN ---------------

#[derive(Component, SSS)]
pub struct BuilderSummoning {
    pub summoning: Summoning,
    pub activated_by: Option<Entity>,
    pub state: SummoningState,
    pub runtime: SummoningRuntime,
}

impl BuilderSummoning {
    pub fn new(summoning: Summoning) -> Self {
        Self {
            summoning,
            activated_by: None,
            state: SummoningState::default(),
            runtime: SummoningRuntime::default(),
        }
    }

    pub fn with_activated_by(mut self, entity: Entity) -> Self {
        self.activated_by = Some(entity);
        self
    }

    pub fn with_state(mut self, state: SummoningState) -> Self {
        self.state = state;
        self
    }

    pub fn with_runtime(mut self, runtime: SummoningRuntime) -> Self {
        self.runtime = runtime;
        self
    }
}
