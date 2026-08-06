use bevy::app::{App, Plugin};

pub(crate) mod chip;
pub(crate) mod cost;
pub(crate) mod display;
pub(crate) mod strip;

/// Aggregates the chip core, the strip, and every chip specialization. Each
/// specialization is its own plugin so that adding one touches a single file.
pub struct ChipsPlugin;
impl Plugin for ChipsPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                chip::ChipPlugin,
                cost::CostChipPlugin,
                display::DisplayChipPlugin,
                strip::ChipStripPlugin,
            ));
    }
}
