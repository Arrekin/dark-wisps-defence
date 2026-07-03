use bevy::prelude::*;

use logging::prelude::{Log, Tag};
use research::prelude::{
    ActiveResearch, CheckForObsoletion, Completed, FulfillOutcome, Obsolete, OutcomeSatisfied,
    Research, ResearchCompleted, ResearchOutcomeOf, ResearchOutcomes, ResearchProgress,
    ResearchSpec, SetActiveResearch, StopResearch,
};
use resources::prelude::{Cost, Stock};

/// Advances the single active research. Payment and progress move in lockstep: progress is clamped so
/// it never crosses a resource-unit threshold it cannot pay, which makes the research stall (no error)
/// when stock runs dry and resume when it returns. Runs only while exactly one research is active.
pub(crate) fn research_tick(
    mut commands: Commands,
    time: Res<Time>,
    mut stock: ResMut<Stock>,
    active: Single<(Entity, &Research, &ResearchSpec, &mut ResearchProgress, &ResearchOutcomes), With<ActiveResearch>>,
) {
    let (entity, research, spec, mut progress, outcomes) = active.into_inner();

    let target = advanced_fraction(progress.fraction, spec.duration.as_secs_f32(), time.delta_secs());
    let target = clamp_to_affordable(progress.fraction, target, &spec.cost, &stock);
    pay_crossed_units(progress.fraction, target, &spec.cost, &mut stock);
    progress.fraction = target;

    if progress.fraction >= 1.0 {
        let research_type = research.0;
        for outcome in outcomes.iter() {
            commands.trigger(FulfillOutcome { outcome });
        }
        commands.entity(entity)
            .remove::<ResearchProgress>()
            .remove::<ActiveResearch>()
            .insert(Completed);
        commands.trigger(ResearchCompleted(research_type));
        Log::info().player().tag(Tag::Research).message(format!("Research complete: {research_type}"));
    }
}

/// The fraction reached after `delta_secs` of unobstructed progress. A non-positive duration means
/// "instant": the fraction jumps to 1.0, with cost still charged through the regular clamp-and-pay flow.
fn advanced_fraction(fraction: f32, duration_secs: f32, delta_secs: f32) -> f32 {
    if duration_secs <= 0.0 {
        1.0
    } else {
        (fraction + delta_secs / duration_secs).min(1.0)
    }
}

/// Whole units of `cost` paid at `fraction` (`paid = floor(fraction * amount)`).
fn units_paid(fraction: f32, cost: &Cost) -> i32 {
    (fraction * cost.amount as f32).floor() as i32
}

/// Clamps `target` so no cost crosses a unit threshold the stock cannot cover — the research stalls
/// just before its first unaffordable unit. Never clamps below `fraction`: earned progress is kept.
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

/// Deducts the whole units of each cost crossed between `fraction` and `target`, which must already
/// be clamped to affordability.
fn pay_crossed_units(fraction: f32, target: f32, costs: &[Cost], stock: &mut Stock) {
    for cost in costs.iter() {
        let units = units_paid(target, cost) - units_paid(fraction, cost);
        if units > 0 {
            let removed = stock.try_remove(cost.resource_type, units);
            debug_assert!(removed, "clamp_to_affordable must keep crossed units payable");
        }
    }
}

/// Starts the requested research or switches the active slot to it, parking the previous one (its
/// progress is retained). Must run whether or not a research is currently active, so the current
/// active is an `Option<Single>`.
pub(crate) fn on_set_active_research_do_so(
    trigger: On<SetActiveResearch>,
    mut commands: Commands,
    current_active: Option<Single<Entity, With<ActiveResearch>>>,
    in_flight: Query<(), With<ResearchProgress>>,
    researches: Query<(Entity, &Research), (Without<Completed>, Without<Obsolete>)>,
) {
    let target = trigger.event().0;
    // Completed and obsolete researches are filtered out by the query — those markers are the single
    // source of truth (maintained reactively), so obsolescence is never recomputed here.
    let Some((entity, _)) = researches.iter().find(|(_, research)| research.0 == target) else { return };

    if let Some(current) = current_active {
        let current = *current;
        if current != entity {
            commands.entity(current).remove::<ActiveResearch>();
        }
    }
    if in_flight.get(entity).is_err() {
        commands.entity(entity).insert(ResearchProgress::default());
    }
    commands.entity(entity).insert(ActiveResearch);
    Log::info().player().tag(Tag::Research).message(format!("Research active: {target}"));
}

/// Parks the active research; its progress is retained. Runs only when a research is active.
pub(crate) fn on_stop_research_do_so(
    _trigger: On<StopResearch>,
    mut commands: Commands,
    active: Single<Entity, With<ActiveResearch>>,
) {
    commands.entity(*active).remove::<ActiveResearch>();
}

/// Generic obsolescence aggregation: a research is obsolete iff it has outcomes and *all* of them are
/// satisfied. It reads only the uniform `OutcomeSatisfied` marker, never an outcome kind. Idempotent
/// and reversible — clearing `Obsolete` when no longer all-satisfied is the un-obsolete path (e.g. a
/// future revocation), so a never-completed research can reappear.
pub(crate) fn on_check_for_obsoletion_do_so(
    trigger: On<CheckForObsoletion>,
    mut commands: Commands,
    satisfied: Query<(), With<OutcomeSatisfied>>,
    researches: Query<(&ResearchOutcomes, Has<Obsolete>), Without<Completed>>,
) {
    let research = trigger.research;
    let Ok((outcomes, is_obsolete)) = researches.get(research) else { return };
    let all_satisfied = !outcomes.is_empty()
        && outcomes.iter().all(|outcome| satisfied.get(outcome).is_ok());

    if all_satisfied && !is_obsolete {
        commands.entity(research).insert(Obsolete).remove::<ActiveResearch>();
    } else if !all_satisfied && is_obsolete {
        commands.entity(research).remove::<Obsolete>();
    }
}

/// Re-checks a research's obsolescence whenever one of its outcomes gains satisfaction.
pub(crate) fn on_insert_outcome_satisfied_recheck_obsoletion(
    trigger: On<Insert, OutcomeSatisfied>,
    mut commands: Commands,
    outcomes: Query<&ResearchOutcomeOf>,
) {
    if let Ok(outcome_of) = outcomes.get(trigger.entity) {
        commands.trigger(CheckForObsoletion { research: outcome_of.0 });
    }
}

/// Re-checks a research's obsolescence whenever one of its outcomes loses satisfaction (un-obsolete).
pub(crate) fn on_remove_outcome_satisfied_recheck_obsoletion(
    trigger: On<Remove, OutcomeSatisfied>,
    mut commands: Commands,
    outcomes: Query<&ResearchOutcomeOf>,
) {
    if let Ok(outcome_of) = outcomes.get(trigger.entity) {
        commands.trigger(CheckForObsoletion { research: outcome_of.0 });
    }
}
