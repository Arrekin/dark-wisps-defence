use bevy::{picking::hover::Hovered, prelude::*};

use widgets::prelude::HoverBackground;

pub(crate) struct HoverBackgroundPlugin;
impl Plugin for HoverBackgroundPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostUpdate, apply_hover_backgrounds);
    }
}

fn apply_hover_backgrounds(
    mut backgrounds: Query<(&HoverBackground, &Hovered, &mut BackgroundColor), Or<(Changed<HoverBackground>, Changed<Hovered>)>>,
) {
    for (hover_background, hovered, mut background_color) in backgrounds.iter_mut() {
        background_color.set_if_neq(BackgroundColor(hover_background.color(hovered.get())));
    }
}
