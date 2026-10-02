use serde::{Deserialize, Serialize};

use logging::prelude::*;

const MAPS_DIRECTORY: &str = "maps";

/// Identity of a map in `maps/`: its file name without the `.dwd` extension. Unique, unlike
/// `MapInfo::name`, which is the display name stored inside the file.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MapFileName(String);
impl MapFileName {
    pub fn new(file_name: impl Into<String>) -> Self {
        Self(file_name.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn path(&self) -> String {
        format!("{MAPS_DIRECTORY}/{}.dwd", self.0)
    }
}

/// Every `.dwd` file in `maps/`.
#[log_tags(Tag::GameLoad)]
pub fn list_map_file_names() -> Vec<MapFileName> {
    std::fs::read_dir(MAPS_DIRECTORY)
        .inspect_err(|error| warn_dev!("Maps directory '{MAPS_DIRECTORY}' not readable ({error}); listing no maps"))
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(|entry| entry.ok()))
        .filter_map(|entry| {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|extension| extension.to_str()) == Some("dwd") {
                path.file_stem().and_then(|stem| stem.to_str()).map(MapFileName::new)
            } else {
                None
            }
        })
        .collect()
}
