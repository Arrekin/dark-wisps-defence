use std::str::FromStr;

use bevy::prelude::*;

use game_core::prelude::{DynamicGameEvent, MapBound};
use logging::prelude::*;
use map_objects::prelude::{QuantumField, Solved};
use narrative::prelude::*;
use persistence::{
    prelude::{AppGameLoadSaveExtension, CollectSave, GameDbHelpers, LoadContext, SaveWriter},
    rusqlite,
};
use session::StatsWispsKilled;
use states::prelude::{GameState, MapLoadingStage};

pub struct ObjectivesPlugin;
impl Plugin for ObjectivesPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(CollectSave, collect_objectives)
            .register_loader(MapLoadingStage::LoadResources, "objectives", load_objectives)
            .add_systems(Update, (
                (
                    update_clear_all_quantum_fields,
                    update_kill_wisps,
                ).run_if(in_state(GameState::Running)),
            ))
            .add_observer(on_builder_add_spawn_objective)
            .add_observer(on_objective_state_changed_update_appearance)
            .add_observer(reassess_inactive_objectives_on_dynamic_event)
            .add_observer(on_add_clear_all_quantum_fields_init_text)
            .add_observer(on_add_kill_wisps_init_text)
            ;
    }
}

fn collect_objectives(
    objectives: Query<(
        Entity,
        &ObjectiveDetails,
        &ObjectiveState,
        Option<&ObjectiveKillWisps>,
    )>,
    mut save: SaveWriter,
) {
    if objectives.is_empty() { return; }
    let rows: Vec<(i64, String, &'static str, String, String, Option<(i64, i64)>)> = objectives
        .iter()
        .map(|(entity, details, state, kill_wisps)| {
            let objective_type_str = match details.objective_type {
                ObjectiveType::ClearAllQuantumFields => "clear_quantum_fields",
                ObjectiveType::KillWisps(_) => "kill_wisps",
            };
            let kill_wisps_data = kill_wisps.map(|kw| (kw.target_amount as i64, kw.started_amount as i64));
            (
                entity.index_u32() as i64,
                details.id_name.clone(),
                objective_type_str,
                details.activation_event.clone(),
                state.as_ref().to_string(),
                kill_wisps_data,
            )
        })
        .collect();
    Log::debug().dev().tag(Tag::GameSave).message(format!("Saving {} objectives", rows.len()));
    save.submit(move |tx| {
        for (id, id_name, objective_type_str, activation_event, state_str, kill_wisps_data) in rows {
            tx.register_entity(id)?;
            tx.execute(
                "INSERT INTO objectives (id, id_name, objective_type, activation_event, state) VALUES (?1, ?2, ?3, ?4, ?5)",
                (id, &id_name, objective_type_str, &activation_event, &state_str),
            )?;
            if let Some((target_amount, started_amount)) = kill_wisps_data {
                tx.execute(
                    "INSERT INTO objective_kill_wisps (id, target_amount, started_amount) VALUES (?1, ?2, ?3)",
                    (id, target_amount, started_amount),
                )?;
            }
        }
        Ok(())
    });
}

fn load_objectives(ctx: &mut LoadContext) -> rusqlite::Result<()> {
    let mut stmt = ctx.conn.prepare("SELECT id, id_name, objective_type, activation_event, state FROM objectives")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let old_id: i64 = row.get(0)?;
        let id_name: String = row.get(1)?;
        let objective_type_str: String = row.get(2)?;
        let activation_event: String = row.get(3)?;
        let state_str: String = row.get(4)?;

        let state = ObjectiveState::from_str(state_str.as_str()).unwrap();

        // Load type-specific data
        let (objective_type, kill_wisps_data) = match objective_type_str.as_str() {
            "clear_quantum_fields" => {
                (ObjectiveType::ClearAllQuantumFields, None)
            }
            "kill_wisps" => {
                let mut kw_stmt = ctx.conn.prepare("SELECT target_amount, started_amount FROM objective_kill_wisps WHERE id = ?1")?;
                let mut kw_rows = kw_stmt.query([old_id])?;
                if let Some(kw_row) = kw_rows.next()? {
                    let target_amount: i64 = kw_row.get(0)?;
                    let started_amount: i64 = kw_row.get(1)?;
                    (ObjectiveType::KillWisps(target_amount as usize), Some((target_amount as usize, started_amount as usize)))
                } else {
                    (ObjectiveType::KillWisps(0), None)
                }
            }
            _ => {
                Log::error().dev().tag(Tag::GameLoad).message(format!("Unknown objective type '{objective_type_str}'"));
                continue;
            }
        };

        let Some(entity) = ctx.entity(old_id) else {
            Log::warn().dev().tag(Tag::GameLoad).message(format!("Objective with old ID {old_id} has no corresponding new entity"));
            continue;
        };
        let objective_details = ObjectiveDetails::new(id_name, objective_type, activation_event);
        let mut builder = BuilderObjective::new(objective_details)
            .with_state(state);
        if let Some((target_amount, started_amount)) = kill_wisps_data {
            builder = builder.with_kill_wisps_data(target_amount, started_amount);
        }
        ctx.insert(entity, builder);
    }
    Ok(())
}

fn on_builder_add_spawn_objective(
    trigger: On<Add, BuilderObjective>,
    mut commands: Commands,
    stats_wisps_killed: Res<StatsWispsKilled>,
    asset_server: Res<AssetServer>,
    builders: Query<&BuilderObjective>,
) {
    let entity = trigger.entity;
    let Ok(builder) = builders.get(entity) else { return; };

    // Create UI children
    let checkmark = commands.spawn((
        Node {
            width: Val::Px(16.),
            height: Val::Px(16.),
            left: Val::Px(2.),
            ..default()
        },
        ImageNode::new(asset_server.load("ui/objectives_check_active.png")),
        ObjectiveCheckmark,
    )).id();
    let text = commands.spawn((
        Text::new(builder.objective_details.id_name.clone()),
        TextFont::default().with_font_size(12.),
        ObjectiveText,
    )).id();

    let mut entity_commands = commands.entity(entity);
    entity_commands
        .remove::<BuilderObjective>()
        .insert((
            builder.objective_details.clone(),
            Objective { checkmark, text },
            MapBound,
            Node {
                width: Val::Percent(100.),
                border: UiRect::all(Val::Px(2.)),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(5.),
                border_radius: BorderRadius::all(Val::Px(7.)),
                ..default()
            },
            BackgroundColor::from(Color::linear_rgba(0.1, 0.3, 0.8, 0.7)),
            BorderColor::from(Color::linear_rgba(0., 0.2, 0.8, 0.9)),
        ))
        .add_children(&[checkmark, text]);

    if let Some(state) = &builder.state {
        // Apply saved state
        entity_commands.insert(state.clone());

        // Restore objective-specific components with saved data
        match builder.objective_details.objective_type {
            ObjectiveType::ClearAllQuantumFields => {
                entity_commands.insert(ObjectiveClearAllQuantumFields::default());
            }
            ObjectiveType::KillWisps(_) => {
                if let Some((target_amount, started_amount)) = builder.kill_wisps_data {
                    entity_commands.insert(ObjectiveKillWisps { target_amount, started_amount });
                }
            }
        }
    } else {
        // New objective (not from save) - insert default state and components
        entity_commands.insert(ObjectiveState::Inactive);
        match builder.objective_details.objective_type {
            ObjectiveType::ClearAllQuantumFields => {
                entity_commands.insert(ObjectiveClearAllQuantumFields::default());
            }
            ObjectiveType::KillWisps(target_amount) => {
                entity_commands.insert(ObjectiveKillWisps { target_amount, started_amount: stats_wisps_killed.0 });
            }
        }
    }
}

fn reassess_inactive_objectives_on_dynamic_event(
    trigger: On<DynamicGameEvent>,
    mut commands: Commands,
    objectives: Query<(Entity, &ObjectiveDetails, &ObjectiveState)>,
) {
    let event = &trigger.event().0;
    for (objective_entity, objective_details, state) in objectives.iter() {
        if !matches!(state, ObjectiveState::Inactive) { continue; }
        if event != &objective_details.activation_event { continue; }
        commands.entity(objective_entity).insert(ObjectiveState::InProgress);
        Log::info().player().tag(Tag::Objectives).message(format!("Objective activated: {}", objective_details.id_name));
    }
}

fn on_objective_state_changed_update_appearance(
    trigger: On<Insert, ObjectiveState>,
    asset_server: Res<AssetServer>,
    objectives: Query<(&Objective, &ObjectiveState)>,
    mut bg_colors: Query<&mut BackgroundColor>,
    mut border_colors: Query<&mut BorderColor>,
    mut checkmarks: Query<&mut ImageNode, With<ObjectiveCheckmark>>,
) {
    let entity = trigger.entity;
    let Ok((objective, state)) = objectives.get(entity) else { return; };

    let Ok(mut checkmark) = checkmarks.get_mut(objective.checkmark) else { return; };
    let Ok(mut bg) = bg_colors.get_mut(entity) else { return; };
    let Ok(mut border) = border_colors.get_mut(entity) else { return; };

    match state {
        ObjectiveState::Inactive => {
            checkmark.image = asset_server.load("ui/objectives_check_active.png");
            *bg = Color::linear_rgba(0.3, 0.3, 0.3, 0.7).into();
            *border = Color::linear_rgba(0.2, 0.2, 0.2, 0.9).into();
        }
        ObjectiveState::InProgress => {
            checkmark.image = asset_server.load("ui/objectives_check_active.png");
            *bg = Color::linear_rgba(0.1, 0.3, 0.8, 0.7).into();
            *border = Color::linear_rgba(0., 0.2, 0.8, 0.9).into();
        }
        ObjectiveState::Completed => {
            checkmark.image = asset_server.load("ui/objectives_check_completed.png");
            *bg = Color::linear_rgba(0.1, 0.8, 0.3, 0.7).into();
            *border = Color::linear_rgba(0., 0.8, 0.2, 0.9).into();
        }
        ObjectiveState::Failed => {
            checkmark.image = asset_server.load("ui/objectives_check_failed.png");
            *bg = Color::linear_rgba(0.8, 0.1, 0.3, 0.7).into();
            *border = Color::linear_rgba(0.8, 0., 0.2, 0.9).into();
        }
    }
}

fn on_add_clear_all_quantum_fields_init_text(
    trigger: On<Add, ObjectiveClearAllQuantumFields>,
    objectives: Query<&Objective>,
    mut texts: Query<&mut Text, With<ObjectiveText>>,
) {
    let entity = trigger.entity;
    let Ok(objective) = objectives.get(entity) else { return; };

    if let Ok(mut text) = texts.get_mut(objective.text) {
        text.0 = "Clear All Quantum Fields: 0/?".to_string();
    }
}
// TODO: make it trigger only on quantum fields change event
fn update_clear_all_quantum_fields(
    mut commands: Commands,
    mut objectives: Query<(Entity, &Objective, &mut ObjectiveClearAllQuantumFields, &ObjectiveState, &ObjectiveDetails)>,
    quantum_fields: Query<(), With<QuantumField>>,
    solved_fields: Query<(), (With<QuantumField>, With<Solved>)>,
    mut texts: Query<&mut Text, With<ObjectiveText>>,
) {
    for (objective_entity, objective, mut objective_clear_all_quantum_fields, state, details) in &mut objectives {
        if !matches!(state, ObjectiveState::InProgress) { continue; }

        let total = quantum_fields.iter().count();
        let completed = solved_fields.iter().count();
        objective_clear_all_quantum_fields.completed_quantum_fields = completed;

        let mut text = texts.get_mut(objective.text).unwrap();
        text.0 = format!("Clear All Quantum Fields: {}/{}", completed, total);

        if total > 0 && completed == total {
            Log::info().player().tag(Tag::Objectives).message(format!("Objective completed: {}", details.id_name));
            commands.entity(objective_entity).insert(ObjectiveState::Completed);
        }
    }
}

fn on_add_kill_wisps_init_text(
    trigger: On<Add, ObjectiveKillWisps>,
    objectives: Query<(&Objective, &ObjectiveKillWisps)>,
    mut texts: Query<&mut Text, With<ObjectiveText>>,
) {
    let entity = trigger.entity;
    let Ok((objective, objective_kill_wisps)) = objectives.get(entity) else { return; };

    if let Ok(mut text) = texts.get_mut(objective.text) {
        text.0 = format!("Kill Wisps: 0/{}", objective_kill_wisps.target_amount);
    }
}
fn update_kill_wisps(
    mut commands: Commands,
    stats_wisps_killed: Res<StatsWispsKilled>,
    mut objectives: Query<(Entity, &Objective, &ObjectiveKillWisps, &ObjectiveState, &ObjectiveDetails)>,
    mut texts: Query<&mut Text, With<ObjectiveText>>,
) {
    for (objective_entity, objective, objective_kill_wisps, state, details) in &mut objectives {
        if !matches!(state, ObjectiveState::InProgress) { continue; }

        let current_amount = std::cmp::min(stats_wisps_killed.0 - objective_kill_wisps.started_amount, objective_kill_wisps.target_amount);
        let mut text = texts.get_mut(objective.text).unwrap();
        text.0 = format!("Kill Wisps: {}/{}", current_amount, objective_kill_wisps.target_amount);

        if current_amount == objective_kill_wisps.target_amount {
            Log::info().player().tag(Tag::Objectives).message(format!("Objective completed: {}", details.id_name));
            commands.entity(objective_entity).insert(ObjectiveState::Completed);
        }

    }
}
