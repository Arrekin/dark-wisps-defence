//! Close-mark construction, hover, and material sync.

use bevy::prelude::*;

use widgets::prelude::{BuilderCloseButton, CloseButton, CloseButtonMaterial};

pub(crate) struct CloseButtonPlugin;
impl Plugin for CloseButtonPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins(UiMaterialPlugin::<CloseButtonMaterial>::default())
            .add_observer(on_builder_add_spawn_close_button)
            .add_systems(Update, sync_close_buttons);
    }
}

fn on_builder_add_spawn_close_button(
    trigger: On<Add, BuilderCloseButton>,
    mut commands: Commands,
    builders: Query<&BuilderCloseButton>,
    mut materials: ResMut<Assets<CloseButtonMaterial>>,
) {
    let entity = trigger.entity;
    let Ok(builder) = builders.get(entity) else { return };

    let button = builder.close_button;
    let material = materials.add(button.to_material());

    commands.entity(entity)
        .remove::<BuilderCloseButton>()
        .insert((MaterialNode(material), button))
        .observe(|trigger: On<Pointer<Over>>, mut buttons: Query<&mut CloseButton>| {
            if let Ok(mut button) = buttons.get_mut(trigger.entity) {
                button.set_hover(true);
            }
        })
        .observe(|trigger: On<Pointer<Out>>, mut buttons: Query<&mut CloseButton>| {
            if let Ok(mut button) = buttons.get_mut(trigger.entity) {
                button.set_hover(false);
            }
        });
}

fn sync_close_buttons(
    time: Res<Time>,
    mut buttons: Query<(&mut CloseButton, &MaterialNode<CloseButtonMaterial>), Changed<CloseButton>>,
    mut materials: ResMut<Assets<CloseButtonMaterial>>,
) {
    let now = time.elapsed_secs();
    for (mut button, material_handle) in buttons.iter_mut() {
        // Fade bookkeeping is derived state. Writing it through change detection would mark
        // the button changed again and requeue it here every frame, forever.
        let button = button.bypass_change_detection();
        button.begin_fades(now);

        let Some(mut material) = materials.get_mut(&material_handle.0) else { continue };
        *material = button.to_material();
    }
}
