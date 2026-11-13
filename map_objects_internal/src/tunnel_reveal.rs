//! # Tunnel reveal
//!
//! The wall and underground canvases share [`TunnelReveal`] to make tunnel roofs translucent
//! and expose the passages beneath.
//!
//! Reveal is active while any entity carries [`RevealsTunnels`], or while the cursor is outside UI
//! and over a wall or excavation with tunnel openings. The fade uses real time so it works while
//! paused.

use bevy::prelude::*;

use indexes::prelude::ObstacleGrid;
use map_objects::prelude::VolumeShape;
use viewport::MouseInfo;

pub(crate) struct TunnelRevealPlugin;
impl Plugin for TunnelRevealPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<TunnelReveal>()
            .add_systems(Update, fade_tunnel_reveal);
    }
}

/// Seconds the tunnel reveal takes to fade fully in or out.
const TUNNEL_REVEAL_FADE_SECONDS: f32 = 0.13;

/// Holds the tunnel reveal active for as long as an entity carries it.
#[derive(Component, Default)]
pub(crate) struct RevealsTunnels;

/// Shared reveal amount, from 0 (opaque roofs) to 1 (translucent roofs).
#[derive(Resource, Default)]
pub(crate) struct TunnelReveal {
    pub amount: f32,
}

fn fade_tunnel_reveal(
    mut reveal: ResMut<TunnelReveal>,
    mouse_info: Res<MouseInfo>,
    obstacle_grid: Res<ObstacleGrid>,
    time: Res<Time<Real>>,
    reveal_holders: Query<(), With<RevealsTunnels>>,
) {
    let is_held = !reveal_holders.is_empty();
    let over_tunnel = !mouse_info.is_over_ui
        && obstacle_grid.get(mouse_info.grid_coords).is_some_and(|cell| {
            [cell.elevation, cell.underground].iter().any(|structure| structure.shape().is_some_and(VolumeShape::is_hollow))
        });
    let target = if is_held || over_tunnel { 1.0 } else { 0.0 };
    if reveal.amount == target { return; }

    let step = time.delta_secs() / TUNNEL_REVEAL_FADE_SECONDS;
    reveal.amount += (target - reveal.amount).clamp(-step, step);
}
