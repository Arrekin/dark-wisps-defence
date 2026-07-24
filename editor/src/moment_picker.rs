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
        .flat_map(|h| h.iter())
        .find(|&c| world.entity(c).contains::<T>())
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
            .map(|(e, name, parent_rel)| {
                let label = match parent_rel {
                    Some(rel) => {
                        let parent = world.entity(rel.0);
                        if let Some(det) = parent.get::<ObjectiveDetails>() {
                            format!("{}: {}", det.id_name, name.as_str())
                        } else if let Some(s) = parent.get::<Summoning>() {
                            format!("{}: {}", s.id_name, name.as_str())
                        } else {
                            name.as_str().to_string()
                        }
                    }
                    None => name.as_str().to_string(),
                };
                (e, label)
            })
            .collect()
    };

    let current: Option<Entity> = world
        .entity(entity)
        .get::<MomentOfInterest>()
        .map(|a| a.0);

    ui.horizontal(|ui| {
        ui.label("Activated by:");
        let selected_text = current
            .and_then(|e| moments.iter().find(|(te, _)| *te == e).map(|(_, n)| n.clone()))
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
