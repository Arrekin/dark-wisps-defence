use std::str::FromStr;
use strum::{AsRefStr, EnumString};

use bevy::prelude::*;

use game_core::prelude::SSS;
use logging::prelude::{Log, Tag};
use persistence::prelude::{Saveable, Loadable, LoadContext, LoadResult, GameDbHelpers};
use persistence::rusqlite;

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
pub struct ObjectiveSaveData {
    pub entity: Entity,
    pub state: ObjectiveState,
    pub kill_wisps_data: Option<(usize, usize)>, // (target_amount, started_amount)
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
    pub objective_details: ObjectiveDetails,
    pub save_data: Option<ObjectiveSaveData>,
}
impl BuilderObjective {
    pub fn new(objective_details: ObjectiveDetails) -> Self {
        Self { objective_details, save_data: None }
    }
    pub fn new_for_saving(objective_details: ObjectiveDetails, save_data: ObjectiveSaveData) -> Self {
        Self { objective_details, save_data: Some(save_data) }
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
                    Log::error().dev().tag(Tag::GameLoad).message(format!("Unknown objective type '{objective_type_str}'"));
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
                Log::warn().dev().tag(Tag::GameLoad).message(format!("Objective with old ID {old_id} has no corresponding new entity"));
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

// ---- SPECIFIC OBJECTIVES ----

#[derive(Component, Default)]
pub struct ObjectiveClearAllQuantumFields {
    pub completed_quantum_fields: usize,
}

#[derive(Component, Default, Clone)]
pub struct ObjectiveKillWisps {
    pub target_amount: usize,
    pub started_amount: usize,
}
