use bevy::prelude::*;

use alteration::effects::prelude::EffectVisualState;
use alteration::modifiers::prelude::{AttackRange, IncomingDamageMultiplier, MaxIntegrityPoints, ModifierBank, MovementSpeed};
use game_core::prelude::{FieldAffectable, GridImprint, Locomotion, MapBound, ZDepth};
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
    FieldAffectable, EffectVisualState, Locomotion, ZDepth::WISP
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

/// Fired when a wisp dies (integrity reaches zero). The wisp entity is
/// despawned in the same command queue batch — observers must not read it.
#[derive(EntityEvent, Clone, Copy)]
pub struct WispDied(pub Entity);

pub mod prelude {
    pub use super::{
        WISP_GRID_IMPRINT,
        Wisp,
        WispChargeAttack,
        WispDied,
        WispState,
    };
}
