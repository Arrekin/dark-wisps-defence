use bevy::prelude::*;

use widgets::prelude::{BuilderChip, Chip, ChipChildren, ChipsFaded};

// Chip body
const CHIP_HEIGHT: f32 = 24.0;
const CHIP_ICON_SIZE: f32 = 18.0;
const CHIP_BACKGROUND: Color = Color::linear_rgba(0.12, 0.12, 0.18, 1.);

/// Border of a chip nothing has coloured. A specialization that paints the
/// border keeps this only for the frame between spawn and its first update.
const CHIP_NEUTRAL_BORDER: Color = Color::linear_rgba(0.4, 0.4, 0.4, 1.);

// Shared with the specializations, which style on top of the core.
pub(super) const CHIP_FONT_SIZE: f32 = 12.0;

pub struct ChipPlugin;
impl Plugin for ChipPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_observer(on_builder_add_spawn_chip)
            .add_observer(on_add_chips_faded_dim_chips)
            .add_observer(on_remove_chips_faded_restore_chips);
    }
}

/// Builds the chip tree and records its nodes. That is the whole widget — no
/// tooltip, no colour states, no notion of what is being shown. Specializations
/// expand into `BuilderChip` and then write `ChipChildren` directly.
fn on_builder_add_spawn_chip(
    trigger: On<Add, BuilderChip>,
    mut commands: Commands,
    builders: Query<&BuilderChip>,
) {
    let chip_entity = trigger.entity;
    let Ok(builder) = builders.get(chip_entity) else { return };

    let icon = commands.spawn((
        ImageNode::new(builder.icon.clone()),
        Node {
            width: Val::Px(CHIP_ICON_SIZE),
            height: Val::Px(CHIP_ICON_SIZE),
            ..default()
        },
    )).id();

    // An icon-only chip spawns no text node at all, rather than an empty one —
    // `ChipChildren.text` being `None` is what tells a specialization there is
    // nothing to write to.
    let text = builder.text.as_ref().map(|content| commands.spawn((
        Text::new(content.clone()),
        TextFont::from_font_size(CHIP_FONT_SIZE),
        TextColor::from(Color::WHITE),
        TextLayout::no_wrap(),
    )).id());

    commands.entity(chip_entity)
        .remove::<BuilderChip>()
        .insert((
            Chip,
            ChipChildren { icon, text },
            Node {
                height: Val::Px(CHIP_HEIGHT),
                padding: UiRect::horizontal(Val::Px(4.)),
                column_gap: Val::Px(2.),
                align_items: AlignItems::Center,
                border: UiRect::all(Val::Px(1.)),
                border_radius: BorderRadius::all(Val::Px(3.)),
                ..default()
            },
            BackgroundColor::from(CHIP_BACKGROUND),
            BorderColor::all(CHIP_NEUTRAL_BORDER),
        ))
        .add_child(icon);

    if let Some(text) = text {
        commands.entity(chip_entity).add_child(text);
    }
}

// ============================================================================
// FADING — ChipsFaded dims the icon and label of the chips beneath it
// ============================================================================

fn on_add_chips_faded_dim_chips(
    trigger: On<Add, ChipsFaded>,
    mut commands: Commands,
    faded: Query<&ChipsFaded>,
    chips: Query<&ChipChildren>,
    children: Query<&Children>,
    mut image_nodes: Query<&mut ImageNode>,
) {
    let Ok(faded) = faded.get(trigger.entity) else { return };
    let brightness = faded.0.clamp(0., 1.);
    paint_chips(
        trigger.entity,
        Color::srgb(brightness, brightness, brightness),
        &chips,
        &children,
        &mut image_nodes,
        &mut commands,
    );
}

fn on_remove_chips_faded_restore_chips(
    trigger: On<Remove, ChipsFaded>,
    mut commands: Commands,
    chips: Query<&ChipChildren>,
    children: Query<&Children>,
    mut image_nodes: Query<&mut ImageNode>,
) {
    paint_chips(
        trigger.entity,
        Color::WHITE,
        &chips,
        &children,
        &mut image_nodes,
        &mut commands,
    );
}

/// Paints the icon and label of every chip in `entity`'s subtree, `entity` included, so the
/// component works on a strip and on a lone chip alike. Entities that are not chips are
/// skipped, which is what lets it run over a subtree it knows nothing about.
fn paint_chips(
    entity: Entity,
    color: Color,
    chips: &Query<&ChipChildren>,
    children: &Query<&Children>,
    image_nodes: &mut Query<&mut ImageNode>,
    commands: &mut Commands,
) {
    for chip_entity in core::iter::once(entity).chain(children.iter_descendants(entity)) {
        let Ok(chip) = chips.get(chip_entity) else { continue };
        if let Ok(mut icon) = image_nodes.get_mut(chip.icon) {
            icon.color = color;
        }
        if let Some(text) = chip.text {
            commands.entity(text).insert(TextColor::from(color));
        }
    }
}
