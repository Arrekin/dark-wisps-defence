//! # Lob Preview
//!
//! Manages [`LobPreviewScope`] and [`LobPreviewShown`] lifecycle, and eases displayed preview arcs
//! toward each entity's target [`LobArc`].
//!
//! Focusing a [`LobPreview`] entity scopes the preview to that entity unless [`LobPreviewScope::Global`]
//! is active. Pressing `Digit9` toggles [`LobPreviewScope::Global`], restoring the focused entity's
//! scope when turned off.
//!
//! When [`LobPreviewScope`] changes, [`LobPreviewShown`] is added or removed to match while preserving
//! existing [`LobPreviewShown`] state on entities that remain in scope.

use bevy::{input::common_conditions::input_just_released, prelude::*};

use hud::prelude::FocusedMapObject;
use weaponry::lob::{LobArc, LobPreview, LobPreviewScope, LobPreviewShown, LobPreviewSystems};

pub(crate) struct LobPreviewPlugin;
impl Plugin for LobPreviewPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<LobPreviewScope>()
            .add_systems(Update, (
                apply_lob_preview_scope.run_if(resource_changed::<LobPreviewScope>),
                ease_lob_preview_arcs.run_if(any_with_component::<LobPreviewShown>),
            ).chain().in_set(LobPreviewSystems))
            .add_systems(Update, toggle_global_lob_preview.run_if(input_just_released(KeyCode::Digit9)).before(LobPreviewSystems))
            .add_observer(on_insert_focused_map_object_scope_lob_preview)
            .add_observer(on_remove_focused_map_object_unscope_lob_preview)
            .add_observer(on_add_lob_preview_show_if_in_scope)
            .add_observer(on_remove_lob_arc_clear_lob_preview);
    }
}

// Easing
/// Exponential smoothing rate toward a new arc, in inverse seconds.
const SETTLE_RATE: f32 = 8.;
/// World-unit gap below which a shown arc snaps onto its entity's `LobArc`.
const SNAP_DISTANCE: f32 = 0.1;

fn on_insert_focused_map_object_scope_lob_preview(
    trigger: On<Insert<FocusedMapObject>>,
    mut scope: ResMut<LobPreviewScope>,
    previews: Query<(), With<LobPreview>>,
) {
    if !previews.contains(trigger.entity) || *scope == LobPreviewScope::Global { return; }
    scope.set_if_neq(LobPreviewScope::Entity(trigger.entity));
}

fn on_remove_focused_map_object_unscope_lob_preview(
    trigger: On<Remove<FocusedMapObject>>,
    mut scope: ResMut<LobPreviewScope>,
) {
    if *scope != LobPreviewScope::Entity(trigger.entity) { return; }
    *scope = LobPreviewScope::None;
}

/// Toggles [`LobPreviewScope::Global`], falling back to the focused [`LobPreview`] entity when disabled.
fn toggle_global_lob_preview(
    mut scope: ResMut<LobPreviewScope>,
    focused: Option<Single<Entity, (With<FocusedMapObject>, With<LobPreview>)>>,
) {
    *scope = match *scope {
        LobPreviewScope::Global => focused.map_or(LobPreviewScope::None, |focused| LobPreviewScope::Entity(*focused)),
        _ => LobPreviewScope::Global,
    };
}

fn apply_lob_preview_scope(
    mut commands: Commands,
    scope: Res<LobPreviewScope>,
    previews: Query<(Entity, Has<LobPreviewShown>), With<LobPreview>>,
) {
    for (entity, is_shown) in previews.iter() {
        match (scope.covers(entity), is_shown) {
            (true, false) => { commands.entity(entity).insert(LobPreviewShown::default()); }
            (false, true) => { commands.entity(entity).remove::<LobPreviewShown>(); }
            _ => {}
        }
    }
}

fn on_add_lob_preview_show_if_in_scope(
    trigger: On<Add<LobPreview>>,
    mut commands: Commands,
    scope: Res<LobPreviewScope>,
) {
    if !scope.covers(trigger.entity) { return; }
    commands.entity(trigger.entity).insert(LobPreviewShown::default());
}

fn on_remove_lob_arc_clear_lob_preview(
    trigger: On<Remove<LobArc>>,
    mut previews: Query<&mut LobPreviewShown>,
) {
    let Ok(mut shown) = previews.get_mut(trigger.entity) else { return; };
    shown.arc = None;
}

fn ease_lob_preview_arcs(
    time: Res<Time>,
    mut previews: Query<(&LobArc, &mut LobPreviewShown)>,
) {
    let settle = 1. - (-SETTLE_RATE * time.delta_secs()).exp();
    for (arc, mut shown) in previews.iter_mut() {
        match shown.arc {
            None => shown.arc = Some(*arc),
            Some(shown_arc) if shown_arc == *arc => {}
            Some(shown_arc) => {
                let eased = shown_arc.lerp(arc, settle);
                let gap = eased.origin.distance(arc.origin).max(eased.target.distance(arc.target)).max((eased.apex - arc.apex).abs());
                shown.arc = Some(if gap < SNAP_DISTANCE { *arc } else { eased });
            }
        }
    }
}
