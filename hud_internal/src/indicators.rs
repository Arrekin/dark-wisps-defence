use bevy::ecs::entity_disabling::Disabled;
use bevy::prelude::*;

use game_core::prelude::{DisabledByPlayer, IsPowered, NeedsPower};
use hud::prelude::*;
use map_objects::prelude::{HasOreInScannerRange, NoOreInScannerRange};

pub struct IndicatorsPlugin;
impl Plugin for IndicatorsPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Update, (
                cycle_indicators_system,
            ))
            .add_observer(on_insert_update_sprite_handle);
    }
}

const PERIOD_SECONDS: f32 = 3.;
const MIN_ALPHA: f32 = 0.;
const MAX_ALPHA: f32 = 1.;

fn on_insert_update_sprite_handle(
    trigger: On<Insert, IndicatorType>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut indicators: Query<(&IndicatorType, &mut IndicatorSpriteHandle, &IndicatorOf, Has<Disabled>)>,
    parents_with_no_power: Query<(), (With<NeedsPower>, Without<IsPowered>)>,
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

    let parent = indicator_of.0;
    match indicator_type {
        IndicatorType::NoPower => {
            commands.entity(parent)
                .observe(disable_on_power_gained(entity))
                .observe(enable_on_power_lost(entity));
            if parents_with_no_power.contains(parent) {
                commands.entity(entity).remove::<Disabled>();
            }
        }
        IndicatorType::OreDepleted => {
            commands.entity(parent)
                .observe(disable_on_ore_gained(entity))
                .observe(enable_on_ore_lost(entity));
            if parents_with_no_ore.contains(parent) {
                commands.entity(entity).remove::<Disabled>();
            }
        }
        IndicatorType::DisabledByPlayer => {
            commands.entity(parent)
                .observe(enable_on_disabled_by_player(entity))
                .observe(disable_on_enabled_by_player(entity));
            if parents_disabled_by_player.contains(parent) {
                commands.entity(entity).remove::<Disabled>();
            }
        }
    }
}

fn disable_on_power_gained(entity: Entity) -> impl Fn(On<Insert, IsPowered>, Commands, Query<&IndicatorType>) {
    move |trigger, mut commands, indicators| {
        if trigger.trigger().new_archetype.is_some() && indicators.get(entity).is_err() {
            commands.entity(trigger.observer()).try_despawn();
            return;
        }
        commands.entity(entity).try_insert(Disabled);
    }
}
fn enable_on_power_lost(entity: Entity) -> impl Fn(On<Remove, IsPowered>, Commands, Query<&IndicatorType>) {
    move |trigger, mut commands, indicators| {
        if trigger.trigger().new_archetype.is_some() && indicators.get(entity).is_err() {
            commands.entity(trigger.observer()).try_despawn();
            return;
        }
        commands.entity(entity).try_remove::<Disabled>();
    }
}
fn disable_on_ore_gained(entity: Entity) -> impl Fn(On<Insert, HasOreInScannerRange>, Commands, Query<&IndicatorType>) {
    move |trigger, mut commands, indicators| {
        if trigger.trigger().new_archetype.is_some() && indicators.get(entity).is_err() {
            commands.entity(trigger.observer()).try_despawn();
            return;
        }
        commands.entity(entity).try_insert(Disabled);
    }
}
fn enable_on_ore_lost(entity: Entity) -> impl Fn(On<Insert, NoOreInScannerRange>, Commands, Query<&IndicatorType>) {
    move |trigger, mut commands, indicators| {
        if trigger.trigger().new_archetype.is_some() && indicators.get(entity).is_err() {
            commands.entity(trigger.observer()).try_despawn();
            return;
        }
        commands.entity(entity).try_remove::<Disabled>();
    }
}
fn enable_on_disabled_by_player(entity: Entity) -> impl Fn(On<Insert, DisabledByPlayer>, Commands, Query<&IndicatorType>) {
    move |trigger, mut commands, indicators| {
        if trigger.trigger().new_archetype.is_some() && indicators.get(entity).is_err() {
            commands.entity(trigger.observer()).try_despawn();
            return;
        }
        commands.entity(entity).try_remove::<Disabled>();
    }
}
fn disable_on_enabled_by_player(entity: Entity) -> impl Fn(On<Remove, DisabledByPlayer>, Commands, Query<&IndicatorType>) {
    move |trigger, mut commands, indicators| {
        if trigger.trigger().new_archetype.is_some() && indicators.get(entity).is_err() {
            commands.entity(trigger.observer()).try_despawn();
            return;
        }
        commands.entity(entity).try_insert(Disabled);
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
        let indicator_count: usize = indicators.entities().len();
        *visibility = Visibility::Inherited;

        // Update cycle time
        display.cycle_time += time.delta_secs();
        if display.cycle_time >= PERIOD_SECONDS {
            display.cycle_time = 0.;
            display.active_index = (display.active_index + 1) % indicator_count;
        }

        // Get active indicator and update sprite
        let Ok(sprite_handle) = indicators_sprites.get(indicators.entities()[display.active_index]) else {
            // Indicator Disabled, cycle
            *visibility = Visibility::Hidden;
            display.active_index = (display.active_index + 1) % indicator_count;
            continue;
        };
        sprite.image = sprite_handle.0.clone();

        // Calculate fade alpha with moderate non-linear curve
        // Short visible plateau with smooth transitions
        let progress = display.cycle_time / PERIOD_SECONDS;
        let alpha = if progress < 0.3 {
            // Fade in (first 30% of cycle)
            let t = progress / 0.3; // Map 0..0.3 to 0..1
            let smoothed = t * t * (3.0 - 2.0 * t); // smoothstep
            MIN_ALPHA + (MAX_ALPHA - MIN_ALPHA) * smoothed
        } else if progress < 0.4 {
            // Visible plateau (brief 10% of cycle)
            MAX_ALPHA
        } else {
            // Fade out (last 60% of cycle)
            let t = (progress - 0.4) / 0.6; // Map 0.4..1 to 0..1
            let smoothed = t * t * (3.0 - 2.0 * t); // smoothstep
            MAX_ALPHA - (MAX_ALPHA - MIN_ALPHA) * smoothed
        };
        sprite.color.set_alpha(alpha);
    }
}
