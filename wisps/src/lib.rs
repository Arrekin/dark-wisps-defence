use bevy::{ecs::query::QueryData, prelude::*};

use alteration::{
    effects::prelude::EffectVisualState,
    modifiers::prelude::{AttackRange, IncomingDamageMultiplier, MaxIntegrityPoints, ModifierBank, MovementSpeed},
};
use fields::prelude::FieldAffectable;
use game_core::{motion::Locomotion, prelude::{GridArea, GridCoords, GridImprint, MapBound}};
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
    FieldAffectable, EffectVisualState, Locomotion, GridArea
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

pub const WISP_GRID_IMPRINT: GridImprint = GridImprint::square(1);

/// Wisp path and current cell, used to predict its next position.
#[derive(QueryData)]
pub struct WispHeading {
    grid_path: &'static GridPath,
    /// Read for its origin, the wisp's current cell.
    area: &'static GridArea,
}
impl WispHeadingItem<'_, '_> {
    /// The next path cell, or the current cell if no next step exists.
    pub fn next_cell(&self) -> GridCoords {
        self.grid_path.next_in_path().unwrap_or(self.area.origin)
    }
    /// World-space center of [`Self::next_cell`].
    pub fn next_cell_center(&self) -> Vec2 {
        GridArea::cell(self.next_cell()).world_center()
    }
}

/// Fired when a wisp dies (integrity reaches zero). The wisp entity is
/// despawned in the same command queue batch — observers must not read it.
#[derive(EntityEvent, Clone, Copy)]
pub struct WispDied(pub Entity);

pub mod prelude {
    pub use super::{
        WISP_GRID_IMPRINT,
        WispHeading,
        Wisp,
        WispChargeAttack,
        WispDied,
        WispState,
    };
}
