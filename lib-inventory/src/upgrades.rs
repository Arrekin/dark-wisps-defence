
use std::str::FromStr;

use crate::lib_prelude::*;

pub mod upgrades_prelude {
    pub use super::*;
}

pub struct UpgradesPlugin;
impl Plugin for UpgradesPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_message::<LevelUpUpgradeMessage>()
            .add_systems(PreUpdate,
                LevelUpUpgradeMessage::process.run_if(on_message::<LevelUpUpgradeMessage>),
            )
            ;
    }
}


////////////////////
////  UPGRADES  ////
////////////////////

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UpgradeType {
    Modifier(ModifierType)
}
impl UpgradeType {
    pub fn as_db_str(&self) -> String {
        match self {
            Self::Modifier(m) => format!("Modifier:{}", m.as_ref()),
        }
    }

    pub fn from_db_str(s: &str) -> Option<Self> {
        let (variant, inner) = s.split_once(':')?;
        match variant {
            "Modifier" => ModifierType::from_str(inner).ok().map(Self::Modifier),
            _ => None,
        }
    }
}

pub struct UpgradeRuntimeInfo {
    pub current_level: usize,
    pub static_info: UpgradeInfo,
}

#[derive(Component)]
pub struct Upgrades {
    pub upgrades: HashMap<UpgradeType, UpgradeRuntimeInfo>,
}
impl Upgrades {
    pub fn from_almanach(
        almanach_upgrades: &HashMap<UpgradeType, UpgradeInfo>,
        apply_levels: Option<&HashMap<UpgradeType, usize>>,
    ) -> Self {
        let upgrades = almanach_upgrades.iter().map(|(upgrade_type, info): (_, &UpgradeInfo)| {
            let level = apply_levels.and_then(|l| l.get(upgrade_type).copied()).unwrap_or(0);
            (*upgrade_type, UpgradeRuntimeInfo {
                current_level: level,
                static_info: info.clone(),
            })
        }).collect();
        Self { upgrades }
    }

    pub fn get_levels(&self) -> HashMap<UpgradeType, usize> {
        self.upgrades.iter()
            .map(|(upgrade_type, info)| (*upgrade_type, info.current_level))
            .collect()
    }

    pub fn total_upgrades_purchased(&self) -> usize {
        self.upgrades.values().map(|info| info.current_level).sum()
    }

    pub fn total_upgrades_available(&self) -> usize {
        self.upgrades.values().map(|info| info.static_info.levels.len()).sum()
    }
}

#[derive(Message)]
pub struct LevelUpUpgradeMessage {
    pub entity: Entity,
    pub upgrade_type: UpgradeType,
}
impl LevelUpUpgradeMessage {
    fn process(mut reader: MessageReader<Self>) {
        for _message in reader.read() {
            // Upgrades disabled — level-up requests are consumed but have no effect.
        }
    }
}
impl Command for LevelUpUpgradeMessage {
    fn apply(self, world: &mut World) {
        let mut messages = world.resource_mut::<Messages<Self>>();
        messages.write(self);
    }
}

/// Event triggered on an entity after an upgrade has been applied.
/// UI can observe this to refresh upgrade displays.
#[derive(EntityEvent)]
pub struct LevelUpUpgradeAppliedEvent {
    #[event_target]
    pub entity: Entity,
    pub upgrade_type: UpgradeType,
}
