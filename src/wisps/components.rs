use crate::prelude::*;

#[derive(Component, Debug, Default, PartialEq)]
#[require(
    WispState, WispChargeAttack, GridPath, ModifierBank, MapBound,
    MovementSpeed, AttackRange, MaxIntegrityPoints, IncomingDamageMultiplier
)]
pub struct Wisp;
#[derive(Component, Default)]
pub enum WispState {
    #[default]
    JustSpawned,
    NeedTarget,
    MovingToTarget,
    Attacking,
    Stranded(GridVersion), // No target available, waiting for change in obstacle grid
}

#[derive(Component, Default)]
pub enum WispChargeAttack {
    #[default]
    Charge,
    Backoff,
}