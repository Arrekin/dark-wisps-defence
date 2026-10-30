use bevy::prelude::*;

use alteration::modifiers::prelude::{AttackAreaRange, AttackRangeType, MaxAttackRange, MinAttackRange};
use buildings::prelude::Tower;
use game_core::prelude::MapInfo;
use grids::tower_ranges::TowerRangeIndex;
use states::prelude::MapLoadingStage;

pub(crate) struct TowerRangesPlugin;
impl Plugin for TowerRangesPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(OnExit(MapLoadingStage::LoadMapInfo), |mut commands: Commands, map_info: Res<MapInfo>| { commands.insert_resource(TowerRangeIndex::new_with_size(map_info.grid_bounds)); })
            .add_observer(on_insert_range_stat_index_tower_range::<MinAttackRange>)
            .add_observer(on_insert_range_stat_index_tower_range::<MaxAttackRange>)
            .add_observer(on_remove_attack_range_type_unindex_tower_range);
    }
}

/// Updates the indexed range when a minimum or maximum attack range component is inserted.
fn on_insert_range_stat_index_tower_range<Stat: Component>(
    trigger: On<Insert<Stat>>,
    mut tower_range_index: ResMut<TowerRangeIndex>,
    towers: Query<AttackAreaRange, With<Tower>>,
) {
    let entity = trigger.entity;
    let Ok(attack_range) = towers.get(entity) else { return; };
    tower_range_index.upsert(entity, attack_range.area_range());
}

fn on_remove_attack_range_type_unindex_tower_range(
    trigger: On<Remove<AttackRangeType>>,
    mut tower_range_index: ResMut<TowerRangeIndex>,
) {
    tower_range_index.remove(trigger.entity);
}
