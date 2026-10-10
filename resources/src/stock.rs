use std::collections::HashMap;

use bevy::prelude::*;

use game_core::prelude::{SSS, Shard};

use crate::common::{ResourceAmount, ResourceType};

// Starting amounts
const STARTING_DARK_ORE: i32 = 5555;

// Stock caps
const MAX_DARK_ORE_STOCK: i32 = 9999;
const MAX_ESSENCE_STOCK: i32 = 999;
const MAX_SHARD_STOCK: i32 = i32::MAX;

#[derive(Message)]
pub struct StockChangedMessage {
    pub resource_type: ResourceType,
    pub delta: i32,
    pub new_amount: i32,
}

/// Reports a net stock increase for one shard type and tier, once per frame.
#[derive(Event, Clone, Copy, Debug)]
pub struct ShardStockAcquired(pub Shard);

#[derive(Clone)]
struct StockInfo {
    amount: i32,
    max_amount: i32,
    /// Net change since the last `take_pending_deltas`.
    pending_delta: i32,
}
impl StockInfo {
    /// Moves to `amount`, capped at `max_amount`, and records the real change as pending.
    fn change_to(&mut self, amount: i32) {
        let previous = self.amount;
        self.amount = std::cmp::min(self.max_amount, amount);
        self.pending_delta += self.amount - previous;
    }
}

#[derive(Resource, Clone, SSS)]
pub struct Stock {
    resources: HashMap<ResourceType, StockInfo>,
}

impl Stock {
    pub fn get(&self, resource_type: impl Into<ResourceType>) -> i32 {
        self.get_info(resource_type.into()).amount
    }
    pub fn has(&self, entry: impl Into<ResourceAmount>) -> bool {
        let entry = entry.into();
        self.get_info(entry.resource_type).amount >= entry.amount
    }
    /// Expects each resource at most once in `amounts`. Entries are checked one by one, so repeated
    /// entries of one resource are not summed and can pass while their total is not held.
    pub fn has_all(&self, amounts: &[ResourceAmount]) -> bool {
        amounts.iter().all(|entry| self.has(*entry))
    }
    pub fn add(&mut self, entry: impl Into<ResourceAmount>) {
        let entry = entry.into();
        let info = self.get_info_mut(entry.resource_type);
        info.change_to(info.amount.saturating_add(entry.amount));
    }
    pub fn set(&mut self, entry: impl Into<ResourceAmount>) {
        let entry = entry.into();
        self.get_info_mut(entry.resource_type).change_to(entry.amount);
    }
    pub fn try_remove(&mut self, entry: impl Into<ResourceAmount>) -> bool {
        let entry = entry.into();
        let info = self.get_info_mut(entry.resource_type);
        if info.amount < entry.amount { return false; }
        info.change_to(info.amount - entry.amount);
        true
    }
    /// Removes all of `amounts` or nothing. Same unique-resource expectation as [`Self::has_all`].
    pub fn try_remove_all(&mut self, amounts: &[ResourceAmount]) -> bool {
        if !self.has_all(amounts) { return false; }
        for entry in amounts {
            self.try_remove(*entry);
        }
        true
    }
    /// Every non-zero net change since the last call, resetting them.
    pub fn take_pending_deltas(&mut self) -> Vec<ResourceAmount> {
        self.resources.iter_mut()
            .filter(|(_, info)| info.pending_delta != 0)
            .map(|(resource_type, info)| ResourceAmount::new(*resource_type, std::mem::take(&mut info.pending_delta)))
            .collect()
    }
    /// Every tracked resource with its current amount, held or not, in `ResourceType::all` order.
    pub fn iter(&self) -> impl Iterator<Item = ResourceAmount> {
        ResourceType::all().map(|resource_type| ResourceAmount::new(resource_type, self.get(resource_type)))
    }
    fn get_info(&self, resource_type: ResourceType) -> &StockInfo {
        self.resources.get(&resource_type).unwrap_or_else(|| panic!("Resource type {resource_type:?} not found in stock"))
    }
    fn get_info_mut(&mut self, resource_type: ResourceType) -> &mut StockInfo {
        self.resources.get_mut(&resource_type).unwrap_or_else(|| panic!("Resource type {resource_type:?} not found in stock"))
    }
}
impl Default for Stock {
    fn default() -> Self {
        let resources = ResourceType::all()
            .map(|resource_type| {
                let (amount, max_amount) = match resource_type {
                    ResourceType::DarkOre => (STARTING_DARK_ORE, MAX_DARK_ORE_STOCK),
                    ResourceType::Essence(_) => (0, MAX_ESSENCE_STOCK),
                    ResourceType::Shard(_) => (0, MAX_SHARD_STOCK),
                };
                (resource_type, StockInfo { amount, max_amount, pending_delta: 0 })
            })
            .collect();
        Self { resources }
    }
}
