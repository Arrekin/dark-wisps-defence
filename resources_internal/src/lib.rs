use bevy::app::{App, Plugin};
use bevy::prelude::*;

use persistence::prelude::AppGameLoadSaveExtension;
use resources::stock::{Stock, StockChangedEvent};
use states::prelude::MapLoadingStage;

pub(crate) mod systems;
use systems::{emit_delta_events_system, save_stock_on_game_save, StockLoader};

pub struct ResourcesPlugin;
impl Plugin for ResourcesPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<Stock>()
            .add_message::<StockChangedEvent>()
            .add_systems(PostUpdate, emit_delta_events_system.run_if(resource_changed::<Stock>))
            .add_systems(OnEnter(MapLoadingStage::Init), |mut commands: Commands| { commands.insert_resource(Stock::default()); })
            .register_db_loader::<StockLoader>(MapLoadingStage::LoadResources)
            .register_db_saver(save_stock_on_game_save);
    }
}
