//! # Shard Sockets
//!
//! Applies socket definitions, contents, and states. State insertion updates query markers and
//! modifier effects; discarding a shard releases it to its configured destination. Upserts also
//! restore saved sockets after their holders have spawned.

use bevy::{
    platform::collections::HashMap,
    prelude::*,
};
use strum::{EnumCount, IntoEnumIterator};

use alteration::{
    effects::prelude::EffectSourceOf,
    modifiers::prelude::ModifierType,
};
use game_core::prelude::{ContentId, OptionalCommands, Shard, ShardTier};
use logging::prelude::*;
use persistence::{prelude::*, rusqlite};
use resources::prelude::Stock;
use shards::{
    effect::ShardEffect,
    orders::{AwaitsShardOrder, ShardOrder},
    prelude::{ActiveSocket, DisabledSocket, RemovedSocket, ShardSocket, ShardSocketOperation, ShardSocketState, ShardSocketUpsert, ShardSockets, SocketedShard},
    sockets::{ShardReleaseDestination, ShardSocketOf},
};
use states::{map_is_live, prelude::MapLoadingStage};

pub(crate) struct ShardSocketsPlugin;
impl Plugin for ShardSocketsPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_observer(on_shard_socket_upsert_do_so)
            .add_observer(on_shard_socket_operation_do_so.run_if(map_is_live))
            .add_observer(on_insert_socketed_shard_reapply_state)
            .add_observer(on_insert_shard_socket_state_apply)
            .add_observer(on_discard_socketed_shard_release)
            .add_systems(CollectSave, collect_shard_sockets)
            .register_loader(MapLoadingStage::SpawnEffectInstances, "shard_sockets", load_shard_sockets);
    }
}

/// Finds or creates a socket by holder and `ContentId`, then replaces its contents and definition.
fn on_shard_socket_upsert_do_so(
    trigger: On<ShardSocketUpsert>,
    mut commands: Commands,
    holders: Query<&ShardSockets>,
    sockets: Query<&ContentId, With<ShardSocket>>,
) {
    let upsert = trigger.event();
    let holder = upsert.holder;
    let existing = holders.get(holder).into_iter()
        .flat_map(|holder_sockets| holder_sockets.iter())
        .find(|&socket_entity| sockets.get(socket_entity).is_ok_and(|content_id| *content_id == upsert.content_id));
    let socket_entity = match existing {
        Some(socket_entity) => {
            commands.entity(socket_entity).remove::<(SocketedShard, AwaitsShardOrder)>();
            socket_entity
        }
        None => commands.spawn((upsert.content_id.clone(), ShardSocketOf(holder))).id(),
    };

    // Apply state last so its observer sees the new contents and definition.
    commands.entity(socket_entity)
        .insert_some(upsert.awaited_order.map(AwaitsShardOrder))
        .insert_some(upsert.shard.map(SocketedShard))
        .insert((upsert.definition.clone(), upsert.state));
}

/// Transfers shards and order links. The discard observer handles releasing replaced shards.
#[log_tags(Tag::Shards)]
fn on_shard_socket_operation_do_so(
    trigger: On<ShardSocketOperation>,
    mut commands: Commands,
    mut stock: ResMut<Stock>,
    sockets: Query<(&ShardSocket, Option<&SocketedShard>, Has<AwaitsShardOrder>, Has<RemovedSocket>, &ShardSocketOf)>,
    orders: Query<(), With<ShardOrder>>,
) {
    match *trigger.event() {
        ShardSocketOperation::Socket { shard, destination } => match destination {
            ShardReleaseDestination::Stock => stock.add((shard, 1)),
            ShardReleaseDestination::Void => {}
            ShardReleaseDestination::Socket(socket_entity) => {
                let accepting = sockets.get(socket_entity).ok().filter(|&(socket, _, _, removed, _)| !removed && socket.accepts(shard));
                #[warn_dev("Socket {socket_entity} refused {shard}, sent to stock")]
                let Some((socket, _, _, _, &ShardSocketOf(holder))) = accepting else {
                    stock.add((shard, 1));
                    return;
                };
                // Keep internal order transfers out of the player log.
                if orders.contains(holder) {
                    debug_dev!("{shard} socketed into order {holder}");
                } else {
                    info_player!("{shard} socketed into '{}'", socket.description);
                }
                commands.entity(socket_entity).remove::<AwaitsShardOrder>().insert(SocketedShard(shard));
            }
        },
        ShardSocketOperation::Unsocket { socket: socket_entity } => {
            #[warn_dev("Entity {socket_entity} is not a shard socket, unsocket ignored")]
            let Ok((socket, socketed, awaits_order, _, _)) = sockets.get(socket_entity) else { return; };
            #[warn_dev("Socket '{}' is already empty", socket.description)]
            if socketed.is_none() && !awaits_order { return; }
            if let Some(&SocketedShard(released)) = socketed {
                info_player!("{released} unsocketed from '{}'", socket.description);
            }
            if awaits_order {
                debug_dev!("Socket '{}' no longer awaits its order", socket.description);
            }
            commands.entity(socket_entity).remove::<(SocketedShard, AwaitsShardOrder)>();
        }
        ShardSocketOperation::AwaitOrder { socket: socket_entity, order } => {
            #[warn_dev("Cannot assign order {order}: entity {socket_entity} is not a shard socket")]
            let Ok((socket, _, _, removed, _)) = sockets.get(socket_entity) else { return; };
            #[warn_dev("Cannot assign order {order}: socket '{}' is removed", socket.description)]
            if removed { return; }
            debug_dev!("Socket '{}' awaits order {order}", socket.description);
            commands.entity(socket_entity).remove::<(SocketedShard, AwaitsShardOrder)>().insert(AwaitsShardOrder(order));
        }
    }
}

/// Reapplies the socket's state to its new shard.
fn on_insert_socketed_shard_reapply_state(
    trigger: On<Insert, SocketedShard>,
    mut commands: Commands,
    sockets: Query<&ShardSocketState>,
) {
    let Ok(&state) = sockets.get(trigger.entity) else { return; };
    // Fulfilling a root order can despawn this socket before the command runs.
    commands.entity(trigger.entity).try_insert(state);
}

/// Updates the query marker and contents: removed sockets empty, disabled sockets lose their
/// effects, and active sockets rebuild their shard's modifier effect.
fn on_insert_shard_socket_state_apply(
    trigger: On<Insert, ShardSocketState>,
    mut commands: Commands,
    sockets: Query<(&ShardSocketState, &ShardSocket, Option<&SocketedShard>, &ShardSocketOf)>,
) {
    let socket_entity = trigger.entity;
    let Ok((&state, socket, socketed, &ShardSocketOf(holder))) = sockets.get(socket_entity) else { return; };
    let mut socket_commands = commands.entity(socket_entity);
    socket_commands.remove::<(ActiveSocket, DisabledSocket, RemovedSocket)>();
    match state {
        ShardSocketState::Removed => { socket_commands.insert(RemovedSocket).remove::<(SocketedShard, AwaitsShardOrder)>(); }
        ShardSocketState::Disabled => { socket_commands.insert(DisabledSocket).despawn_related::<EffectSourceOf>(); }
        ShardSocketState::Active => {
            socket_commands.insert(ActiveSocket).despawn_related::<EffectSourceOf>();
            if let Some(&SocketedShard(shard)) = socketed {
                commands.spawn(ShardEffect::from_modifiers(socket_entity, holder, socket.contributions_for(shard.tier).clone()));
            }
        }
    }
}

/// Releases a discarded shard to the socket's configured destination.
/// Removes the effect explicitly when the socket survives.
fn on_discard_socketed_shard_release(
    trigger: On<Discard, SocketedShard>,
    mut commands: Commands,
    sockets: Query<(&SocketedShard, &ShardReleaseDestination)>,
) {
    let Ok((&SocketedShard(shard), &destination)) = sockets.get(trigger.entity) else { return; };
    commands.trigger(ShardSocketOperation::socket(destination, shard));
    // During holder despawn, EffectInstances already cascades to the effect.
    if trigger.trigger().new_archetype.is_none() { return; }
    commands.entity(trigger.entity).despawn_related::<EffectSourceOf>();
}

#[log_tags(Tag::GameSave)]
fn collect_shard_sockets(
    sockets: Query<(&ShardSocketOf, &ContentId, &ShardSocket, &ShardSocketState, Option<&SocketedShard>, Option<&AwaitsShardOrder>)>,
    mut save: SaveWriter,
) {
    struct SocketRow {
        holder_id: u32,
        content_id: String,
        socket: ShardSocket,
        state: ShardSocketState,
        socketed: Option<Shard>,
        awaited_order_id: Option<u32>,
    }

    #[debug_dev("Saving {} shard sockets", rows.len())]
    let rows: Vec<SocketRow> = sockets.iter()
        .map(|(&ShardSocketOf(holder), content_id, socket, &state, socketed, awaited_order)| SocketRow {
            holder_id: holder.index_u32(),
            content_id: content_id.0.clone(),
            socket: socket.clone(),
            state,
            socketed: socketed.map(|&SocketedShard(shard)| shard),
            awaited_order_id: awaited_order.map(|&AwaitsShardOrder(order)| order.index_u32()),
        })
        .collect();
    if rows.is_empty() { return; }
    save.submit(move |ctx| {
        for SocketRow { holder_id, content_id, socket, state, socketed, awaited_order_id } in rows {
            ctx.tx.prepare_cached("INSERT INTO shard_sockets (holder_id, content_id, shard_type, shard_tier, description, state, socketed_shard_type, socketed_shard_tier, awaited_order_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)")?
                .execute(rusqlite::params![
                    holder_id,
                    content_id,
                    socket.shard_type.as_ref(),
                    socket.shard_tier.as_ref().map(ShardTier::as_ref),
                    socket.description,
                    state.as_ref(),
                    socketed.as_ref().map(|shard| shard.shard_type.as_ref()),
                    socketed.as_ref().map(|shard| shard.tier.as_ref()),
                    awaited_order_id,
                ])?;
            let socket_row_id = ctx.tx.last_insert_rowid();
            for tier in ShardTier::iter() {
                for (modifier_type, value) in socket.contributions_for(tier) {
                    ctx.tx.prepare_cached("INSERT INTO shard_socket_contributions (socket_row_id, shard_tier, modifier_type, value) VALUES (?1, ?2, ?3, ?4)")?
                        .execute(rusqlite::params![socket_row_id, tier.as_ref(), modifier_type.as_ref(), value])?;
                }
            }
        }
        Ok(())
    });
}

/// Upserts every saved socket over the defaults its holder created, in saved order.
fn load_shard_sockets(ctx: &mut LoadContext) -> LoadResult {
    ctx.for_each_row(
        "SELECT id, holder_id, content_id, shard_type, shard_tier, description, state, socketed_shard_type, socketed_shard_tier, awaited_order_id FROM shard_sockets ORDER BY id",
        |ctx, row| {
            let socket_row_id: i64 = row.get(0)?;
            let holder = ctx.entity(row.get(1)?)?;
            let socketed = row.get_parsed_optional(7)?.zip(row.get_parsed_optional(8)?)
                .map(|(shard_type, tier)| Shard::new(shard_type, tier));

            let mut contributions: [HashMap<ModifierType, f32>; ShardTier::COUNT] = Default::default();
            let mut statement = ctx.conn.prepare_cached("SELECT shard_tier, modifier_type, value FROM shard_socket_contributions WHERE socket_row_id = ?1")?;
            let mut contribution_rows = statement.query([socket_row_id])?;
            while let Some(contribution_row) = contribution_rows.next()? {
                let tier: ShardTier = contribution_row.get_parsed(0)?;
                contributions[tier as usize].insert(contribution_row.get_parsed(1)?, contribution_row.get(2)?);
            }

            let definition = ShardSocket {
                shard_type: row.get_parsed(3)?,
                shard_tier: row.get_parsed_optional(4)?,
                description: row.get(5)?,
                contributions,
            };
            let upsert = ShardSocketUpsert::new(holder, ContentId(row.get(2)?), definition)
                .with_state(row.get_parsed(6)?)
                .with_shard(socketed)
                .with_awaited_order(ctx.optional_entity(row.get(9)?)?);
            ctx.trigger(upsert);
            Ok(())
        },
    )
}
