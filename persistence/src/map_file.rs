//! # Map files
//!
//! Every place a game is saved to or loaded from is a [`MapFile`]. Its place decides the save
//! kind: a scenario in `maps/` resets playthrough state on save; the quicksave (`test_save.dwd`)
//! and named saves in `saves/` keep it. A [`MapFileName`] never carries a path, so a file cannot
//! land outside its folder.

use serde::{Deserialize, Serialize};

use logging::prelude::*;

const QUICKSAVE_PATH: &str = "test_save.dwd";
const SCENARIOS_DIRECTORY: &str = "maps";
const SAVES_DIRECTORY: &str = "saves";

/// A saved game's place on disk.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MapFile {
    /// `test_save.dwd`: playthrough save of the quicksave and quickload keybinds.
    Quick,
    /// `maps/<name>.dwd`: scenario.
    Scenario(MapFileName),
    /// `saves/<name>.dwd`: playthrough save.
    Save(MapFileName),
}
impl MapFile {
    /// The scenario `maps/<name>.dwd`. Panics on an invalid name; meant for names fixed in code.
    pub fn scenario(name: impl Into<String>) -> Self {
        Self::Scenario(MapFileName::try_new(name).expect("scenario names fixed in code are valid"))
    }

    /// The save `saves/<name>.dwd`. Panics on an invalid name; meant for names fixed in code.
    pub fn save(name: impl Into<String>) -> Self {
        Self::Save(MapFileName::try_new(name).expect("save names fixed in code are valid"))
    }

    pub fn path(&self) -> String {
        match self {
            Self::Quick => QUICKSAVE_PATH.to_string(),
            Self::Scenario(file_name) => format!("{SCENARIOS_DIRECTORY}/{}.dwd", file_name.as_str()),
            Self::Save(file_name) => format!("{SAVES_DIRECTORY}/{}.dwd", file_name.as_str()),
        }
    }

    pub fn exists(&self) -> bool {
        std::path::Path::new(&self.path()).exists()
    }

    /// Whether this file is a scenario in `maps/`; saving to it resets playthrough state.
    pub fn is_scenario(&self) -> bool {
        matches!(self, Self::Scenario(_))
    }

    /// Every scenario in `maps/`.
    pub fn list_scenarios() -> Vec<MapFile> {
        map_file_names_in(SCENARIOS_DIRECTORY).into_iter().map(Self::Scenario).collect()
    }

    /// Every save in `saves/`.
    pub fn list_saves() -> Vec<MapFile> {
        map_file_names_in(SAVES_DIRECTORY).into_iter().map(Self::Save).collect()
    }

    /// Every existing map file: scenarios, the quicksave and named saves.
    pub fn list_all() -> Vec<MapFile> {
        let quick = Self::Quick.exists().then_some(Self::Quick);
        Self::list_scenarios().into_iter().chain(quick).chain(Self::list_saves()).collect()
    }
}

/// A map file's name without the `.dwd` extension. Unlike `MapInfo::name`, the display name stored
/// inside the file, it is unique within its folder.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub struct MapFileName(String);
impl MapFileName {
    pub fn try_new(file_name: impl Into<String>) -> Result<Self, InvalidFileName> {
        let file_name = file_name.into();
        if file_name.is_empty() {
            return Err(InvalidFileName::Empty);
        }
        if let Some(character) = file_name.chars().find(|character| character.is_control() || r#"/\:*?"<>|"#.contains(*character)) {
            return Err(InvalidFileName::ForbiddenCharacter(character));
        }
        if is_reserved_device_name(&file_name) {
            return Err(InvalidFileName::ReservedName);
        }
        Ok(Self(file_name))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for MapFileName {
    type Error = InvalidFileName;
    fn try_from(file_name: String) -> Result<Self, Self::Error> {
        Self::try_new(file_name)
    }
}

/// Why a name cannot be a map file name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InvalidFileName {
    Empty,
    /// A path separator, or a character Windows does not allow in file names.
    ForbiddenCharacter(char),
    /// A Windows device name such as `CON` or `COM1`.
    ReservedName,
}
impl std::fmt::Display for InvalidFileName {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(formatter, "the name is empty"),
            Self::ForbiddenCharacter(character) => write!(formatter, "the name contains {character:?}"),
            Self::ReservedName => write!(formatter, "the name is reserved by Windows"),
        }
    }
}

/// Whether Windows reserves `file_name` for a device, with or without an extension.
fn is_reserved_device_name(file_name: &str) -> bool {
    let stem = file_name.split('.').next().unwrap_or_default().trim_end().to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit())
}

/// Names of every `.dwd` file in `directory`. A missing folder lists no names.
#[log_tags(Tag::GameLoad)]
fn map_file_names_in(directory: &str) -> Vec<MapFileName> {
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        #[warn_dev("Directory '{directory}' not readable ({error}); listing no files")]
        Err(error) => return Vec::new(),
    };
    entries
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if !path.is_file() || path.extension()? != "dwd" {
                return None;
            }
            MapFileName::try_new(path.file_stem()?.to_str()?).ok()
        })
        .collect()
}
