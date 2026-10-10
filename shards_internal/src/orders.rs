//! # Shard Orders
//!
//! Coordinates ingredient gathering, Forge assignment, cancellation, and delivery.
//!
//! ## Gathering
//! Builders create an empty order socket and mark the order for resolution. Resolution waits until
//! map loading finishes so it can use restored sockets, ingredients, and progress. Missing ingredients
//! are created one level per pass. Ingredient orders first try to reserve their shard from `Stock`;
//! root orders always forge a new shard. Waiting ingredients can also reserve newly acquired stock.
//! An order that loses its Forge gathers ingredients again because the previous job consumed them.
//!
//! ## Pickup
//! Each idle, operational Forge takes the first ready order it can afford, checking its Forge queue
//! before the global queue. `WorksGlobalQueue` controls access to the global queue. Traversal visits
//! ingredients before their parents and skips blocked orders. An order is ready once resolution has
//! finished and every required ingredient order holds its shard.
//!
//! Starting a job consumes its ingredients and pickup cost without refunds. The root order keeps
//! its queue position until completion. Progress pauses while the Forge is not operational.
//!
//! ## Cancellation
//! Hard cancellation despawns the order and its ingredients, discarding active jobs. Soft cancellation
//! first detaches active ingredient jobs as root orders in the same queue, then despawns the cancelled
//! order unless it is itself in progress. Detached jobs finish into `Stock`; shards held by despawned
//! ingredient orders also return to `Stock`.
//!
//! ## Fulfillment
//! Inserting a shard into an order's socket fulfills it, whether forged or reserved from `Stock`.
//! Unneeded ingredient orders are soft-cancelled. A fulfilled ingredient keeps its shard until its
//! parent starts forging. A fulfilled root despawns, releasing its shard to the destination socket
//! recorded by `ShardOrderDestination`, or to `Stock` if it has no destination.

use std::{iter::Copied, slice::Iter};

use bevy::{ecs::system::SystemParam, prelude::*};
use strum::EnumCount;

use almanach::prelude::*;
use buildings::Forge;
use game_core::prelude::{ContentId, GridCoords, IsOperational, Shard, ShardTier};
use logging::prelude::*;
use persistence::{creating_new_map, prelude::*, rusqlite};
use resources::{prelude::{ResourceType, Stock}, stock::ShardStockAcquired};
use shards::{
    orders::{
        BuilderShardOrder, ForgeCurrentOrder, ForgedBy, ForgingProgress, ForgingQueue, GlobalForgingQueue, InForgingQueue, IngredientOf,
        Ingredients, OrderLink, ShardOrder, ShardOrderCancelMode, ShardOrderCancelRequest, ShardOrderDestination, WorksGlobalQueue,
    },
    prelude::*,
    sockets::{ShardReleaseDestination, ShardSocketOf},
};
use states::{map_is_live, prelude::{GameState, MapLoadingStage}};

pub(crate) struct ShardOrdersPlugin;
impl Plugin for ShardOrdersPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Update, (
                resolve_orders.run_if(map_is_live.and_then(any_with_component::<OrderNeedsResolution>)),
                (assign_orders_to_free_forges, advance_orders_in_progress).run_if(in_state(GameState::Running)),
            ).chain())
            .add_systems(OnEnter(MapLoadingStage::SpawnMapElements), seed_global_forging_queue.run_if(creating_new_map))
            .add_observer(on_shard_order_cancel_request_do_so)
            .add_observer(on_builder_add_spawn_shard_order)
            .add_observer(on_remove_forged_by_reset_order.run_if(map_is_live))
            .add_observer(on_remove_shard_order_destination_soft_cancel.run_if(map_is_live))
            .add_observer(on_insert_shard_order_destination_release_to_it)
            .add_observer(on_shard_stock_acquired_reserve_late_shards.run_if(map_is_live))
            .add_observer(on_insert_socketed_shard_fulfill_order)
            .add_systems(CollectSave, (collect_global_forging_queue, collect_shard_orders, collect_forge_queues))
            .register_loader(MapLoadingStage::SpawnMapElements, "global_forging_queue", load_global_forging_queue)
            .register_loader(MapLoadingStage::SpawnMapElements, "shard_orders", load_shard_orders)
            .register_loader(MapLoadingStage::SpawnEffectInstances, "forge_queue", load_forge_queues);
    }
}

/// Identifies an order's output socket.
const ORDER_SOCKET_ID: &str = "order";
const ORDER_SOCKET_DESCRIPTION: &str = "Forged shard";

fn seed_global_forging_queue(mut commands: Commands) {
    commands.spawn(GlobalForgingQueue);
}

// ============================================================================
// ORDER TREE
// ============================================================================

/// Reads an order, its socket and its tree of ingredient orders, and soft-cancels parts of it.
#[derive(SystemParam)]
pub(crate) struct OrderTreeParam<'w, 's> {
    pub almanach: Res<'w, Almanach>,
    pub orders: Query<'w, 's, &'static ShardOrder>,
    pub ingredients: Query<'w, 's, &'static Ingredients>,
    pub parents: Query<'w, 's, &'static IngredientOf>,
    pub queue_links: Query<'w, 's, &'static InForgingQueue>,
    pub in_progress: Query<'w, 's, (), With<ForgedBy>>,
    pub socket_holders: Query<'w, 's, &'static ShardSockets>,
    pub socketed: Query<'w, 's, (), With<SocketedShard>>,
}
impl OrderTreeParam<'_, '_> {
    /// The order's output socket.
    pub fn socket(&self, order: Entity) -> Option<Entity> {
        self.socket_holders.get(order).ok()?.iter().next()
    }

    pub fn is_fulfilled(&self, order: Entity) -> bool {
        self.socket(order).is_some_and(|socket| self.socketed.contains(socket))
    }

    pub fn set_release_destination(&self, commands: &mut Commands, order: Entity, destination: ShardReleaseDestination) {
        if let Some(socket) = self.socket(order) {
            commands.entity(socket).insert(destination);
        }
    }

    /// Required shards that do not yet have ingredient orders, including repeated quantities.
    #[log_tags(Tag::Shards)]
    pub fn missing(&self, order: Entity) -> impl Iterator<Item = Shard> + '_ {
        let recipe = self.orders.get(order).ok().and_then(|&ShardOrder(shard)| self.almanach.get_shard_info(shard).recipe.as_ref());
        recipe.into_iter().flat_map(|recipe| &recipe.reserved_cost).filter_map(move |entry| {
            #[error_dev("Order {order} requires non-shard ingredient {:?}; ingredient skipped", entry.resource_type)]
            let ResourceType::Shard(ingredient) = entry.resource_type else { return None; };
            let held = self.ingredients_of(order).iter()
                .filter(|&&held| self.orders.get(held).is_ok_and(|&ShardOrder(held_shard)| held_shard == ingredient))
                .count();
            Some(std::iter::repeat_n(ingredient, (entry.amount as usize).saturating_sub(held)))
        }).flatten()
    }

    /// Every required ingredient order exists and holds its shard.
    pub fn are_ready(&self, order: Entity) -> bool {
        self.missing(order).next().is_none()
            && self.ingredients_of(order).iter().all(|&ingredient| self.is_fulfilled(ingredient))
    }

    /// Empty when the order has no ingredient orders.
    pub fn ingredients_of(&self, order: Entity) -> &[Entity] {
        self.ingredients.get(order).map_or(&[], |ingredients| ingredients.collection())
    }

    /// Whether any ingredient order below `order`, at any depth, is in progress.
    pub fn has_jobs_in_progress_below(&self, order: Entity) -> bool {
        self.ingredients.iter_descendants_depth_first::<Ingredients>(order).any(|ingredient| self.in_progress.contains(ingredient))
    }

    /// The queue of the root order above `order`, or of `order` itself when it is a root.
    pub fn queue_of(&self, order: Entity) -> Option<Entity> {
        let root = self.parents.root_ancestor(order);
        self.queue_links.get(root).ok().map(|&InForgingQueue(queue)| queue)
    }

    /// Detaches active ingredient jobs into the root's queue before despawning the cancelled tree.
    /// Keeps `order` too if it is already in progress.
    #[log_tags(Tag::Shards)]
    pub fn soft_cancel(&self, commands: &mut Commands, order: Entity) {
        #[warn_dev("Order {order} has no queue; soft cancellation ignored")]
        let Some(queue) = self.queue_of(order) else { return; };
        let tree = std::iter::once(order).chain(self.ingredients.iter_descendants_depth_first::<Ingredients>(order));
        for job in tree.filter(|&job| self.in_progress.contains(job) && self.parents.contains(job)) {
            #[debug_dev("Order {job} continues as a root order")]
            commands.entity(job).remove::<IngredientOf>().insert(InForgingQueue(queue));
        }
        if !self.in_progress.contains(order) {
            commands.entity(order).despawn();
        }
    }
}

// ============================================================================
// BUILDER
// ============================================================================

/// Creates the order, its relationships, and an empty output socket, then schedules resolution.
/// During loading, the socket loader replaces the empty socket with its saved contents.
#[log_tags(Tag::Shards)]
fn on_builder_add_spawn_shard_order(
    trigger: On<Add<BuilderShardOrder>>,
    mut commands: Commands,
    almanach: Res<Almanach>,
    builders: Query<&BuilderShardOrder>,
) {
    let order = trigger.entity;
    let Ok(&BuilderShardOrder { shard, link, in_progress }) = builders.get(order) else { return; };

    let mut order_commands = commands.entity(order);
    order_commands.remove::<BuilderShardOrder>().insert((ShardOrder(shard), OrderNeedsResolution));
    match link {
        OrderLink::Queue(queue) => { order_commands.insert(InForgingQueue(queue)); }
        OrderLink::IngredientOf(parent) => { order_commands.insert(IngredientOf(parent)); }
    }
    commands.trigger(ShardSocketUpsert::new(order, ContentId::from(ORDER_SOCKET_ID), ShardSocket::exactly(shard, ORDER_SOCKET_DESCRIPTION)).with_state(ShardSocketState::Disabled));

    let Some((forge, remaining_secs)) = in_progress else { return; };
    match &almanach.get_shard_info(shard).recipe {
        Some(recipe) => { commands.entity(order).insert((ForgedBy(forge), ForgingProgress::resumed(recipe.duration, remaining_secs))); }
        None => warn_dev!("Order {order} was saved forging {shard}, which has no recipe — order left waiting"),
    }
}

// ============================================================================
// GATHERING
// ============================================================================

/// Marks an order whose ingredients need resolution. Forges skip it until resolution finishes.
#[derive(Component)]
pub(crate) struct OrderNeedsResolution;

/// Reserves available stock for ingredient orders, otherwise creates missing ingredient orders.
/// Skips fulfilled and active orders; existing ingredient orders count toward recipe requirements.
#[log_tags(Tag::Shards)]
fn resolve_orders(
    mut commands: Commands,
    mut stock: ResMut<Stock>,
    marked: Query<(Entity, &ShardOrder), With<OrderNeedsResolution>>,
    order_tree: OrderTreeParam,
) {
    for (order, &ShardOrder(shard)) in marked.iter() {
        commands.entity(order).remove::<OrderNeedsResolution>();
        if order_tree.in_progress.contains(order) || order_tree.is_fulfilled(order) { continue; }
        if order_tree.parents.contains(order) && let Some(socket) = order_tree.socket(order) && stock.try_remove((shard, 1)) {
            #[debug_dev("Order {order} reserved {shard} from stock")]
            commands.trigger(ShardSocketOperation::socket(socket, shard));
            continue;
        }
        for ingredient in order_tree.missing(order) {
            commands.spawn(BuilderShardOrder::new(ingredient, OrderLink::IngredientOf(order)));
        }
    }
}

/// Uses newly available stock to fulfill waiting ingredient orders, in pickup order within each queue.
#[log_tags(Tag::Shards)]
fn on_shard_stock_acquired_reserve_late_shards(
    trigger: On<ShardStockAcquired>,
    mut commands: Commands,
    mut stock: ResMut<Stock>,
    queues: Query<&ForgingQueue>,
    waiting_ingredients: Query<&ShardOrder, (With<IngredientOf>, Without<ForgedBy>)>,
    order_tree: OrderTreeParam,
) {
    let ShardStockAcquired(shard) = *trigger.event();
    for queue in queues.iter() {
        for order in OrderWalk::new(queue, &order_tree) {
            if !waiting_ingredients.get(order).is_ok_and(|&ShardOrder(wanted)| wanted == shard) { continue; }
            if order_tree.is_fulfilled(order) { continue; }
            let Some(socket) = order_tree.socket(order) else { continue; };
            if !stock.try_remove((shard, 1)) { return; }
            #[debug_dev("Order {order} reserved newly available {shard} from stock")]
            commands.trigger(ShardSocketOperation::socket(socket, shard));
        }
    }
}

// ============================================================================
// CANCEL AND RESET
// ============================================================================

#[log_tags(Tag::Shards)]
fn on_shard_order_cancel_request_do_so(
    trigger: On<ShardOrderCancelRequest>,
    mut commands: Commands,
    order_tree: OrderTreeParam,
) {
    let &ShardOrderCancelRequest { order, mode } = trigger.event();
    #[warn_dev("Entity {order} is not a shard order, cancel ignored")]
    let Ok(&ShardOrder(shard)) = order_tree.orders.get(order) else { return; };

    match mode {
        ShardOrderCancelMode::Hard => {
            #[info_player("Cancelled the order for {shard}")]
            commands.entity(order).despawn();
        }
        ShardOrderCancelMode::Soft => {
            #[info_player("Cancelled the order for {shard}. Jobs in progress will finish.")]
            order_tree.soft_cancel(&mut commands, order);
        }
    }
}

/// Clears progress and schedules ingredient resolution when an order loses its Forge.
/// Resolution skips completed orders and replaces consumed ingredients for interrupted ones.
/// Does nothing during despawn.
fn on_remove_forged_by_reset_order(trigger: On<Remove<ForgedBy>>, mut commands: Commands) {
    if trigger.trigger().new_archetype.is_none() { return; }
    commands.entity(trigger.entity).remove::<ForgingProgress>().insert(OrderNeedsResolution);
}

/// Redirects an order to `Stock` and soft-cancels it when its destination is removed.
/// Does nothing during despawn, allowing completed orders to deliver normally.
fn on_remove_shard_order_destination_soft_cancel(
    trigger: On<Remove<ShardOrderDestination>>,
    mut commands: Commands,
    order_tree: OrderTreeParam,
) {
    let order = trigger.entity;
    if trigger.trigger().new_archetype.is_none() { return; }
    // A job that survives the soft cancel must not deliver into a socket that no longer awaits it.
    order_tree.set_release_destination(&mut commands, order, ShardReleaseDestination::Stock);
    commands.trigger(ShardOrderCancelRequest { order, mode: ShardOrderCancelMode::Soft });
}

// ============================================================================
// FULFILLING
// ============================================================================

/// Cancels unneeded ingredients when an order receives its shard.
/// Root orders despawn to release the shard to their destination; ingredient orders keep it.
#[log_tags(Tag::Shards)]
fn on_insert_socketed_shard_fulfill_order(
    trigger: On<Insert<SocketedShard>>,
    mut commands: Commands,
    sockets: Query<&ShardSocketOf>,
    order_tree: OrderTreeParam,
) {
    let Ok(&ShardSocketOf(order)) = sockets.get(trigger.entity) else { return; };
    let Ok(&ShardOrder(shard)) = order_tree.orders.get(order) else { return; };

    for &ingredient in order_tree.ingredients_of(order) {
        order_tree.soft_cancel(&mut commands, ingredient);
    }
    if order_tree.parents.contains(order) { return; }
    #[debug_dev("Order {order} delivers {shard}")]
    commands.entity(order).despawn();
}

/// While an order has a destination, its socket releases the shard into the destination socket.
fn on_insert_shard_order_destination_release_to_it(
    trigger: On<Insert<ShardOrderDestination>>,
    mut commands: Commands,
    destinations: Query<&ShardOrderDestination>,
    order_tree: OrderTreeParam,
) {
    let order = trigger.entity;
    let Ok(destination) = destinations.get(order) else { return; };
    order_tree.set_release_destination(&mut commands, order, ShardReleaseDestination::Socket(destination.socket()));
}

// ============================================================================
// FORGING
// ============================================================================

/// Visits each queue root in order, yielding its ingredients before the root (post-order traversal).
struct OrderWalk<'a> {
    roots: Copied<Iter<'a, Entity>>,
    order_tree: &'a OrderTreeParam<'a, 'a>,
    /// Ancestors being visited, each paired with its remaining ingredients. Depth is bounded by tier count.
    stack: [(Entity, Copied<Iter<'a, Entity>>); ShardTier::COUNT],
    depth: usize,
}
impl<'a> OrderWalk<'a> {
    fn new(queue: &'a ForgingQueue, order_tree: &'a OrderTreeParam<'a, 'a>) -> Self {
        Self {
            roots: queue.collection().iter().copied(),
            order_tree,
            stack: std::array::from_fn(|_| (Entity::PLACEHOLDER, [].iter().copied())),
            depth: 0,
        }
    }

    #[log_tags(Tag::Shards)]
    fn enter(&mut self, order: Entity) {
        #[error_dev("Order {order} is nested deeper than any recipe allows — skipped")]
        if self.depth == self.stack.len() { return; }
        self.stack[self.depth] = (order, self.order_tree.ingredients_of(order).iter().copied());
        self.depth += 1;
    }
}
impl<'a> Iterator for OrderWalk<'a> {
    type Item = Entity;

    fn next(&mut self) -> Option<Entity> {
        loop {
            if self.depth == 0 {
                let root = self.roots.next()?;
                self.enter(root);
                continue;
            }
            let (order, ingredients) = &mut self.stack[self.depth - 1];
            match ingredients.next() {
                Some(ingredient) => self.enter(ingredient),
                None => {
                    let order = *order;
                    self.depth -= 1;
                    return Some(order);
                }
            }
        }
    }
}

/// Each free operational Forge takes the next ready order it can pay for. It looks in its Forge queue
/// first, then in the global queue if its setting allows.
#[log_tags(Tag::Forge)]
fn assign_orders_to_free_forges(
    mut commands: Commands,
    mut stock: ResMut<Stock>,
    global_queue: Single<Option<&ForgingQueue>, With<GlobalForgingQueue>>,
    free_forges: Query<(Entity, &GridCoords, Option<&ForgingQueue>, &WorksGlobalQueue), (With<Forge>, With<IsOperational>, Without<ForgeCurrentOrder>)>,
    waiting_orders: Query<&ShardOrder, (Without<ForgedBy>, Without<OrderNeedsResolution>)>,
    order_tree: OrderTreeParam,
) {
    // Share one iterator to prevent duplicate assignments before deferred ForgedBy inserts take effect.
    let mut global_orders = global_queue.map(|queue| OrderWalk::new(queue, &order_tree));

    // Returns whether the Forge took the order.
    let mut try_take = |forge: Entity, coords: GridCoords, order: Entity| -> bool {
        let Ok(&ShardOrder(shard)) = waiting_orders.get(order) else { return false; };
        if order_tree.is_fulfilled(order) { return false; }
        if !order_tree.are_ready(order) { return false; }
        let Some(recipe) = &order_tree.almanach.get_shard_info(shard).recipe else { return false; };
        if !stock.try_remove_all(&recipe.pickup_cost) { return false; }

        // Ingredients are used up. Their shards must not return to `Stock`.
        for &ingredient in order_tree.ingredients_of(order) {
            order_tree.set_release_destination(&mut commands, ingredient, ShardReleaseDestination::Void);
            commands.entity(ingredient).despawn();
        }
        info_player!("Forge at {coords} started forging {shard}");
        commands.entity(order).insert((ForgedBy(forge), ForgingProgress::new(recipe.duration)));
        true
    };

    for (forge, &coords, own_queue, &WorksGlobalQueue(works_global)) in free_forges.iter() {
        let mut own_orders = own_queue.map(|queue| OrderWalk::new(queue, &order_tree)).into_iter().flatten();
        if own_orders.any(|order| try_take(forge, coords, order)) { continue; }
        if !works_global { continue; }
        // All Forges share one walk over the global queue. Each continues where the previous one stopped.
        if let Some(global_orders) = global_orders.as_mut() {
            global_orders.any(|order| try_take(forge, coords, order));
        }
    }
}

/// Advances orders whose Forge is operational. A finished job frees its Forge and sockets its shard
/// into the order's socket, which fulfills the order.
#[log_tags(Tag::Forge)]
fn advance_orders_in_progress(
    mut commands: Commands,
    time: Res<Time>,
    operational_forges: Query<&GridCoords, (With<Forge>, With<IsOperational>)>,
    mut orders: Query<(Entity, &ShardOrder, &ForgedBy, &mut ForgingProgress)>,
    order_tree: OrderTreeParam,
) {
    for (order, &ShardOrder(shard), &ForgedBy(forge), mut progress) in orders.iter_mut() {
        let Ok(coords) = operational_forges.get(forge) else { continue; };
        if !progress.advance(time.delta()) { continue; }

        commands.entity(order).remove::<ForgedBy>();
        #[error_dev("Order {order} finished {shard} but has no socket, shard lost")]
        let Some(socket) = order_tree.socket(order) else { continue; };
        #[info_player("Forge at {coords} finished {shard}")]
        commands.trigger(ShardSocketOperation::socket(socket, shard));
    }
}

// ============================================================================
// PERSISTENCE
// ============================================================================

#[log_tags(Tag::GameSave)]
fn collect_global_forging_queue(
    global_queue: Single<Entity, With<GlobalForgingQueue>>,
    mut save: SaveWriter,
) {
    let id = global_queue.index_u32();
    save.submit(move |ctx| {
        ctx.register_entity(id)?;
        ctx.tx.execute("INSERT INTO global_forging_queue (id, entity_id) VALUES (1, ?1)", [id])?;
        Ok(())
    });
}

fn load_global_forging_queue(ctx: &mut LoadContext) -> LoadResult {
    ctx.for_each_entity("SELECT entity_id FROM global_forging_queue", |ctx, _old_id, entity, _row| {
        ctx.insert(entity, GlobalForgingQueue);
        Ok(())
    })
}

#[log_tags(Tag::GameSave)]
fn collect_forge_queues(
    forges: Query<(Entity, &WorksGlobalQueue), With<Forge>>,
    mut save: SaveWriter,
) {
    #[debug_dev("Saving {} forge queues", rows.len())]
    let rows: Vec<(u32, bool)> = forges.iter().map(|(forge, &WorksGlobalQueue(works_global))| (forge.index_u32(), works_global)).collect();
    if rows.is_empty() { return; }
    save.submit(move |ctx| {
        for (forge_id, works_global) in rows {
            ctx.register_entity(forge_id)?;
            ctx.tx.prepare_cached("INSERT INTO forge_queue (forge_id, works_global) VALUES (?1, ?2)")?
                .execute(rusqlite::params![forge_id, works_global])?;
        }
        Ok(())
    });
}

/// Runs after Forges spawn, so the saved setting replaces the default their builder inserts.
fn load_forge_queues(ctx: &mut LoadContext) -> LoadResult {
    ctx.for_each_entity("SELECT forge_id, works_global FROM forge_queue", |ctx, _old_id, forge, row| {
        ctx.insert(forge, WorksGlobalQueue(row.get(1)?));
        Ok(())
    })
}

/// Walks each queue: its root orders in placement order, each followed by its ingredient orders.
#[log_tags(Tag::GameSave)]
fn collect_shard_orders(
    queues: Query<(Entity, &ForgingQueue)>,
    ingredients: Query<&Ingredients>,
    orders: Query<(&ShardOrder, Option<&IngredientOf>, Option<(&ForgedBy, &ForgingProgress)>)>,
    mut save: SaveWriter,
) {
    struct OrderRow {
        id: u32,
        shard: Shard,
        /// Queue holder and position of a root order.
        queue: Option<(u32, usize)>,
        parent_id: Option<u32>,
        in_progress: Option<(u32, f32)>,
    }

    #[debug_dev("Saving {} shard orders", rows.len())]
    let rows: Vec<OrderRow> = queues.iter()
        .flat_map(|(queue_holder, queue)| queue.iter().enumerate().map(move |(position, root)| (queue_holder, position, root)))
        .flat_map(|(queue_holder, position, root)| {
            std::iter::once((root, Some((queue_holder.index_u32(), position))))
                .chain(ingredients.iter_descendants_depth_first::<Ingredients>(root).map(|ingredient| (ingredient, None)))
        })
        .filter_map(|(order, queue)| {
            let (&ShardOrder(shard), ingredient_of, in_progress) = orders.get(order).ok()?;
            Some(OrderRow {
                id: order.index_u32(),
                shard,
                queue,
                parent_id: ingredient_of.map(|&IngredientOf(parent)| parent.index_u32()),
                in_progress: in_progress.map(|(&ForgedBy(forge), progress)| (forge.index_u32(), progress.remaining_secs())),
            })
        })
        .collect();
    if rows.is_empty() { return; }
    save.submit(move |ctx| {
        for OrderRow { id, shard, queue, parent_id, in_progress } in rows {
            let (queue_holder_id, queue_position) = queue.unzip();
            let (forge_id, remaining_secs) = in_progress.unzip();
            ctx.register_entity(id)?;
            ctx.tx.prepare_cached("INSERT INTO shard_orders (id, shard_type, shard_tier, queue_holder_id, queue_position, parent_id, forge_id, remaining_secs) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)")?
                .execute(rusqlite::params![
                    id,
                    shard.shard_type.as_ref(),
                    shard.tier.as_ref(),
                    queue_holder_id,
                    queue_position,
                    parent_id,
                    forge_id,
                    remaining_secs,
                ])?;
        }
        Ok(())
    });
}

/// Restores order links and progress. Sorting by queue and position preserves root order priority.
fn load_shard_orders(ctx: &mut LoadContext) -> LoadResult {
    ctx.for_each_entity(
        "SELECT id, shard_type, shard_tier, queue_holder_id, parent_id, forge_id, remaining_secs FROM shard_orders ORDER BY queue_holder_id, queue_position",
        |ctx, _old_id, entity, row| {
            let shard = Shard::new(row.get_parsed(1)?, row.get_parsed(2)?);
            let link = match (ctx.optional_entity(row.get(3)?)?, ctx.optional_entity(row.get(4)?)?) {
                (_, Some(parent)) => OrderLink::IngredientOf(parent),
                (Some(queue), None) => OrderLink::Queue(queue),
                (None, None) => return Err(LoadError::unknown_value("shard order link", "neither queue nor parent")),
            };
            let in_progress = ctx.optional_entity(row.get(5)?)?.zip(row.get::<_, Option<f32>>(6)?);
            ctx.insert(entity, BuilderShardOrder::new(shard, link).with_in_progress(in_progress));
            Ok(())
        },
    )
}

