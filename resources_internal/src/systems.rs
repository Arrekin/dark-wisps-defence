use bevy::prelude::*;
use strum::IntoEnumIterator;

use logging::prelude::*;
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
    let essences: Vec<(EssenceType, i32)> = EssenceType::iter()
        .map(|essence_type| (essence_type, stock.get(ResourceType::Essence(essence_type))))
        .collect();
    save.submit(move |tx| {
        tx.save_stock_resource("DarkOre", dark_ore)?;
        for (essence_type, amount) in essences {
            tx.save_stock_resource(essence_type.as_ref(), amount)?;
        }
        Ok(())
    });
}

#[log_tags(Tag::GameLoad)]
pub(crate) fn load_stock(ctx: &mut LoadContext) -> rusqlite::Result<()> {
    let mut stock = Stock::default();

    // Load DarkOre
    let dark_ore_amount = ctx.conn.get_stock_resource("DarkOre")
        .inspect_err(|error| warn_dev!("DarkOre stock not read from save ({error}); starting at 5555"))
        .unwrap_or(5555);
    stock.set(ResourceType::DarkOre, dark_ore_amount);
    // Load Essences
    for essence_type in EssenceType::iter() {
        let resource_key = essence_type.as_ref();
        let amount = ctx.conn.get_stock_resource(resource_key)
            .inspect_err(|error| warn_dev!("{resource_key} stock not read from save ({error}); starting at 0"))
            .unwrap_or(0);
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
