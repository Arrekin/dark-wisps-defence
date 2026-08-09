//! Progress bar rendering: material asset creation and value sync.

use bevy::prelude::*;

use widgets::prelude::{BuilderProgressBar, ProgressBar, ProgressBarMaterial};

pub(crate) struct ProgressBarPlugin;
impl Plugin for ProgressBarPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins(UiMaterialPlugin::<ProgressBarMaterial>::default())
            .add_observer(on_builder_add_spawn_progress_bar)
            .add_systems(Update, sync_progress_bars);
    }
}

fn on_builder_add_spawn_progress_bar(
    trigger: On<Add, BuilderProgressBar>,
    mut commands: Commands,
    builders: Query<&BuilderProgressBar>,
    mut materials: ResMut<Assets<ProgressBarMaterial>>,
) {
    let entity = trigger.entity;
    let Ok(builder) = builders.get(entity) else { return };

    let bar = builder.progress_bar;
    let material = materials.add(bar.to_material());

    commands.entity(entity)
        .remove::<BuilderProgressBar>()
        .insert((MaterialNode(material), bar));
}

fn sync_progress_bars(
    time: Res<Time>,
    mut bars: Query<(&mut ProgressBar, &MaterialNode<ProgressBarMaterial>), Changed<ProgressBar>>,
    mut materials: ResMut<Assets<ProgressBarMaterial>>,
) {
    let now = time.elapsed_secs();
    for (mut bar, material_handle) in bars.iter_mut() {
        // Fade bookkeeping is derived state. Writing it through change detection would mark
        // the bar changed again and requeue it here every frame, forever.
        let bar = bar.bypass_change_detection();
        bar.begin_fades(now);

        let Some(mut material) = materials.get_mut(&material_handle.0) else { continue };
        *material = bar.to_material();
    }
}
