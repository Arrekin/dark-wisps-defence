use std::str::FromStr;
use strum::{AsRefStr, EnumString};

use lib_core::map_objects::QuantumField;
use lib_inventory::stats::StatsWispsKilled;

use crate::map_objects::quantum_field::Solved;
use crate::prelude::*;

pub struct ObjectivesPlugin;
impl Plugin for ObjectivesPlugin {
    fn build(&self, app: &mut App) {
        app
            .register_db_loader::<BuilderObjective>(MapLoadingStage::LoadResources)
            .register_db_saver(BuilderObjective::on_game_save)
            .add_systems(Update, (
                (
                    ObjectiveClearAllQuantumFields::update,
                    ObjectiveKillWisps::update,
                ).run_if(in_state(GameState::Running)),
            ))
            .add_observer(BuilderObjective::on_add)
            .add_observer(Objective::on_objective_state_changed)
            .add_observer(Objective::reassess_inactive_objectives_on_dynamic_event)
            .add_observer(ObjectiveClearAllQuantumFields::on_add)
            .add_observer(ObjectiveKillWisps::on_add)
            ;
    }
}

#[derive(Copy, Clone, Debug)]
pub enum ObjectiveType {
    ClearAllQuantumFields,
    // TODO: Get rid of this param once legacy load/save is removed
    KillWisps(usize),
}

#[derive(Component, Clone, Debug)]
pub struct ObjectiveDetails {
    pub id_name: String,
    pub objective_type: ObjectiveType,
    pub activation_event: String,
}
impl ObjectiveDetails {
    pub fn new(id_name: String, objective_type: ObjectiveType, activation_event: String) -> Self {
        Self { id_name, objective_type, activation_event }
    }
}

#[derive(Clone, Debug)]
struct ObjectiveSaveData {
    entity: Entity,
    state: ObjectiveState,
    kill_wisps_data: Option<(usize, usize)>, // (target_amount, started_amount)
}

#[derive(Component, Clone, Debug, EnumString, AsRefStr)]
pub enum ObjectiveState {
    Inactive,
    InProgress,
    Completed,
    Failed,
}

#[derive(Component, SSS)]
pub struct BuilderObjective {
    objective_details: ObjectiveDetails,
    save_data: Option<ObjectiveSaveData>,
}
impl BuilderObjective {
    pub fn new(objective_details: ObjectiveDetails) -> Self {
        Self { objective_details, save_data: None }
    }
    fn new_for_saving(objective_details: ObjectiveDetails, save_data: ObjectiveSaveData) -> Self {
        Self { objective_details, save_data: Some(save_data) }
    }

    fn on_game_save(
        mut commands: Commands,
        objectives: Query<(
            Entity,
            &ObjectiveDetails,
            &ObjectiveState,
            Option<&ObjectiveKillWisps>,
        )>,
    ) {
        if objectives.is_empty() { return; }
        let batch = objectives.iter().map(|(entity, details, state, kill_wisps)| {
            let kill_wisps_data = kill_wisps.map(|kw| (kw.target_amount, kw.started_amount));

            let save_data = ObjectiveSaveData {
                entity,
                state: state.clone(),
                kill_wisps_data,
            };
            BuilderObjective::new_for_saving(details.clone(), save_data)
        }).collect::<SaveableBatchCommand<_>>();
        println!("Creating batch of BuilderObjective for saving. {} items", batch.len());
        commands.queue(batch);
    }

    pub fn on_add(
        trigger: On<Add, BuilderObjective>,
        mut commands: Commands,
        builders: Query<&BuilderObjective>,
        stats_wisps_killed: Res<StatsWispsKilled>,
        asset_server: Res<AssetServer>,
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

        if let Some(save_data) = &builder.save_data {
            // Apply saved state
            entity_commands.insert(save_data.state.clone());

            // Restore objective-specific components with saved data
            match builder.objective_details.objective_type {
                ObjectiveType::ClearAllQuantumFields => {
                    entity_commands.insert(ObjectiveClearAllQuantumFields::default());
                }
                ObjectiveType::KillWisps(_) => {
                    if let Some((target_amount, started_amount)) = save_data.kill_wisps_data {
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
}
impl Saveable for BuilderObjective {
    fn save(self, tx: &rusqlite::Transaction) -> rusqlite::Result<()> {
        let save_data = self.save_data.expect("BuilderObjective for saving must have save_data");
        let entity_index = save_data.entity.index_u32() as i64;

        let objective_type_str = match self.objective_details.objective_type {
            ObjectiveType::ClearAllQuantumFields => "clear_quantum_fields",
            ObjectiveType::KillWisps(_) => "kill_wisps",
        };

        // Save objective to DB
        tx.register_entity(entity_index)?;
        tx.execute(
            "INSERT INTO objectives (id, id_name, objective_type, activation_event, state) VALUES (?1, ?2, ?3, ?4, ?5)",
            (entity_index, &self.objective_details.id_name, objective_type_str, &self.objective_details.activation_event, save_data.state.as_ref()),
        )?;

        // Save type-specific data
        match self.objective_details.objective_type {
            ObjectiveType::ClearAllQuantumFields => {
                // No additional data to save
            }
            ObjectiveType::KillWisps(_) => {
                if let Some((target_amount, started_amount)) = save_data.kill_wisps_data {
                    tx.execute(
                        "INSERT INTO objective_kill_wisps (id, target_amount, started_amount) VALUES (?1, ?2, ?3)",
                        (entity_index, target_amount as i64, started_amount as i64),
                    )?;
                }
            }
        }

        Ok(())
    }
}
impl Loadable for BuilderObjective {
    fn load(ctx: &mut LoadContext) -> rusqlite::Result<LoadResult> {
        let mut stmt = ctx.conn.prepare("SELECT id, id_name, objective_type, activation_event, state FROM objectives LIMIT ?1 OFFSET ?2")?;
        let mut rows = stmt.query(ctx.pagination.as_params())?;

        let mut count = 0;
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
                    eprintln!("Unknown objective type: {}", objective_type_str);
                    continue;
                }
            };

            if let Some(new_entity) = ctx.get_new_entity_for_old(old_id) {
                let objective_details = ObjectiveDetails::new(id_name, objective_type, activation_event);
                let save_data = ObjectiveSaveData {
                    entity: new_entity,
                    state,
                    kill_wisps_data,
                };
                ctx.commands.entity(new_entity).insert(BuilderObjective::new_for_saving(objective_details, save_data));
            } else {
                eprintln!("Warning: Objective with old ID {} has no corresponding new entity", old_id);
            }
            count += 1;
        }

        Ok(count.into())
    }
}

#[derive(Component)]
pub struct ObjectiveCheckmark;
#[derive(Component)]
pub struct ObjectiveText;


#[derive(Component)]
pub struct Objective {
    pub checkmark: Entity,
    pub text: Entity,
}
impl Objective {
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
        }
    }
    fn on_objective_state_changed(
        trigger: On<Insert, ObjectiveState>,
        objectives: Query<(&Objective, &ObjectiveState)>,
        mut bg_colors: Query<&mut BackgroundColor>,
        mut border_colors: Query<&mut BorderColor>,
        mut checkmarks: Query<&mut ImageNode, With<ObjectiveCheckmark>>,
        asset_server: Res<AssetServer>,
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
}

// ---- SPECIFIC OBJECTIVES ----

#[derive(Component, Default)]
pub struct ObjectiveClearAllQuantumFields {
    completed_quantum_fields: usize,
}
impl ObjectiveClearAllQuantumFields {
    fn on_add(
        trigger: On<Add, ObjectiveClearAllQuantumFields>,
        objectives: Query<&Objective>,
        mut texts: Query<&mut Text, With<ObjectiveText>>,
    ) {
        let entity = trigger.entity;
        let Ok(objective) = objectives.get(entity) else { return; };
        
        if let Ok(mut text) = texts.get_mut(objective.text) {
            text.0 = format!("Clear All Quantum Fields: 0/?");
        }
    }
    // TODO: make it trigger only on quantum fields change event
    fn update(
        mut commands: Commands,
        mut objectives: Query<(Entity, &Objective, &mut ObjectiveClearAllQuantumFields, &ObjectiveState)>,
        quantum_fields: Query<(), With<QuantumField>>,
        solved_fields: Query<(), (With<QuantumField>, With<Solved>)>,
        mut texts: Query<&mut Text, With<ObjectiveText>>,
    ) {
        for (objective_entity, objective, mut objective_clear_all_quantum_fields, state) in &mut objectives {
            if !matches!(state, ObjectiveState::InProgress) { continue; }

            let total = quantum_fields.iter().count();
            let completed = solved_fields.iter().count();
            objective_clear_all_quantum_fields.completed_quantum_fields = completed;

            let mut text = texts.get_mut(objective.text).unwrap();
            text.0 = format!("Clear All Quantum Fields: {}/{}", completed, total);

            if total > 0 && completed == total {
                commands.entity(objective_entity).insert(ObjectiveState::Completed);
            }
        }
    }
}

#[derive(Component, Default, Clone)]
pub struct ObjectiveKillWisps {
    pub target_amount: usize,
    pub started_amount: usize,
}
impl ObjectiveKillWisps {
    fn on_add(
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
    fn update(
        mut commands: Commands,
        stats_wisps_killed: Res<StatsWispsKilled>,
        mut objectives: Query<(Entity, &Objective, &ObjectiveKillWisps, &ObjectiveState)>,
        mut texts: Query<&mut Text, With<ObjectiveText>>,
    ) {
        for (objective_entity, objective, objective_kill_wisps, state)  in &mut objectives {
            if !matches!(state, ObjectiveState::InProgress) { continue; }
            
            let current_amount = std::cmp::min(stats_wisps_killed.0 - objective_kill_wisps.started_amount, objective_kill_wisps.target_amount);
            let mut text = texts.get_mut(objective.text).unwrap();
            text.0 = format!("Kill Wisps: {}/{}", current_amount, objective_kill_wisps.target_amount);

            if current_amount == objective_kill_wisps.target_amount {
                commands.entity(objective_entity).insert(ObjectiveState::Completed);
            }

        }
    }
}