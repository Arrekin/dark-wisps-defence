use bevy::prelude::*;

use resources::prelude::Stock;
use widgets::prelude::{
    BuilderChip, BuilderChipStrip, BuilderCostChip, BuilderFullPriceCostStrip, BuilderTooltip,
    ChipChildren, CostChip, CostChipVisualFullPrice, CostChipVisualUnitAvailable,
};
use widgets::common::utils::set_text_if_changed;

use super::chip::CHIP_FONT_SIZE;

pub struct CostChipPlugin;
impl Plugin for CostChipPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_observer(on_builder_add_spawn_cost_chip)
            .add_observer(on_builder_add_spawn_full_price_cost_strip)
            .add_systems(Update, (
                sync_cost_chip_contents,
                update_cost_chip_borders,
            ));
    }
}

/// Expands a cost request into the shared chip builder and runtime cost state, then attaches the
/// resource-name tooltip. Border color is applied by `update_cost_chip_borders` after insertion.
fn on_builder_add_spawn_cost_chip(
    trigger: On<Add, BuilderCostChip>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    builders: Query<&BuilderCostChip>,
) {
    let chip_entity = trigger.entity;
    let Ok(builder) = builders.get(chip_entity) else { return };

    let resource_type = builder.resource_type;
    let amount = builder.amount;

    commands.entity(chip_entity)
        .remove::<BuilderCostChip>()
        .insert((
            BuilderChip {
                icon: asset_server.load(resource_type.icon_path()),
                text: Some(amount.to_string()),
            },
            CostChip { resource_type, amount },
        ));

    // The single-line resource name uses a content-sized tooltip.
    let tooltip = commands.spawn(BuilderTooltip::new(chip_entity).sized_to_content()).id();
    commands.entity(tooltip).with_child((
        Text::new(resource_type.to_string()),
        TextFont::from_font_size(CHIP_FONT_SIZE),
        TextColor::from(Color::WHITE),
        TextLayout::no_wrap(),
    ));
}

/// Expands into [`BuilderChipStrip`] with one chip per cost.
fn on_builder_add_spawn_full_price_cost_strip(
    trigger: On<Add, BuilderFullPriceCostStrip>,
    mut commands: Commands,
    builders: Query<&BuilderFullPriceCostStrip>,
) {
    let strip_entity = trigger.entity;
    let Ok(builder) = builders.get(strip_entity) else { return };
    let costs = builder.0.clone();

    commands.entity(strip_entity)
        .remove::<BuilderFullPriceCostStrip>()
        .insert(BuilderChipStrip)
        .with_children(|strip| {
            for cost in costs {
                strip.spawn((BuilderCostChip::from(cost), CostChipVisualFullPrice));
            }
        });
}

/// Rewrites the amount text when the owner mutates `CostChip`. The icon is
/// set at spawn and never changes — a chip's resource type is fixed for life.
fn sync_cost_chip_contents(
    chips: Query<(&CostChip, &ChipChildren), Changed<CostChip>>,
    mut texts: Query<&mut Text>,
) {
    for (chip, children) in chips.iter() {
        let Some(text_entity) = children.text else { continue };
        let Ok(mut text) = texts.get_mut(text_entity) else { continue };

        set_text_if_changed(&mut text, &chip.amount.to_string());
    }
}

/// Updates specialized chip borders when stock or displayed cost changes. Unspecialized chips are
/// excluded and retain their neutral border.
fn update_cost_chip_borders(
    stock: Res<Stock>,
    mut chips: Query<
        (Ref<CostChip>, &mut BorderColor, Has<CostChipVisualFullPrice>),
        Or<(With<CostChipVisualFullPrice>, With<CostChipVisualUnitAvailable>)>,
    >,
) {
    let stock_changed = stock.is_changed();
    for (chip, mut border_color, is_full_price) in chips.iter_mut() {
        if !stock_changed && !chip.is_changed() { continue }

        let affordable = if is_full_price {
            stock.has(chip.resource_type, chip.amount)
        } else {
            // Nothing owed cannot block, so a spent cost stays affordable.
            chip.amount <= 0 || stock.has(chip.resource_type, 1)
        };
        *border_color = BorderColor::all(availability_color(affordable));
    }
}

/// Returns the palette colors for affordable and blocking costs.
fn availability_color(affordable: bool) -> Color {
    if affordable {
        Color::srgb_u8(0x35, 0xB8, 0x7A)
    } else {
        Color::srgb_u8(0xFF, 0x3D, 0x8D)
    }
}
