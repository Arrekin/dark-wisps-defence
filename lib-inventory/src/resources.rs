use strum::{AsRefStr, EnumIter, EnumString, IntoEnumIterator};

use crate::lib_prelude::*;

pub mod resources_prelude {
    pub use super::*;
}

pub struct ResourcesPlugin;
impl Plugin for ResourcesPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<Stock>()
            .add_message::<StockChangedEvent>()
            .add_systems(PostUpdate, emit_delta_events_system.run_if(resource_changed::<Stock>))
            .add_systems(OnEnter(MapLoadingStage::Init), |mut commands: Commands| { commands.insert_resource(Stock::default()); })
            .register_db_loader::<StockLoader>(MapLoadingStage::LoadResources)
            .register_db_saver(Stock::on_game_save);
    }
}

pub const MAX_DARK_ORE_STOCK: i32 = 9999;
pub const MAX_ESSENCE_STOCK: i32 = 999;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResourceType {
    DarkOre,
    Essence(EssenceType),
}
impl From<EssenceType> for ResourceType {
    fn from(essence_type: EssenceType) -> Self {
        Self::Essence(essence_type)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, EnumString, AsRefStr, EnumIter)]
pub enum EssenceType {
    Fire,
    Water,
    Light,
    Electric,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EssenceContainer {
    pub essence_type: EssenceType,
    pub amount: i32,
}
impl EssenceContainer {
    pub fn new(essence_type: EssenceType, amount: i32) -> Self {
        Self { essence_type, amount }
    }
}


#[derive(Component)]
pub struct EssencesContainer(pub Vec<EssenceContainer>);
impl From<EssenceContainer> for EssencesContainer {
    fn from(essence_container: EssenceContainer) -> Self {
        Self(vec![essence_container])
    }
}



#[derive(Message)]
pub struct StockChangedEvent {
    pub resource_type: ResourceType,
    pub delta: i32,
    pub new_amount: i32,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Cost {
    pub resource_type: ResourceType,
    pub amount: i32,
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

impl Saveable for Stock {
    fn save(self, tx: &rusqlite::Transaction) -> rusqlite::Result<()> {
        tx.save_stock_resource("DarkOre", self.get(ResourceType::DarkOre))?;
        for essence_type in EssenceType::iter() {
            let resource_key = essence_type.as_ref();
            tx.save_stock_resource(&resource_key, self.get(ResourceType::Essence(essence_type)))?;
        }
        Ok(())
    }
}

impl Stock {
    fn on_game_save(
        mut commands: Commands,
        stock: Res<Stock>,
    ) {
        commands.queue(SaveableBatchCommand::from_single(stock.clone()));
    }
}

struct StockLoader;
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

fn emit_delta_events_system(
    mut stock: ResMut<Stock>,
    mut event_writer: MessageWriter<StockChangedEvent>,
 ) {
    for (resource_type, delta) in stock.delta.iter() {
        if *delta != 0 {
            event_writer.write(StockChangedEvent { resource_type: *resource_type, delta: *delta, new_amount: stock.get(*resource_type) });
        }
    }
    stock.delta.values_mut().for_each(|v| *v = 0);
}