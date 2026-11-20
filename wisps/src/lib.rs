use bevy::prelude::*;

use alteration::{
    effects::prelude::EffectVisualState,
    modifiers::prelude::{IncomingDamageMultiplier, MaxAttackRange, MaxIntegrityPoints, MinAttackRange, ModifierBank, MovementSpeed},
};
use fields::prelude::FieldAffectable;
use game_core::{motion::Locomotion, prelude::{GridArea, GridImprint, MapBound}};
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
    MovementSpeed, MinAttackRange, MaxAttackRange, MaxIntegrityPoints, IncomingDamageMultiplier,
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
