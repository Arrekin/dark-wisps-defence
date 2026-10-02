use bevy::prelude::*;
use bevy_egui::egui;

use game_core::prelude::{HasMoments, Moment, MomentOf, MomentOfInterest};
use narrative::prelude::ObjectiveDetails;
use wisps::summoning::Summoning;

/// Find the moment child of `parent` that has marker `T`.
pub(crate) fn find_moment_child<T: Component>(world: &World, parent: Entity) -> Option<Entity> {
    world
        .entity(parent)
        .get::<HasMoments>()
        .into_iter()
        .flat_map(|moments| moments.iter())
        .find(|&moment| world.entity(moment).contains::<T>())
}

/// Dropdown over all `With<Moment>` entities. Labels compose at render time:
/// parent's `id_name` (if the parent is an objective or summoning) + the
/// moment's `Name`. Standalone moments show just their `Name`.
///
/// `id_salt` must be unique per picker instance on screen (egui requires
/// distinct IDs for concurrent combo boxes).
pub(crate) fn ui_moment_picker(ui: &mut egui::Ui, world: &mut World, entity: Entity, id_salt: &str) {
    let moments: Vec<(Entity, String)> = {
        let mut query = world.query_filtered::<(Entity, &Name, Option<&MomentOf>), With<Moment>>();
        query
            .iter(world)
            .map(|(moment, name, moment_of)| {
                let label = match moment_of {
                    Some(moment_of) => {
                        let parent = world.entity(moment_of.0);
                        if let Some(details) = parent.get::<ObjectiveDetails>() {
                            format!("{}: {}", details.id_name, name.as_str())
                        } else if let Some(summoning) = parent.get::<Summoning>() {
                            format!("{}: {}", summoning.id_name, name.as_str())
                        } else {
                            name.as_str().to_string()
                        }
                    }
                    None => name.as_str().to_string(),
                };
                (moment, label)
            })
            .collect()
    };

    let current: Option<Entity> = world
        .entity(entity)
        .get::<MomentOfInterest>()
        .map(|moment_of_interest| moment_of_interest.0);

    ui.horizontal(|ui| {
        ui.label("Activated by:");
        let selected_text = current
            .and_then(|current| moments.iter().find(|(moment, _)| *moment == current).map(|(_, label)| label.clone()))
            .unwrap_or_else(|| "—".to_string());
        egui::ComboBox::from_id_salt(id_salt)
            .selected_text(selected_text)
            .show_ui(ui, |ui| {
                for (moment_entity, label) in &moments {
                    let is_selected = current == Some(*moment_entity);
                    if ui.selectable_label(is_selected, label).clicked() {
                        // Insert directly — Bevy replaces the existing relationship
                        // without firing `On<Remove>`, which would trigger the
                        // lost-activation observer and fail Inactive objectives.
                        world
                            .entity_mut(entity)
                            .insert(MomentOfInterest(*moment_entity));
                    }
                }
            });
    });
}
