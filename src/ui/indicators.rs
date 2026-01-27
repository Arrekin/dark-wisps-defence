use bevy::ecs::entity_disabling::Disabled;

use crate::prelude::*;
use crate::map_objects::dark_ore::dark_ore_area_scanner::{HasOreInScannerRange, NoOreInScannerRange};

pub struct IndicatorsPlugin;
impl Plugin for IndicatorsPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Update, (
                cycle_indicators_system,
            ))
            .add_observer(IndicatorType::on_insert_update_sprite_handle);
    }
}

#[derive(Component)]
#[relationship(relationship_target = Indicators)]
pub struct IndicatorOf(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = IndicatorOf, linked_spawn)]
pub struct Indicators(Vec<Entity>);

#[derive(Component)]
#[require(Transform, Visibility, Sprite, ZDepth = Z_MAP_UI)]
pub struct IndicatorDisplay {
    pub active_index: usize,
    pub cycle_time: f32,
}
impl IndicatorDisplay {
    const PERIOD_SECONDS: f32 = 3.;
    const MIN_ALPHA: f32 = 0.;
    const MAX_ALPHA: f32 = 1.;
}
impl Default for IndicatorDisplay {
    fn default() -> Self {
        Self {
            active_index: 0,
            cycle_time: 0.0,
        }
    }
}

#[derive(Component, Default)]
struct IndicatorSpriteHandle(Handle<Image>);

/// When attaching triggers to the parent for change detection, we need to know which Identicator entity is interested in that action
/// Should be attached to the observer entity.
#[derive(Component)]
pub struct IndicatorObserverForChanges(Entity);

#[derive(Component, Clone, Copy, PartialEq, Eq)]
#[component(immutable)]
#[require(IndicatorSpriteHandle, Disabled)]
pub enum IndicatorType {
    NoPower,
    OreDepleted,
    DisabledByPlayer,
}
impl IndicatorType {
    fn on_insert_update_sprite_handle(
        trigger: On<Insert, IndicatorType>,
        mut commands: Commands,
        asset_server: Res<AssetServer>,
        mut indicators: Query<(&IndicatorType, &mut IndicatorSpriteHandle, &IndicatorOf, Has<Disabled>)>,
        parents_with_no_power: Query<(), With<NoPower>>,
        parents_with_no_ore: Query<(), With<NoOreInScannerRange>>,
        parents_disabled_by_player: Query<(), With<DisabledByPlayer>>,
    ) {
        let entity = trigger.entity;
        let (indicator_type, mut sprite_handle, indicator_of, _) = indicators.get_mut(entity).unwrap();
        let path = match indicator_type {
            IndicatorType::NoPower => "indicators/no_power.png",
            IndicatorType::OreDepleted => "indicators/no_dark_ore.png",
            IndicatorType::DisabledByPlayer => "indicators/disabled.png",
        };
        sprite_handle.0 = asset_server.load(path);

        match indicator_type {
            IndicatorType::NoPower => {
                // Add Power State observers to the parent
                commands.spawn((Observer::new(Self::on_parent_gains_power).with_entity(indicator_of.0), IndicatorObserverForChanges(entity)));
                commands.spawn((Observer::new(Self::on_parent_looses_power).with_entity(indicator_of.0), IndicatorObserverForChanges(entity)));
                // Configure initial state
                if parents_with_no_power.contains(indicator_of.0) {
                    commands.entity(entity).remove::<Disabled>();
                }
            }
            IndicatorType::OreDepleted => {
                // Add Ore State observers to the parent
                commands.spawn((Observer::new(Self::on_parent_gains_ore).with_entity(indicator_of.0), IndicatorObserverForChanges(entity)));
                commands.spawn((Observer::new(Self::on_parent_looses_ore).with_entity(indicator_of.0), IndicatorObserverForChanges(entity)));
                // Configure initial state - check if parent has no ore in scanner range
                if parents_with_no_ore.contains(indicator_of.0) {
                    commands.entity(entity).remove::<Disabled>();
                }
            }
            IndicatorType::DisabledByPlayer => {
                // Add DisabledByPlayer observers to the parent
                commands.spawn((Observer::new(Self::on_parent_disabled_by_player).with_entity(indicator_of.0), IndicatorObserverForChanges(entity)));
                commands.spawn((Observer::new(Self::on_parent_enabled_by_player).with_entity(indicator_of.0), IndicatorObserverForChanges(entity)));
                // Configure initial state - check if parent is disabled by player
                if parents_disabled_by_player.contains(indicator_of.0) {
                    commands.entity(entity).remove::<Disabled>();
                }
            }
        }
    }

    // TODO: Condolidate when Bevy supports multiple different triggers in one expression
    fn on_parent_looses_power(
        trigger: On<Insert, NoPower>,
        mut commands: Commands,
        observers_for_changes: Query<&IndicatorObserverForChanges>,
        indicators: Query<(&IndicatorType, Has<Disabled>)>,
    ) {
        let observer_entity = trigger.observer();
        let Ok(indicator_entity) = observers_for_changes.get(observer_entity) else { return; };
        let Ok((indicator_type, _)) = indicators.get(indicator_entity.0) else { 
            commands.entity(indicator_entity.0).despawn(); // Indicator no longer exist, remove the observer
            return;
        };
        if !matches!(indicator_type, IndicatorType::NoPower) { return; };
        commands.entity(indicator_entity.0).remove::<Disabled>();
    }

    fn on_parent_gains_power(
        trigger: On<Insert, HasPower>,
        mut commands: Commands,
        observers_for_changes: Query<&IndicatorObserverForChanges>,
        indicators: Query<&IndicatorType>,
    ) {
        let observer_entity = trigger.observer();
        let Ok(indicator_entity) = observers_for_changes.get(observer_entity) else { return; };
        let Ok(indicator_type) = indicators.get(indicator_entity.0) else { 
            commands.entity(indicator_entity.0).despawn(); // Indicator no longer exist, remove the observer
            return;
        };
        if !matches!(indicator_type, IndicatorType::NoPower) { return; };
        commands.entity(indicator_entity.0).insert(Disabled);
    }

    fn on_parent_looses_ore(
        trigger: On<Insert, NoOreInScannerRange>,
        mut commands: Commands,
        observers_for_changes: Query<&IndicatorObserverForChanges>,
        indicators: Query<(&IndicatorType, Has<Disabled>)>,
    ) {
        let observer_entity = trigger.observer();
        let Ok(indicator_entity) = observers_for_changes.get(observer_entity) else { return; };
        let Ok((indicator_type, _)) = indicators.get(indicator_entity.0) else { 
            commands.entity(indicator_entity.0).despawn(); // Indicator no longer exist, remove the observer
            return;
        };
        if !matches!(indicator_type, IndicatorType::OreDepleted) { return; };
        commands.entity(indicator_entity.0).remove::<Disabled>();
    }

    fn on_parent_gains_ore(
        trigger: On<Insert, HasOreInScannerRange>,
        mut commands: Commands,
        observers_for_changes: Query<&IndicatorObserverForChanges>,
        indicators: Query<&IndicatorType>,
    ) {
        let observer_entity = trigger.observer();
        let Ok(indicator_entity) = observers_for_changes.get(observer_entity) else { return; };
        let Ok(indicator_type) = indicators.get(indicator_entity.0) else { 
            commands.entity(indicator_entity.0).despawn(); // Indicator no longer exist, remove the observer
            return;
        };
        if !matches!(indicator_type, IndicatorType::OreDepleted) { return; };
        commands.entity(indicator_entity.0).insert(Disabled);
    }

    fn on_parent_disabled_by_player(
        trigger: On<Insert, DisabledByPlayer>,
        mut commands: Commands,
        observers_for_changes: Query<&IndicatorObserverForChanges>,
        indicators: Query<&IndicatorType, With<Disabled>>,
    ) {
        let observer_entity = trigger.observer();
        let Ok(indicator_entity) = observers_for_changes.get(observer_entity) else { return; };
        let Ok(indicator_type) = indicators.get(indicator_entity.0) else { 
            commands.entity(indicator_entity.0).despawn(); // Indicator no longer exist, remove the observer
            return;
        };
        if !matches!(indicator_type, IndicatorType::DisabledByPlayer) { return; };
        commands.entity(indicator_entity.0).remove::<Disabled>();
    }

    fn on_parent_enabled_by_player(
        trigger: On<Remove, DisabledByPlayer>,
        mut commands: Commands,
        observers_for_changes: Query<&IndicatorObserverForChanges>,
        indicators: Query<&IndicatorType>,
    ) {
        let observer_entity = trigger.observer();
        let Ok(indicator_entity) = observers_for_changes.get(observer_entity) else { return; };
        let Ok(indicator_type) = indicators.get(indicator_entity.0) else { 
            commands.entity(indicator_entity.0).despawn(); // Indicator no longer exist, remove the observer
            return;
        };
        if !matches!(indicator_type, IndicatorType::DisabledByPlayer) { return; };
        commands.entity(indicator_entity.0).try_insert(Disabled);
    }
}

// Cycle through indicators and animate fade in/out.
fn cycle_indicators_system(
    time: Res<Time>,
    parents: Query<&Indicators>,
    indicators_sprites: Query<&IndicatorSpriteHandle>,
    mut displays: Query<(&mut IndicatorDisplay, &mut Sprite, &mut Visibility, &ChildOf)>,
) {
    for (mut display, mut sprite, mut visibility, child_of) in displays.iter_mut() {
        let Ok(indicators) = parents.get(child_of.parent()) else {
            // No active indicators, hide display
            *visibility = Visibility::Hidden;
            continue; 
        };
        let indicator_count: usize = indicators.0.len();
        *visibility = Visibility::Inherited;
        
        // Update cycle time
        display.cycle_time += time.delta_secs();
        if display.cycle_time >= IndicatorDisplay::PERIOD_SECONDS {
            display.cycle_time = 0.;
            display.active_index = (display.active_index + 1) % indicator_count;
        }

        // Get active indicator and update sprite
        let Ok(sprite_handle) = indicators_sprites.get(indicators.0[display.active_index]) else { 
            // Indicator Disabled, cycle
            *visibility = Visibility::Hidden;
            display.active_index = (display.active_index + 1) % indicator_count;
            continue; 
        };
        sprite.image = sprite_handle.0.clone();
                
        // Calculate fade alpha with moderate non-linear curve
        // Short visible plateau with smooth transitions
        let progress = display.cycle_time / IndicatorDisplay::PERIOD_SECONDS;
        let alpha = if progress < 0.3 {
            // Fade in (first 30% of cycle)
            let t = progress / 0.3; // Map 0..0.3 to 0..1
            let smoothed = t * t * (3.0 - 2.0 * t); // smoothstep
            IndicatorDisplay::MIN_ALPHA + (IndicatorDisplay::MAX_ALPHA - IndicatorDisplay::MIN_ALPHA) * smoothed
        } else if progress < 0.4 {
            // Visible plateau (brief 10% of cycle)
            IndicatorDisplay::MAX_ALPHA
        } else {
            // Fade out (last 60% of cycle)
            let t = (progress - 0.4) / 0.6; // Map 0.4..1 to 0..1
            let smoothed = t * t * (3.0 - 2.0 * t); // smoothstep
            IndicatorDisplay::MAX_ALPHA - (IndicatorDisplay::MAX_ALPHA - IndicatorDisplay::MIN_ALPHA) * smoothed
        };
        sprite.color.set_alpha(alpha);
    }
}