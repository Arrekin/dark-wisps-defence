use bevy::prelude::*;

use almanach::prelude::Almanach;
use game_core::prelude::{BuildingType, Shard, ShardTier, ShardType};
use logging::prelude::*;
use persistence::{prelude::*, rusqlite};
use resources::prelude::Stock;
use shards::{
    prelude::{ShardEffect, ShardSlots, ShardSocketOperation},
    slots::{ShardSocketOperationKind, SocketedShard},
};
use states::prelude::MapLoadingStage;

pub(crate) struct ShardSlotsPlugin;
impl Plugin for ShardSlotsPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_observer(on_shard_socket_operation_do_so)
            .add_systems(CollectSave, collect_shard_slots)
            .register_loader(MapLoadingStage::SpawnEffectInstances, "entity_shards", load_shard_slots);
    }
}

/// Sockets a shard, spawning the effect its socket defines for the shard's tier, or empties the slot.
/// The socket comes from the target's `BuildingInfo`. Socketing takes the shard out of `Stock` unless
/// it is restored from a save; the shard the slot held goes back to `Stock` and its effect is removed.
#[log_tags(Tag::Shards)]
fn on_shard_socket_operation_do_so(
    trigger: On<ShardSocketOperation>,
    mut commands: Commands,
    almanach: Res<Almanach>,
    mut stock: ResMut<Stock>,
    mut shard_targets: Query<(&BuildingType, &mut ShardSlots)>,
) {
    let ShardSocketOperation { shard_target, slot_index, kind } = *trigger;
    #[warn_dev("Entity {shard_target} has no ShardSlots for {kind:?}")]
    let Ok((building_type, mut slots)) = shard_targets.get_mut(shard_target) else { return; };
    let building_info = almanach.get_building_info(*building_type);
    #[warn_dev("{building_type:?} has no socket {slot_index} for {kind:?}")]
    let Some(socket) = building_info.sockets.get(slot_index) else { return; };

    let released = match kind {
        ShardSocketOperationKind::Socket(shard) | ShardSocketOperationKind::Restore(shard) => {
            #[warn_dev("Socket {slot_index} of {building_type:?} takes {} shards, refused {shard}", socket.shard_type)]
            if !socket.accepts(shard) { return; }
            if matches!(kind, ShardSocketOperationKind::Socket(_)) {
                #[warn_dev("{shard} is not in stock, socket {slot_index} of {building_type:?} left as is")]
                if !stock.try_remove((shard, 1)) { return; }
                info_player!("{shard} socketed into '{}'", building_info.name);
            }
            let effect = commands.spawn(ShardEffect::from_modifiers(shard_target, socket.contributions_for(shard.tier).clone())).id();
            slots.socket(slot_index, SocketedShard { shard, effect })
        }
        ShardSocketOperationKind::Unsocket => {
            let released = slots.unsocket(slot_index);
            match released {
                Some(released) => info_player!("{} unsocketed from '{}'", released.shard, building_info.name),
                None => warn_dev!("Slot {slot_index} of {building_type:?} is already empty"),
            }
            released
        }
    };
    if let Some(released) = released {
        commands.entity(released.effect).try_despawn();
        stock.add((released.shard, 1));
    }
}

#[log_tags(Tag::GameSave)]
fn collect_shard_slots(
    shard_targets: Query<(Entity, &ShardSlots)>,
    mut save: SaveWriter,
) {
    #[debug_dev("Saving {} shard slots", rows.len())]
    let rows: Vec<(u32, usize, Shard)> = shard_targets.iter()
        .flat_map(|(entity, slots)| {
            slots.iter().enumerate().filter_map(move |(slot_index, shard)| {
                shard.map(|shard| (entity.index_u32(), slot_index, shard))
            })
        })
        .collect();
    if rows.is_empty() { return; }
    save.submit(move |ctx| {
        for (entity_id, slot_index, shard) in rows {
            ctx.register_entity(entity_id)?;
            ctx.tx.execute(
                "INSERT INTO entity_shards (shard_target_id, shard_index, shard_type, shard_tier) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![entity_id, slot_index, shard.shard_type.as_ref(), shard.tier.as_ref()],
            )?;
        }
        Ok(())
    });
}

fn load_shard_slots(ctx: &mut LoadContext) -> LoadResult {
    ctx.for_each_entity("SELECT shard_target_id, shard_index, shard_type, shard_tier FROM entity_shards ORDER BY shard_target_id, shard_index", |ctx, _, entity, row| {
        let slot_index: usize = row.get(1)?;
        let shard = Shard::new(row.get_parsed::<ShardType>(2)?, row.get_parsed::<ShardTier>(3)?);
        ctx.trigger(ShardSocketOperation::restore(entity, slot_index, shard));
        Ok(())
    })
}
