use bevy::prelude::*;

use widgets::prelude::{BuilderFillBar, FillAxis, FillBar, FillBarChildren};

pub struct FillBarPlugin;
impl Plugin for FillBarPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_observer(on_builder_add_spawn_fill_bar)
            .add_systems(Update, sync_fill_bars);
    }
}

/// Builds the track + fill nodes from the builder spec, inserts the runtime
/// `FillBar`, and records the fill entity via `FillBarChildren`. Insertion
/// of `FillBar` counts as a change, so the first `sync_fill_bars` happens
/// on the next frame with no separate init path.
fn on_builder_add_spawn_fill_bar(
    trigger: On<Add, BuilderFillBar>,
    mut commands: Commands,
    builders: Query<&BuilderFillBar>,
) {
    let entity = trigger.entity;
    let Ok(builder) = builders.get(entity) else { return };

    let flex_direction = match builder.fill_bar.axis {
        FillAxis::Horizontal => FlexDirection::Row,
        FillAxis::Vertical => FlexDirection::Column,
    };
    let justify_content = match builder.fill_bar.axis {
        FillAxis::Horizontal => JustifyContent::FlexStart,
        FillAxis::Vertical => JustifyContent::FlexEnd,
    };

    let mut children_ref = FillBarChildren { fill: Entity::PLACEHOLDER };
    commands.entity(entity)
        .remove::<BuilderFillBar>()
        .insert((
            Node {
                width: Val::Percent(100.),
                height: Val::Percent(100.),
                flex_direction,
                justify_content,
                border: builder.border,
                border_radius: builder.border_radius,
                ..default()
            },
            builder.background_color,
            builder.border_color,
            builder.fill_bar,
        ))
        .with_children(|track| {
            children_ref.fill = track.spawn((
                Node::default(),
                builder.fill_color,
            )).id();
        })
        .insert(children_ref);
}

/// Writes the fill node's size, clamped to 0..=1, axis-aware. Runs on
/// `Changed<FillBar>` only — no per-frame writes for static bars.
fn sync_fill_bars(
    bars: Query<(&FillBar, &FillBarChildren), Changed<FillBar>>,
    mut fill_nodes: Query<&mut Node>,
) {
    for (bar, children) in bars.iter() {
        let Ok(mut node) = fill_nodes.get_mut(children.fill) else { continue };
        let fraction = bar.fill_fraction.clamp(0.0, 1.0);
        (node.width, node.height) = fill_dimensions(bar.axis, fraction);
    }
}

/// Returns `(width, height)` for the fill node given the axis and fraction.
fn fill_dimensions(axis: FillAxis, fraction: f32) -> (Val, Val) {
    let pct = Val::Percent(fraction * 100.0);
    let full = Val::Percent(100.0);
    match axis {
        FillAxis::Horizontal => (pct, full),
        FillAxis::Vertical => (full, pct),
    }
}
