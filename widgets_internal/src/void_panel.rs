//! Void-panel rendering: material asset creation and state sync.

use bevy::prelude::*;

use widgets::prelude::{BuilderVoidPanel, VoidPanel, VoidPanelMaterial};

pub(crate) struct VoidPanelPlugin;
impl Plugin for VoidPanelPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins(UiMaterialPlugin::<VoidPanelMaterial>::default())
            .add_observer(on_builder_add_spawn_void_panel)
            .add_systems(Update, sync_void_panels);
    }
}

// ============================================================================
// BUILDER — create material asset, insert MaterialNode + VoidPanel
// ============================================================================

fn on_builder_add_spawn_void_panel(
    trigger: On<Add, BuilderVoidPanel>,
    mut commands: Commands,
    builders: Query<&BuilderVoidPanel>,
    mut materials: ResMut<Assets<VoidPanelMaterial>>,
) {
    let entity = trigger.entity;
    let Ok(builder) = builders.get(entity) else { return };

    let panel = builder.void_panel;
    let material = materials.add(panel.to_material());

    commands.entity(entity)
        .remove::<BuilderVoidPanel>()
        .insert((MaterialNode(material), panel))
        .observe(move |trigger: On<Pointer<Over>>, mut panels: Query<&mut VoidPanel>| {
            if let Ok(mut panel) = panels.get_mut(trigger.entity) {
                panel.set_hover(true);
            }
        })
        .observe(move |trigger: On<Pointer<Out>>, mut panels: Query<&mut VoidPanel>| {
            if let Ok(mut panel) = panels.get_mut(trigger.entity) {
                panel.set_hover(false);
            }
        });
}

// ============================================================================
// SYNC — stamp fades on change, write to material asset
// ============================================================================

fn sync_void_panels(
    time: Res<Time>,
    mut panels: Query<(&mut VoidPanel, &MaterialNode<VoidPanelMaterial>), Changed<VoidPanel>>,
    mut materials: ResMut<Assets<VoidPanelMaterial>>,
) {
    let now = time.elapsed_secs();
    for (mut panel, material_handle) in panels.iter_mut() {
        // Fade bookkeeping is derived state. Writing it through change detection would
        // mark the panel changed again and requeue it here every frame, forever.
        let panel = panel.bypass_change_detection();
        panel.begin_fades(now);

        let Some(mut material) = materials.get_mut(&material_handle.0) else { continue };
        *material = panel.to_material();
    }
}
