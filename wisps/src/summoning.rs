use bevy::prelude::*;
use nanorand::Rng;
use serde::{Deserialize, Serialize};
use strum::{AsRefStr, EnumIter};

use game_core::prelude::{GridCoords, MapBound, SSS, WispType};
use grids::prelude::ObstacleGrid;

#[derive(Component, Clone, Debug, Serialize, Deserialize)]
#[require(MapBound, SummoningRuntime)]
pub struct Summoning {
    pub id_name: String,
    pub wisp_types: Vec<WispType>,
    pub area: SpawnArea,
    pub tempo: SpawnTempo,
    pub limit_count: Option<i32>,
    pub activation_event: String,
}
impl Default for Summoning {
    fn default() -> Self {
        Self {
            id_name: "new_summoning".to_string(),
            wisp_types: vec![WispType::Fire],
            area: SpawnArea::default(),
            tempo: SpawnTempo::default(),
            limit_count: None,
            activation_event: "game-started".to_string(),
        }
    }
}
impl Summoning {
    pub fn get_random_wisp_type(&self, rng: &mut nanorand::tls::TlsWyRand) -> WispType {
        self.wisp_types[rng.generate_range(0..self.wisp_types.len())]
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, EnumIter, AsRefStr)]
#[serde(tag = "kind", rename_all = "snake_case")]
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
        // Nano-rand is off by 1 in i32!
        match self {
            SpawnArea::Coords { coords } => {
                let idx = rng.generate_range(0..coords.len());
                coords[idx]
            }
            SpawnArea::Rect { origin, width, height } => {
                let x = origin.x + rng.generate_range(1..=*width);
                let y = origin.y + rng.generate_range(1..=*height);
                GridCoords { x, y }
            }
            SpawnArea::Edge { side } => {
                let (width, height) = obstacle_grid.bounds();
                match side {
                    EdgeSide::Top => GridCoords { x: rng.generate_range(1..=width), y: height - 1 },
                    EdgeSide::Bottom => GridCoords { x: rng.generate_range(1..=width), y: 0 },
                    EdgeSide::Left => GridCoords { x: 0, y: rng.generate_range(1..=height) },
                    EdgeSide::Right => GridCoords { x: width - 1, y: rng.generate_range(1..=height) },
                }
            }
            SpawnArea::EdgesAll => {
                let (width, height) = obstacle_grid.bounds();
                let edge_count = 2 * (width + height - 2);
                let edge_idx = rng.generate_range(1..=edge_count);

                if edge_idx < width {
                    GridCoords { x: edge_idx, y: 0 }
                } else if edge_idx < width + height - 1 {
                    GridCoords { x: width - 1, y: edge_idx - width + 1 }
                } else if edge_idx < 2 * width + height - 2 {
                    GridCoords { x: width - 1 - (edge_idx - width - height + 1), y: height - 1 }
                } else {
                    GridCoords { x: 0, y: height - 1 - (edge_idx - 2 * width - height + 2) }
                }
            }
        }
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, EnumIter, AsRefStr)]
#[serde(rename_all = "snake_case")]
pub enum EdgeSide {
    #[default]
    Top,
    Bottom,
    Left,
    Right,
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SpawnTempo {
    /// Spawn `count` wisps every `seconds` (optional jitter). If `count` omitted -> 1.
    Continuous { seconds: f32, #[serde(default)] jitter: f32, #[serde(default = "default_one")] bulk_count: i32 },
}

impl Default for SpawnTempo {
    fn default() -> Self {
        Self::Continuous { seconds: 1.0, jitter: 0.0, bulk_count: 1 }
    }
}

fn default_one() -> i32 { 1 }

// --------------- SUMMONING RUNTIME ---------------

#[derive(Component, Default)]
pub struct SummoningRuntime {
    pub produced: i32,
    pub next_spawn_time: f32,
}

// --------------- BUILDER PATTERN ---------------

#[derive(Component, SSS)]
pub struct BuilderSummoning {
    pub summoning: Summoning,
    /// Runtime state to restore. `None` on fresh spawn (runtime defaults apply);
    /// `Some` on load (restores mid-wave progress).
    pub runtime: Option<SummoningRuntimeState>,
}

/// Snapshot of `SummoningRuntime` + active marker, used to restore a summoning
/// mid-wave.
#[derive(Clone, Copy, Debug)]
pub struct SummoningRuntimeState {
    pub produced: i32,
    pub next_spawn_time: f32,
    pub is_active: bool,
}

impl BuilderSummoning {
    pub fn new(summoning: Summoning) -> Self {
        Self { summoning, runtime: None }
    }

    /// Set runtime state to restore (used by the loader).
    pub fn with_runtime(mut self, runtime: SummoningRuntimeState) -> Self {
        self.runtime = Some(runtime);
        self
    }
}
