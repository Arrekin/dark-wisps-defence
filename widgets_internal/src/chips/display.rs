use bevy::prelude::*;

use game_core::prelude::{DisplayDescription, DisplayIcon, DisplayName};
use widgets::prelude::{
    BuilderChip, BuilderDisplayChip, ChipChildren, DisplayChipChildren, DisplayChipOf, DisplayChips,
    TooltipBundle,
};
use widgets::common::utils::set_text_if_changed;

use super::chip::CHIP_FONT_SIZE;

// Tooltip text
const TOOLTIP_TITLE_FONT_SIZE: f32 = CHIP_FONT_SIZE;
const TOOLTIP_BODY_FONT_SIZE: f32 = 11.0;
const TOOLTIP_BODY_COLOR: Color = Color::linear_rgba(0.75, 0.75, 0.8, 1.);

pub struct DisplayChipPlugin;
impl Plugin for DisplayChipPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_observer(on_builder_add_spawn_display_chip)
            // One system per aspect rather than one filtered on all three: a
            // name change must not cost an icon write, and each aspect has its
            // own node to write to.
            .add_systems(Update, (
                sync_display_chip_icons,
                sync_display_chip_names,
                sync_display_chip_descriptions,
            ));
    }
}

/// Expands into the chip core plus the `DisplayChipOf` relationship, and builds
/// the tooltip this specialization owns.
///
/// Icon-only by design: the chip stays narrow enough for a strip, and the detail
/// lives on hover. A subject missing any of the three display components simply
/// contributes nothing for that aspect.
fn on_builder_add_spawn_display_chip(
    trigger: On<Add, BuilderDisplayChip>,
    mut commands: Commands,
    builders: Query<&BuilderDisplayChip>,
    subjects: Query<(Option<&DisplayIcon>, Option<&DisplayName>, Option<&DisplayDescription>)>,
) {
    let chip_entity = trigger.entity;
    let Ok(builder) = builders.get(chip_entity) else { return };

    let subject = builder.0;
    let Ok((icon, name, description)) = subjects.get(subject) else {
        // The subject vanished between spawn and flush — the chip has nothing
        // to show, so despawn it rather than leaving an empty node behind.
        commands.entity(chip_entity).despawn();
        return;
    };

    let tooltip = commands.spawn(TooltipBundle::new(chip_entity)).id();
    let tooltip_title = commands.spawn((
        Text::new(name.map(|name| name.0.clone()).unwrap_or_default()),
        TextFont::from_font_size(TOOLTIP_TITLE_FONT_SIZE),
        TextColor::from(Color::WHITE),
    )).id();
    let tooltip_body = commands.spawn((
        Text::new(description.map(|body| body.0.clone()).unwrap_or_default()),
        TextFont::from_font_size(TOOLTIP_BODY_FONT_SIZE),
        TextColor::from(TOOLTIP_BODY_COLOR),
    )).id();
    commands.entity(tooltip).add_children(&[tooltip_title, tooltip_body]);

    commands.entity(chip_entity)
        .remove::<BuilderDisplayChip>()
        .insert((
            BuilderChip::new(icon.map(|icon| icon.0.clone()).unwrap_or_default()),
            DisplayChipOf(subject),
            DisplayChipChildren { tooltip_title, tooltip_body },
        ));
}

/// Pushes a changed icon into every chip showing that subject.
///
/// Driven from the subject side so only subjects that actually have chips are
/// scanned, and so one subject shown in several panels updates all of them.
fn sync_display_chip_icons(
    subjects: Query<(&DisplayChips, &DisplayIcon), Changed<DisplayIcon>>,
    chips: Query<&ChipChildren>,
    mut image_nodes: Query<&mut ImageNode>,
) {
    for (display_chips, icon) in subjects.iter() {
        for chip in display_chips.iter() {
            let Ok(children) = chips.get(chip) else { continue };
            let Ok(mut image_node) = image_nodes.get_mut(children.icon) else { continue };
            if image_node.image != icon.0 {
                image_node.image = icon.0.clone();
            }
        }
    }
}

/// Pushes a changed name into the tooltip title of every chip showing it.
fn sync_display_chip_names(
    subjects: Query<(&DisplayChips, &DisplayName), Changed<DisplayName>>,
    chips: Query<&DisplayChipChildren>,
    mut texts: Query<&mut Text>,
) {
    for (display_chips, name) in subjects.iter() {
        for chip in display_chips.iter() {
            let Ok(children) = chips.get(chip) else { continue };
            let Ok(mut text) = texts.get_mut(children.tooltip_title) else { continue };
            set_text_if_changed(&mut text, &name.0);
        }
    }
}

/// Pushes a changed description into the tooltip body of every chip showing it.
fn sync_display_chip_descriptions(
    subjects: Query<(&DisplayChips, &DisplayDescription), Changed<DisplayDescription>>,
    chips: Query<&DisplayChipChildren>,
    mut texts: Query<&mut Text>,
) {
    for (display_chips, description) in subjects.iter() {
        for chip in display_chips.iter() {
            let Ok(children) = chips.get(chip) else { continue };
            let Ok(mut text) = texts.get_mut(children.tooltip_body) else { continue };
            set_text_if_changed(&mut text, &description.0);
        }
    }
}
