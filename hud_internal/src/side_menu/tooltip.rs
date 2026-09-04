//! Lays out the tooltip a side-menu tile shows on hover.

use bevy::prelude::*;

use hud::prelude::BuilderSideMenuItemTooltip;
use widgets::prelude::{BuilderFullPriceCostStrip, BuilderTooltip, TextRole};

use super::root::SIDE_MENU_LEFT;
use super::section::SIDE_MENU_SECTION_SIZE;

// Typography
const TITLE_FONT_SIZE: f32 = 13.0;
const BODY_FONT_SIZE: f32 = 10.0;
const FACTS_FONT_SIZE: f32 = 10.0;

// Text colors: title, description, then lower-contrast facts.
const TITLE_COLOR: Color = Color::srgb_u8(0xEA, 0xF4, 0xFF);
const BODY_COLOR: Color = Color::srgb_u8(0x8B, 0xA8, 0xCC);
const FACTS_COLOR: Color = Color::srgb_u8(0x6B, 0x82, 0xA0);
/// #233A68 structural border, drawn between the two zones.
const DIVIDER_COLOR: Color = Color::srgb_u8(0x23, 0x3A, 0x68);

/// Chamfer depth. Tooltips default to square corners.
const CORNER_CUT: f32 = 8.0;

// Child zones own the padding. The divider uses the same horizontal inset to align with the text
// and avoid the panel contour.
const ZONE_INSET: f32 = 10.0;
const HEAD_PADDING: UiRect = UiRect::px(ZONE_INSET, ZONE_INSET, 9., 10.);
const FOOT_PADDING: UiRect = UiRect::px(ZONE_INSET, ZONE_INSET, 7., 8.);
const ZONE_ROW_GAP: f32 = 5.0;

/// The menu column's right edge, which a tooltip does not cross.
const SIDE_MENU_RIGHT_EDGE: f32 = SIDE_MENU_LEFT + SIDE_MENU_SECTION_SIZE;

pub(crate) fn on_builder_add_spawn_side_menu_item_tooltip(
    trigger: On<Add, BuilderSideMenuItemTooltip>,
    mut commands: Commands,
    builders: Query<&BuilderSideMenuItemTooltip>,
) {
    let entity = trigger.entity;
    let Ok(builder) = builders.get(entity) else { return; };

    let has_footer = !builder.facts.is_empty() || !builder.cost.is_empty();

    commands.entity(entity)
        .remove::<BuilderSideMenuItemTooltip>()
        .insert(
            BuilderTooltip::new(builder.anchor)
                .with_corner_cut(CORNER_CUT)
                .with_left_limit(SIDE_MENU_RIGHT_EDGE)
                .with_padding(UiRect::ZERO)
                .with_row_gap(0.),
        )
        .with_children(|tooltip| {
            tooltip.spawn(Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(ZONE_ROW_GAP),
                padding: HEAD_PADDING,
                ..default()
            }).with_children(|head| {
                if let Some(name) = &builder.name {
                    head.spawn((
                        Text::new(name.to_uppercase()),
                        TextRole::Heading.font(TITLE_FONT_SIZE),
                        TextColor::from(TITLE_COLOR),
                        TextLayout::no_wrap(),
                    ));
                }
                if let Some(description) = &builder.description {
                    head.spawn((
                        Text::new(description.clone()),
                        TextRole::Body.font(BODY_FONT_SIZE),
                        TextColor::from(BODY_COLOR),
                    ));
                }
            });

            if has_footer {
                tooltip.spawn((
                    Node {
                        height: Val::Px(1.),
                        margin: UiRect::horizontal(Val::Px(ZONE_INSET)),
                        ..default()
                    },
                    BackgroundColor::from(DIVIDER_COLOR),
                ));
                tooltip.spawn(Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(ZONE_ROW_GAP),
                    padding: FOOT_PADDING,
                    ..default()
                }).with_children(|footer| {
                    // Facts wrap within the tooltip width; costs use a fixed-height horizontal row.
                    if !builder.facts.is_empty() {
                        footer.spawn((
                            Text::new(builder.facts.join(" · ")),
                            TextRole::Data.font(FACTS_FONT_SIZE),
                            TextColor::from(FACTS_COLOR),
                        ));
                    }
                    if !builder.cost.is_empty() {
                        footer.spawn(BuilderFullPriceCostStrip(builder.cost.clone()));
                    }
                });
            }
        });
}
