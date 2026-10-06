use bevy::{
    platform::collections::HashMap,
    prelude::*,
};
use strum::{EnumCount, IntoEnumIterator};

use alteration::{
    effects::prelude::EffectSourceOf,
    modifiers::prelude::ModifierType,
};
use game_core::prelude::{ContentId, Shard, ShardTier};
use logging::prelude::*;
use persistence::{prelude::*, rusqlite};
use resources::prelude::Stock;
use shards::{
    prelude::{RemovedSocket, ShardEffect, ShardSocket, ShardSocketOf, ShardSocketOperation, ShardSocketUpsert, ShardSockets, SocketedShard},
    sockets::ShardSocketOperationKind,
};
use states::prelude::MapLoadingStage;

pub(crate) struct ShardSocketsPlugin;
impl Plugin for ShardSocketsPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_observer(on_shard_socket_upsert_do_so)
            .add_observer(on_shard_socket_operation_do_so)
            .add_observer(on_insert_socketed_shard_spawn_effect)
            .add_observer(on_discard_socketed_shard_release)
            .add_systems(CollectSave, collect_shard_sockets)
            .register_loader(MapLoadingStage::SpawnEffectInstances, "shard_sockets", load_shard_sockets);
    }
}

/// Applies `ShardSocketUpsert`: finds the holder's socket by `ContentId` or creates it, empties it,
/// then applies the definition, removed state and shard.
#[log_tags(Tag::Shards)]
fn on_shard_socket_upsert_do_so(
    trigger: On<ShardSocketUpsert>,
    mut commands: Commands,
    mut stock: ResMut<Stock>,
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
            commands.entity(socket_entity).remove::<SocketedShard>();
            socket_entity
        }
        None => commands.spawn((upsert.content_id.clone(), ShardSocketOf(holder))).id(),
    };

    let mut socket_commands = commands.entity(socket_entity);
    socket_commands.insert(upsert.definition.clone());
    if upsert.removed {
        socket_commands.insert(RemovedSocket);
    } else {
        socket_commands.remove::<RemovedSocket>();
    }
    if let Some(shard) = upsert.shard {
        if upsert.removed || !upsert.definition.accepts(shard) {
            warn_dev!("Socket '{}' of {holder} does not accept {shard}, returned to stock", upsert.content_id.0);
            stock.add((shard, 1));
        } else {
            socket_commands.insert(SocketedShard(shard));
        }
    }
}

/// Applies `ShardSocketOperation`: socketing takes the shard from `Stock`; the shard leaving the
/// socket is handed over by `SocketedShard`'s discard observer.
#[log_tags(Tag::Shards)]
fn on_shard_socket_operation_do_so(
    trigger: On<ShardSocketOperation>,
    mut commands: Commands,
    mut stock: ResMut<Stock>,
    sockets: Query<(&ShardSocket, Option<&SocketedShard>, Has<RemovedSocket>)>,
) {
    let ShardSocketOperation { socket: socket_entity, kind } = *trigger;
    #[warn_dev("Entity {socket_entity} is not a shard socket, {kind:?} ignored")]
    let Ok((socket, socketed, removed)) = sockets.get(socket_entity) else { return; };

    match kind {
        ShardSocketOperationKind::Socket(shard) => {
            #[warn_dev("Socket '{}' is removed, refused {shard}", socket.description)]
            if removed { return; }
            #[warn_dev("Socket '{}' takes {} shards, refused {shard}", socket.description, socket.shard_type)]
            if !socket.accepts(shard) { return; }
            #[warn_dev("{shard} is not in stock, socket '{}' left as is", socket.description)]
            if !stock.try_remove((shard, 1)) { return; }
            info_player!("{shard} socketed into '{}'", socket.description);
            commands.entity(socket_entity).insert(SocketedShard(shard));
        }
        ShardSocketOperationKind::Unsocket => {
            #[warn_dev("Socket '{}' is already empty", socket.description)]
            let Some(&SocketedShard(released)) = socketed else { return; };
            info_player!("{released} unsocketed from '{}'", socket.description);
            commands.entity(socket_entity).remove::<SocketedShard>();
        }
    }
}

/// Spawns the effect of the socketed shard's tier on the holder, sourced from the socket.
fn on_insert_socketed_shard_spawn_effect(
    trigger: On<Insert, SocketedShard>,
    mut commands: Commands,
    sockets: Query<(&SocketedShard, &ShardSocket, &ShardSocketOf)>,
) {
    let socket_entity = trigger.entity;
    let Ok((&SocketedShard(shard), socket, &ShardSocketOf(holder))) = sockets.get(socket_entity) else { return; };
    commands.spawn(ShardEffect::from_modifiers(socket_entity, holder, socket.contributions_for(shard.tier).clone()));
}

/// Hands a shard leaving its socket (replaced, unsocketed or despawned with its holder) over to
/// `Stock`, then despawns its effect.
fn on_discard_socketed_shard_release(
    trigger: On<Discard, SocketedShard>,
    mut commands: Commands,
    mut stock: ResMut<Stock>,
    sockets: Query<&SocketedShard>,
) {
    let Ok(&SocketedShard(shard)) = sockets.get(trigger.entity) else { return; };
    stock.add((shard, 1));
    if trigger.trigger().new_archetype.is_none() { return; }
    commands.entity(trigger.entity).despawn_related::<EffectSourceOf>();
}

#[log_tags(Tag::GameSave)]
fn collect_shard_sockets(
    holders: Query<(Entity, &ShardSockets)>,
    sockets: Query<(&ContentId, &ShardSocket, Has<RemovedSocket>, Option<&SocketedShard>)>,
    mut save: SaveWriter,
) {
    struct SocketRow {
        holder_id: u32,
        content_id: String,
        socket: ShardSocket,
        removed: bool,
        socketed: Option<Shard>,
    }

    #[debug_dev("Saving {} shard sockets", rows.len())]
    let rows: Vec<SocketRow> = holders.iter()
        .flat_map(|(holder, holder_sockets)| holder_sockets.iter().map(move |socket_entity| (holder, socket_entity)))
        .filter_map(|(holder, socket_entity)| {
            let (content_id, socket, removed, socketed) = sockets.get(socket_entity).ok()?;
            Some(SocketRow {
                holder_id: holder.index_u32(),
                content_id: content_id.0.clone(),
                socket: socket.clone(),
                removed,
                socketed: socketed.map(|socketed| socketed.0),
            })
        })
        .collect();
    if rows.is_empty() { return; }
    save.submit(move |ctx| {
        for SocketRow { holder_id, content_id, socket, removed, socketed } in rows {
            ctx.tx.execute(
                "INSERT INTO shard_sockets (holder_id, content_id, shard_type, shard_tier, description, removed, socketed_shard_type, socketed_shard_tier) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                rusqlite::params![
                    holder_id,
                    content_id,
                    socket.shard_type.as_ref(),
                    socket.shard_tier.as_ref().map(ShardTier::as_ref),
                    socket.description,
                    removed,
                    socketed.as_ref().map(|shard| shard.shard_type.as_ref()),
                    socketed.as_ref().map(|shard| shard.tier.as_ref()),
                ],
            )?;
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
        "SELECT id, holder_id, content_id, shard_type, shard_tier, description, removed, socketed_shard_type, socketed_shard_tier FROM shard_sockets ORDER BY id",
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
                .with_removed(row.get(6)?)
                .with_shard(socketed);
            ctx.trigger(upsert);
            Ok(())
        },
    )
}
