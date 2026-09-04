use bevy::prelude::*;

use game_core::prelude::{DisplayDescription, DisplayIcon, DisplayName};
use widgets::prelude::{
    BuilderChip, BuilderDisplayChip, BuilderTooltip, ChipChildren, DisplayChipChildren,
    DisplayChipOf, DisplayChips,
};
use widgets::common::utils::set_text_if_changed;

use super::chip::CHIP_FONT_SIZE;

// Tooltip text
const TOOLTIP_TITLE_FONT_SIZE: f32 = CHIP_FONT_SIZE;
const TOOLTIP_BODY_FONT_SIZE: f32 = 11.0;
// #EAF4FF primary text, #8BA8CC secondary.
const TOOLTIP_TITLE_COLOR: Color = Color::srgb_u8(0xEA, 0xF4, 0xFF);
const TOOLTIP_BODY_COLOR: Color = Color::srgb_u8(0x8B, 0xA8, 0xCC);

pub struct DisplayChipPlugin;
impl Plugin for DisplayChipPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_observer(on_builder_add_spawn_display_chip)
            // Each display component updates independently so changes only touch their own node.
            .add_systems(Update, (
                sync_display_chip_icons,
                sync_display_chip_names,
                sync_display_chip_descriptions,
            ));
    }
}

/// Builds an icon-only chip linked to its display subject. Name and description appear in its
/// tooltip; missing display components leave their corresponding content empty.
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
        // A request whose subject was despawned before command flush cannot produce a chip.
        commands.entity(chip_entity).despawn();
        return;
    };

    let tooltip = commands.spawn(BuilderTooltip::new(chip_entity)).id();
    let tooltip_title = commands.spawn((
        Text::new(name.map(|name| name.0.clone()).unwrap_or_default()),
        TextFont::from_font_size(TOOLTIP_TITLE_FONT_SIZE),
        TextColor::from(TOOLTIP_TITLE_COLOR),
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
