use bevy::prelude::*;

use widgets::prelude::BuilderChipStrip;

const STRIP_HEIGHT: f32 = 28.0;
const STRIP_COLUMN_GAP: f32 = 4.0;

pub struct ChipStripPlugin;
impl Plugin for ChipStripPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(on_builder_add_spawn_chip_strip);
    }
}

/// Applies the strip's layout to the entity the caller spawned. `ScrollPosition`
/// is what `MouseScrollingPlugin` writes, so the strip is scrollable the moment
/// its children overflow.
fn on_builder_add_spawn_chip_strip(
    trigger: On<Add, BuilderChipStrip>,
    mut commands: Commands,
) {
    commands.entity(trigger.entity)
        .remove::<BuilderChipStrip>()
        .insert((
            Node {
                height: Val::Px(STRIP_HEIGHT),
                flex_direction: FlexDirection::Row,
                flex_wrap: FlexWrap::NoWrap,
                column_gap: Val::Px(STRIP_COLUMN_GAP),
                align_items: AlignItems::Center,
                overflow: Overflow::scroll_x(),
                ..default()
            },
            ScrollPosition::default(),
        ));
}
