use bevy::app::{App, Plugin};
use bevy::prelude::*;

use persistence::prelude::{AppGameLoadSaveExtension, CollectSave};
use resources::stock::{Stock, StockChangedEvent};
use states::prelude::MapLoadingStage;

pub(crate) mod systems;
use systems::{collect_stock, emit_delta_events_system, load_stock};

pub struct ResourcesPlugin;
impl Plugin for ResourcesPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<Stock>()
            .add_message::<StockChangedEvent>()
            .add_systems(PostUpdate, emit_delta_events_system.run_if(resource_changed::<Stock>))
            .add_systems(OnEnter(MapLoadingStage::Init), |mut commands: Commands| { commands.insert_resource(Stock::default()); })
            .add_systems(CollectSave, collect_stock)
            .register_loader(MapLoadingStage::LoadResources, "stock", load_stock)
            ;
    }
}
