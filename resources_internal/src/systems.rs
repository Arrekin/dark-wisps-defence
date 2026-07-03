use bevy::prelude::*;
use strum::IntoEnumIterator;

use persistence::{
    prelude::{GameDbHelpers, Loadable, LoadContext, LoadResult, SaveableBatchCommand},
    rusqlite,
};
use resources::{
    common::{EssenceType, ResourceType},
    stock::{Stock, StockChangedEvent},
};

pub(crate) fn save_stock_on_game_save(
    mut commands: Commands,
    stock: Res<Stock>,
) {
    commands.queue(SaveableBatchCommand::from_single(stock.clone()));
}

pub(crate) struct StockLoader;
impl Loadable for StockLoader {
    fn load(ctx: &mut LoadContext) -> rusqlite::Result<LoadResult> {
        let mut stock = Stock::default();

        // Load DarkOre
        let dark_ore_amount = ctx.conn.get_stock_resource("DarkOre").unwrap_or(5555);
        stock.set(ResourceType::DarkOre, dark_ore_amount);
        // Load Essences
        for essence_type in EssenceType::iter() {
            let resource_key = essence_type.as_ref();
            let amount = ctx.conn.get_stock_resource(&resource_key).unwrap_or(0);
            stock.set(ResourceType::Essence(essence_type), amount);
        }

        ctx.commands.insert_resource(stock);
        Ok(LoadResult::Finished)
    }
}

pub(crate) fn emit_delta_events_system(
    mut stock: ResMut<Stock>,
    mut event_writer: MessageWriter<StockChangedEvent>,
) {
    for (resource_type, delta) in stock.take_pending_deltas() {
        event_writer.write(StockChangedEvent { resource_type, delta, new_amount: stock.get(resource_type) });
    }
}
