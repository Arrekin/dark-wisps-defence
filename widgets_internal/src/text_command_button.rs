use bevy::prelude::*;

use widgets::prelude::{
    BuilderTextCommandButton, TextCommandButton, TextCommandButtonChildren,
};

pub(crate) struct TextCommandButtonPlugin;
impl Plugin for TextCommandButtonPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(on_builder_add_spawn_text_command_button);
    }
}

fn on_builder_add_spawn_text_command_button(
    trigger: On<Add, BuilderTextCommandButton>,
    mut commands: Commands,
    builders: Query<&BuilderTextCommandButton>,
) {
    let button_entity = trigger.entity;
    let Ok(builder) = builders.get(button_entity) else { return };

    let label = commands.spawn((
        Text::new(builder.text.clone()),
        builder.text_role.font(builder.font_size),
        TextColor::from(builder.text_color),
        TextLayout::no_wrap(),
    )).id();

    commands.entity(button_entity)
        .remove::<BuilderTextCommandButton>()
        .insert((
            TextCommandButton,
            TextCommandButtonChildren { label },
            builder.void_panel,
        ))
        .add_child(label);
}
