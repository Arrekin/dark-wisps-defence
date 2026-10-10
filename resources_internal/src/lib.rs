use bevy::prelude::*;

use persistence::prelude::{AppGameLoadSaveExtension, CollectSave};
use resources::stock::{Stock, StockChangedMessage};
use states::prelude::MapLoadingStage;

pub(crate) mod resource_catalog;
pub(crate) mod systems;
use systems::{collect_stock, emit_stock_changes, load_stock};

pub struct ResourcesPlugin;
impl Plugin for ResourcesPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins(resource_catalog::ResourceCatalogPlugin)
            .init_resource::<Stock>()
            .add_message::<StockChangedMessage>()
            .add_systems(PostUpdate, emit_stock_changes.run_if(resource_changed::<Stock>))
            .add_systems(OnEnter(MapLoadingStage::Init), |mut commands: Commands| { commands.insert_resource(Stock::default()); })
            .add_systems(CollectSave, collect_stock)
            .register_loader(MapLoadingStage::LoadResources, "stock", load_stock);
    }
}
