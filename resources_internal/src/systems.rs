use bevy::prelude::*;

use persistence::{prelude::*, rusqlite};
use resources::{
    common::ResourceAmount,
    stock::{Stock, StockChangedMessage},
};

pub(crate) fn collect_stock(stock: Res<Stock>, mut save: SaveWriter) {
    let entries: Vec<ResourceAmount> = stock.iter().collect();
    save.submit(move |ctx| {
        let list_id = ctx.save_resource_list(&entries)?;
        ctx.tx.execute("INSERT INTO stock (id, list_id) VALUES (1, ?1)", rusqlite::params![list_id])?;
        Ok(())
    });
}

/// Starts from `Stock::default` and overrides every saved entry; resources absent from the save
/// keep their default amount.
pub(crate) fn load_stock(ctx: &mut LoadContext) -> LoadResult {
    let list_id: i64 = ctx.conn
        .query_row("SELECT list_id FROM stock WHERE id = 1", [], |row| row.get(0))
        .map_err(LoadError::table_read("stock"))?;
    let mut stock = Stock::default();
    for entry in ctx.resource_list(list_id)? {
        stock.set(entry);
    }
    ctx.insert_resource(stock);
    Ok(())
}

pub(crate) fn emit_stock_changed_messages(
    mut stock: ResMut<Stock>,
    mut stock_changed_messages: MessageWriter<StockChangedMessage>,
) {
    // Draining deltas is bookkeeping, not a stock change; flagging it would re-run this system every frame.
    let stock = stock.bypass_change_detection();
    for delta in stock.take_pending_deltas() {
        stock_changed_messages.write(StockChangedMessage {
            resource_type: delta.resource_type,
            delta: delta.amount,
            new_amount: stock.get(delta.resource_type),
        });
    }
}
