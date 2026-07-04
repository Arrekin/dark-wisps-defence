use std::collections::HashMap;

use bevy::prelude::*;
use strum::IntoEnumIterator;

use game_core::prelude::SSS;

use crate::common::{Cost, EssenceType, ResourceType};

const MAX_DARK_ORE_STOCK: i32 = 9999;
const MAX_ESSENCE_STOCK: i32 = 999;

#[derive(Message)]
pub struct StockChangedEvent {
    pub resource_type: ResourceType,
    pub delta: i32,
    pub new_amount: i32,
}

#[derive(Clone)]
struct StockInfo {
    amount: i32,
    max_amount: i32,
}

#[derive(Resource, Clone, SSS)]
pub struct Stock {
    current: HashMap<ResourceType, StockInfo>,
    delta: HashMap<ResourceType, i32>,
}

impl Stock {
    pub fn get(&self, resource_type: ResourceType) -> i32 {
        self.get_info(resource_type).amount
    }
    pub fn can_cover(&self, cost: &Cost) -> bool {
        self.has(cost.resource_type, cost.amount)
    }
    pub fn can_cover_all(&self, costs: &[Cost]) -> bool {
        costs.iter().all(|c| self.has(c.resource_type, c.amount))
    }
    pub fn has(&self, resource_type: ResourceType, amount: i32) -> bool {
        self.get_info(resource_type).amount >= amount
    }
    pub fn add(&mut self, resource_type: ResourceType, amount: i32) {
        let info = self.get_info_mut(resource_type);
        info.amount = std::cmp::min(info.max_amount, info.amount + amount);
        self.add_delta(resource_type, amount);
    }
    pub fn set(&mut self, resource_type: ResourceType, amount: i32) {
        let info = self.get_info_mut(resource_type);
        let delta = amount - info.amount;
        info.amount = std::cmp::min(info.max_amount, amount);
        self.add_delta(resource_type, delta);
    }
    pub fn try_pay_cost(&mut self, cost: Cost) -> bool {
        self.try_remove(cost.resource_type, cost.amount)
    }
    pub fn try_pay_costs(&mut self, costs: &[Cost]) -> bool {
        if !self.can_cover_all(costs) { return false; }
        for cost in costs {
            self.try_remove(cost.resource_type, cost.amount);
        }
        true
    }
    pub fn try_remove(&mut self, resource_type: ResourceType, amount: i32) -> bool {
        let info = self.get_info_mut(resource_type);
        if info.amount < amount { return false; }
        info.amount = info.amount - amount;
        self.add_delta(resource_type, -amount);
        true
    }
    fn get_info(&self, resource_type: ResourceType) -> &StockInfo {
        self.current.get(&resource_type).expect(format!("Resource type {resource_type:?} not found in stock").as_str())
    }
    fn get_info_mut(&mut self, resource_type: ResourceType) -> &mut StockInfo {
        self.current.get_mut(&resource_type).expect(format!("Resource type {resource_type:?} not found in stock").as_str())
    }
    fn add_delta(&mut self, resource_type: ResourceType, amount: i32) {
        *self.delta.get_mut(&resource_type).expect(format!("Resource type {resource_type:?} not found in delta").as_str()) += amount;
    }
    pub fn take_pending_deltas(&mut self) -> Vec<(ResourceType, i32)> {
        let pending: Vec<_> = self.delta.iter().filter(|(_, d)| **d != 0).map(|(rt, d)| (*rt, *d)).collect();
        self.delta.values_mut().for_each(|v| *v = 0);
        pending
    }
}
impl Default for Stock {
    fn default() -> Self {
        let mut current = HashMap::new();
        let mut delta = HashMap::new();
        current.insert(ResourceType::DarkOre, StockInfo { amount: 5555, max_amount: MAX_DARK_ORE_STOCK });
        delta.insert(ResourceType::DarkOre, 0);
        for essence_type in EssenceType::iter() {
            current.insert(ResourceType::Essence(essence_type), StockInfo { amount: 0, max_amount: MAX_ESSENCE_STOCK });
            delta.insert(ResourceType::Essence(essence_type), 0);
        }
        Self { current, delta }
    }
}


