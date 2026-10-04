//! # Resource Catalog
//!
//! Registers the presentation (name, description, icon) of Dark Ore and every essence into the
//! [`Almanach`] at startup. Shards register their own presentation in the shard catalog.

use bevy::prelude::*;

use almanach::prelude::{AlmanachAppExt, ResourceInfo};
use resources::prelude::{EssenceType, ResourceType};

pub(crate) struct ResourceCatalogPlugin;
impl Plugin for ResourceCatalogPlugin {
    fn build(&self, app: &mut App) {
        let entries = [
            (ResourceType::DarkOre, "Dark Ore", "Mined from dark ore deposits. Pays for almost everything.", "indicators/no_dark_ore.png"),
            (ResourceType::Essence(EssenceType::Fire), "Fire Essence", "Left behind by defeated Fire wisps.", "ui/shards/shard_fire.png"),
            (ResourceType::Essence(EssenceType::Water), "Water Essence", "Left behind by defeated Water wisps.", "ui/shards/shard_water.png"),
            (ResourceType::Essence(EssenceType::Light), "Light Essence", "Left behind by defeated Light wisps.", "ui/shards/shard_light.png"),
            (ResourceType::Essence(EssenceType::Electric), "Electric Essence", "Left behind by defeated Electric wisps.", "ui/shards/shard_electric.png"),
        ];
        for (resource_type, name, description, icon_path) in entries {
            let icon: Handle<Image> = app.world().resource::<AssetServer>().load(icon_path);
            app.register_resource(resource_type, ResourceInfo {
                name: name.to_string(),
                description: description.to_string(),
                icon,
            });
        }
    }
}
