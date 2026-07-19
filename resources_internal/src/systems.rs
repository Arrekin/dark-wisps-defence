use bevy::prelude::*;
use strum::IntoEnumIterator;

use persistence::{
    prelude::{GameDbHelpers, LoadContext, SaveWriter},
    rusqlite,
};
use resources::{
    common::{EssenceType, ResourceType},
    stock::{Stock, StockChangedEvent},
};

pub(crate) fn collect_stock(stock: Res<Stock>, mut save: SaveWriter) {
    let dark_ore = stock.get(ResourceType::DarkOre);
    let essences: Vec<(String, i32)> = EssenceType::iter()
        .map(|e| (e.as_ref().to_string(), stock.get(ResourceType::Essence(e))))
        .collect();
    save.submit(move |tx| {
        tx.save_stock_resource("DarkOre", dark_ore)?;
        for (resource_key, amount) in essences {
            tx.save_stock_resource(&resource_key, amount)?;
        }
        Ok(())
    });
}

pub(crate) fn load_stock(ctx: &mut LoadContext) -> rusqlite::Result<()> {
    let mut stock = Stock::default();

    // Load DarkOre
    let dark_ore_amount = ctx.conn.get_stock_resource("DarkOre").unwrap_or(5555);
    stock.set(ResourceType::DarkOre, dark_ore_amount);
    // Load Essences
    for essence_type in EssenceType::iter() {
        let resource_key = essence_type.as_ref();
        let amount = ctx.conn.get_stock_resource(resource_key).unwrap_or(0);
        stock.set(ResourceType::Essence(essence_type), amount);
    }

    ctx.insert_resource(stock);
    Ok(())
}

pub(crate) fn emit_delta_events_system(
    mut stock: ResMut<Stock>,
    mut event_writer: MessageWriter<StockChangedEvent>,
) {
    for (resource_type, delta) in stock.take_pending_deltas() {
        event_writer.write(StockChangedEvent { resource_type, delta, new_amount: stock.get(resource_type) });
    }
}
