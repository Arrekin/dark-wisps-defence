use bevy::color::palettes::css::{GREEN, RED};
use bevy::prelude::*;

use resources::prelude::Stock;
use widgets::prelude::{
    BuilderChip, BuilderCostChip, ChipChildren, CostChip, CostChipVisualFullPrice,
    CostChipVisualUnitAvailable, TooltipBundle,
};
use widgets::utils::set_text_if_changed;

use super::chip::CHIP_FONT_SIZE;

pub struct CostChipPlugin;
impl Plugin for CostChipPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_observer(on_builder_add_spawn_cost_chip)
            .add_systems(Update, (
                sync_cost_chip_contents,
                update_cost_chip_borders,
            ));
    }
}

/// Expands the builder into the core's `BuilderChip` plus the runtime
/// `CostChip`, then attaches the resource-name tooltip. The icon-and-amount
/// tree itself is built by `on_builder_add_spawn_chip` once `BuilderChip`
/// lands — this observer never touches nodes directly.
///
/// The border is left neutral here rather than computed: inserting `CostChip`
/// counts as a change, so `update_cost_chip_borders` paints it on the next
/// frame and the affordability rule lives in exactly one place.
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

    let tooltip = commands.spawn(TooltipBundle::new(chip_entity)).id();
    commands.entity(tooltip).with_child((
        Text::new(resource_type.to_string()),
        TextFont::from_font_size(CHIP_FONT_SIZE),
        TextColor::from(Color::WHITE),
        TextLayout::no_wrap(),
    ));
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

/// Paints the border of every specialized chip. Two triggers matter — the
/// stock moved, or the owner rewrote the amount — so this is one system
/// filtering on both rather than a pair of run conditions that would each
/// miss the other's trigger.
///
/// Chips with no specialization are excluded by the query and keep their
/// neutral border for life.
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

fn availability_color(affordable: bool) -> Color {
    if affordable { GREEN.into() } else { RED.into() }
}
