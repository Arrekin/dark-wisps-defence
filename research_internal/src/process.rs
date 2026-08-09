use bevy::prelude::*;

use outcomes::prelude::{FulfillOutcome, HasOutcomes};
use research::prelude::*;
use resources::prelude::{Cost, Stock};

/// Start or switch the active research. Parks the incumbent (back to
/// `Available`, progress retained) and sets the target `Active`. Only an
/// `Available` research can be started — one with no state cannot be targeted.
pub(crate) fn on_set_active_research(
    trigger: On<SetActiveResearch>,
    mut commands: Commands,
    current_active: Option<Single<Entity, With<ResearchActive>>>,
    target: Query<(), With<ResearchAvailable>>,
) {
    let target_entity = trigger.event().research;
    if target.get(target_entity).is_err() { return; }

    if let Some(current) = current_active
        && *current != target_entity
    {
        commands.entity(*current).insert(ResearchState::Available);
    }
    commands.entity(target_entity).insert(ResearchState::Active);
}

/// Park an active research: set it back to `Available`, progress retained.
/// No-ops on a research which is not active.
pub(crate) fn on_stop_research(
    trigger: On<StopResearch>,
    mut commands: Commands,
    active: Query<&ResearchState, With<ResearchActive>>,
) {
    let target = trigger.event().research;
    if active.get(target).is_err() { return; }
    commands.entity(target).insert(ResearchState::Available);
}

/// Advances the single active research. Payment and progress move in lockstep:
/// progress is clamped so it never crosses a resource-unit threshold it cannot
/// pay, which makes the research stall (no error) when stock runs dry and resume
/// when it returns. Runs only while the game is running and exactly one research
/// is active.
pub(crate) fn research_tick(
    mut commands: Commands,
    time: Res<Time>,
    mut stock: ResMut<Stock>,
    active: Single<(Entity, &Research, &mut ResearchRuntime), With<ResearchActive>>,
) {
    let (entity, research, mut runtime) = active.into_inner();
    let duration_secs = research.duration.as_secs_f32();

    // Nothing moves unless the next whole unit of every outstanding cost is
    // covered. Without this, progress below a unit threshold is free: `paid` is
    // `floor(fraction * amount)`, so a research with an empty stock creeps up to
    // just under its first unit boundary before `clamp_to_affordable` sees
    // anything owed — the bar advances while nothing is consumed.
    if !can_advance(runtime.progress, &research.cost, &stock) { return }

    let target = advanced_fraction(runtime.progress, duration_secs, time.delta_secs());
    let target = clamp_to_affordable(runtime.progress, target, &research.cost, &stock);
    pay_crossed_units(&mut commands, entity, runtime.progress, target, &research.cost, &mut stock);
    runtime.progress = target;

    if runtime.progress >= 1.0 {
        commands.entity(entity)
            .remove::<ResearchRuntime>()
            .insert(ResearchState::Completed);
        commands.trigger(ResearchFinished { research: entity });
    }
}

/// Emitted only by the tick — never on load. Fires `FulfillOutcome` on each
/// outcome. The research does not know what its outcomes are; each one acts on
/// itself.
pub(crate) fn on_research_finished(
    trigger: On<ResearchFinished>,
    mut commands: Commands,
    outcomes: Query<&HasOutcomes, With<Research>>,
) {
    let research = trigger.event().research;
    let Ok(has_outcomes) = outcomes.get(research) else { return; };
    for outcome in has_outcomes.iter() {
        commands.trigger(FulfillOutcome { outcome });
    }
}

// ---- Pure helpers (ported verbatim from the pre-rebuild implementation) ----

/// The fraction reached after `delta_secs` of unobstructed progress. A non-positive
/// duration means "instant": the fraction jumps to 1.0, with cost still charged
/// through the regular clamp-and-pay flow.
fn advanced_fraction(fraction: f32, duration_secs: f32, delta_secs: f32) -> f32 {
    if duration_secs <= 0.0 {
        1.0
    } else {
        (fraction + delta_secs / duration_secs).min(1.0)
    }
}

/// Whether progress may advance at all: every cost that still has units
/// outstanding must have at least one whole unit in stock.
///
/// This is the entry condition to `clamp_to_affordable`'s continuous stall. That
/// function only reacts once a unit boundary is *crossed*, which leaves the
/// sub-unit stretch before the first boundary unpaid for and therefore free —
/// harmless at an amount of 100, but a research costing a single unit would run
/// to 99.9% on an empty stock, since its one unit is not owed until `1.0`.
fn can_advance(fraction: f32, costs: &[Cost], stock: &Stock) -> bool {
    costs.iter().all(|cost| {
        let outstanding = cost.amount - units_paid(fraction, cost);
        outstanding <= 0 || stock.get(cost.resource_type) >= 1
    })
}

/// Whole units of `cost` paid at `fraction` (`paid = floor(fraction * amount)`).
pub(crate) fn units_paid(fraction: f32, cost: &Cost) -> i32 {
    (fraction * cost.amount as f32).floor() as i32
}

/// The furthest fraction this research can reach with the stock currently held. 1.0
/// when nothing will run out. Each cost can advance to `(paid + available) / amount`;
/// the research is limited by whichever runs out first.
pub(crate) fn reachable_fraction(fraction: f32, costs: &[Cost], stock: &Stock) -> f32 {
    costs.iter()
        .map(|cost| {
            let paid = units_paid(fraction, cost);
            let available = stock.get(cost.resource_type);
            (paid + available) as f32 / cost.amount as f32
        })
        .fold(1.0, f32::min)
        .clamp(0., 1.)
}

/// Clamps `target` so no cost crosses a unit threshold the stock cannot cover —
/// the research stalls just before its first unaffordable unit. Never clamps
/// below `fraction`: earned progress is kept.
fn clamp_to_affordable(fraction: f32, mut target: f32, costs: &[Cost], stock: &Stock) -> f32 {
    for cost in costs.iter() {
        let paid = units_paid(fraction, cost);
        let owed = units_paid(target, cost) - paid;
        let available = stock.get(cost.resource_type);
        if owed > available {
            target = target.min((paid + available) as f32 / cost.amount as f32);
        }
    }
    target.max(fraction)
}

/// Deducts the whole units of each cost crossed between `fraction` and `target`,
/// which must already be clamped to affordability. Fires `ResearchUnitPaid`
/// once for every unit deducted, so a step crossing several thresholds at once
/// still yields one event per unit.
fn pay_crossed_units(
    commands: &mut Commands,
    research: Entity,
    fraction: f32,
    target: f32,
    costs: &[Cost],
    stock: &mut Stock,
) {
    for cost in costs.iter() {
        let units = units_paid(target, cost) - units_paid(fraction, cost);
        if units > 0 {
            let removed = stock.try_remove(cost.resource_type, units);
            debug_assert!(removed, "clamp_to_affordable must keep crossed units payable");
            for _ in 0..units {
                commands.trigger(ResearchUnitPaid { research, resource_type: cost.resource_type });
            }
        }
    }
}

