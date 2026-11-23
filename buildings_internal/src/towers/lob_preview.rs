//! # Tower Lob Preview
//!
//! Populates [`LobArc`] on towers with [`LobPreviewShown`] so the weaponry preview system can render
//! their next shot.
//!
//! Towers only carry [`LobArc`] while their preview is shown. Each lob tower type registers its
//! aiming sync system in [`TowerLobArcSystems`], which runs after [`TowerAimingSystems`] and before
//! [`LobPreviewSystems`].

use bevy::prelude::*;

use buildings::prelude::*;
use game_core::prelude::IsOperational;
use weaponry::lob::{LobArc, LobPreviewShown, LobPreviewSystems, LobProfile};

use crate::common::{PredictiveAim, TowerAimingSystems};

pub(crate) struct TowerLobPreviewPlugin;
impl Plugin for TowerLobPreviewPlugin {
    fn build(&self, app: &mut App) {
        app
            .configure_sets(Update, TowerLobArcSystems.after(TowerAimingSystems).before(LobPreviewSystems))
            .add_observer(on_remove_lob_preview_shown_clear_tower_lob_arc);
    }
}

/// Updates [`LobArc`] on towers with [`LobPreviewShown`].
#[derive(SystemSet, Clone, Copy, Eq, PartialEq, Hash, Debug)]
pub(crate) struct TowerLobArcSystems;

/// Updates [`LobArc`] for previewed towers of type `T` to aim at their target wisp's predicted next
/// cell.
///
/// Removes [`LobArc`] when the tower is non-operational or [`TowerWispTarget::has_no_targets`] is
/// true. Retains the previous [`LobArc`] while [`TowerWispTarget::SearchForNewTarget`] is pending so
/// the preview does not flicker between consecutive targets.
pub(crate) fn sync_lob_arc_with_predictive_aiming<T: Component>(
    mut commands: Commands,
    predictive_aim: PredictiveAim,
    towers: Query<(Entity, &Transform, &LobProfile, &TowerWispTarget, Has<IsOperational>, Option<&LobArc>), (With<T>, With<LobPreviewShown>)>,
) {
    for (tower, transform, lob_profile, target, is_operational, current_arc) in towers.iter() {
        if !is_operational || target.has_no_targets() {
            if current_arc.is_some() { commands.entity(tower).remove::<LobArc>(); }
            continue;
        }
        let Some(aim_coords) = target.wisp().and_then(|wisp| predictive_aim.next_cell(wisp)) else { continue; };
        let arc = lob_profile.arc(transform.translation.xy(), aim_coords.world_center());
        if current_arc != Some(&arc) { commands.entity(tower).insert(arc); }
    }
}

/// Removes [`LobArc`] when a tower's preview is hidden so re-showing it snaps directly to the new aim.
fn on_remove_lob_preview_shown_clear_tower_lob_arc(
    trigger: On<Remove<LobPreviewShown>>,
    mut commands: Commands,
    towers: Query<(), (With<Tower>, With<LobArc>)>,
) {
    if !towers.contains(trigger.entity) { return; }
    commands.entity(trigger.entity).try_remove::<LobArc>();
}
