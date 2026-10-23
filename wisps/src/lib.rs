use bevy::{ecs::query::QueryData, prelude::*};

use alteration::effects::prelude::EffectVisualState;
use alteration::modifiers::prelude::{AttackRange, IncomingDamageMultiplier, MaxIntegrityPoints, ModifierBank, MovementSpeed};
use fields::prelude::FieldAffectable;
use game_core::{motion::Locomotion, prelude::{GridCoords, GridImprint, MapBound, WispType}};
use grids::prelude::{GridPath, GridVersion};

pub mod summoning;

#[derive(Component)]
pub struct WispFireType;
#[derive(Component)]
pub struct WispWaterType;
#[derive(Component)]
pub struct WispLightType;
#[derive(Component)]
pub struct WispElectricType;

#[derive(Component, Debug, Default, PartialEq)]
#[require(
    WispState, WispChargeAttack, GridPath, ModifierBank, MapBound,
    MovementSpeed, AttackRange, MaxIntegrityPoints, IncomingDamageMultiplier,
    FieldAffectable, EffectVisualState, Locomotion, GridImprint = WISP_GRID_IMPRINT
)]
pub struct Wisp;

#[derive(Component, Default)]
pub enum WispState {
    #[default]
    JustSpawned,
    NeedTarget,
    MovingToTarget,
    Attacking,
    Stranded(GridVersion),
}

#[derive(Component, Default)]
pub enum WispChargeAttack {
    #[default]
    Charge,
    Backoff,
}

pub const WISP_GRID_IMPRINT: GridImprint = GridImprint::Rectangle { width: 1, height: 1 };

/// Wisp path and current cell, used to predict its next position.
#[derive(QueryData)]
pub struct WispHeading {
    grid_path: &'static GridPath,
    grid_coords: &'static GridCoords,
}
impl WispHeadingItem<'_, '_> {
    /// The next path cell, or the current cell if no next step exists.
    pub fn next_cell(&self) -> GridCoords {
        self.grid_path.next_in_path().unwrap_or(*self.grid_coords)
    }
    /// World-space center of [`Self::next_cell`].
    pub fn next_cell_center(&self) -> Vec2 {
        self.next_cell().to_world_position_centered(WISP_GRID_IMPRINT)
    }
}

/// Spawns a side-menu tooltip for `wisp_type`, anchored to `anchor`.
#[derive(Component, Clone, Copy, Debug)]
pub struct BuilderWispSideMenuTooltip {
    pub anchor: Entity,
    pub wisp_type: WispType,
}

/// Fired when a wisp dies (integrity reaches zero). The wisp entity is
/// despawned in the same command queue batch — observers must not read it.
#[derive(EntityEvent, Clone, Copy)]
pub struct WispDied(pub Entity);

pub mod prelude {
    pub use super::{
        WISP_GRID_IMPRINT,
        WispHeading,
        BuilderWispSideMenuTooltip,
        Wisp,
        WispChargeAttack,
        WispDied,
        WispState,
    };
}
