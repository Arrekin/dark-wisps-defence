//! What only the card following the running research does: the border carries surges, and
//! every unit of resource the research pays throws a glyph from the card's edge into its
//! progress bar.
//!
//! The card is found by the source it was bound to — a view carrying
//! `ResearchDetailViewSource<ResearchActive>` is the one showing whatever research is
//! running. No separate marker is involved; the binding already says which card this is.

use bevy::{prelude::*, ui::UiGlobalTransform};
use nanorand::Rng;

use game_core::prelude::ContentId;
use research::prelude::{ResearchActive, ResearchRuntime, ResearchUnitPaid};
use states::prelude::UiInteraction;
use widgets::prelude::{BuilderRune, RuneFlight, VoidPanel};

use super::view::{ResearchDetailView, ResearchDetailViewContent, ResearchDetailViewSource};

pub(crate) struct ActiveDetailViewPlugin;
impl Plugin for ActiveDetailViewPlugin {
    fn build(&self, app: &mut App) {
        app
            // The panel remains spawned while closed; avoid spawning runes until it is visible.
            .add_observer(on_research_unit_paid_spawn_rune.run_if(in_state(UiInteraction::ResearchPanel)))
            .add_observer(on_add_research_active_surge_card)
            .add_observer(on_remove_research_active_still_card);
    }
}

// ============================================================================
// CONSTANTS
// ============================================================================

/// Seconds a paid-unit rune takes to cross from the card's edge to the bar.
const RUNE_FLIGHT_DURATION: f32 = 0.9;
/// Sideways bend of the flight path, in pixels, so a burst of runes reads as a
/// stream instead of every glyph retracing the same line.
const RUNE_FLIGHT_CURVE: f32 = 30.0;
/// Glyph size in pixels.
const RUNE_SIZE: f32 = 14.0;
/// Half-range of each glyph's random tilt, in radians.
const RUNE_TILT_RANGE: f32 = 0.25;
/// How many distinct glyphs one research draws from. Small enough that the set becomes
/// familiar while watching a single research.
const RUNE_ALPHABET_SIZE: u32 = 6;

// ============================================================================
// BORDER SURGE
// ============================================================================

/// Turns the card's border surges on and off with the research it follows. The card is bound
/// to `ResearchActive`, so the marker arriving and leaving is exactly when the card gains and
/// loses its subject.
fn on_add_research_active_surge_card(
    _: On<Add, ResearchActive>,
    mut card: Single<&mut VoidPanel, With<ResearchDetailViewSource<ResearchActive>>>,
) {
    card.set_border_surge(true);
}

fn on_remove_research_active_still_card(
    _: On<Remove, ResearchActive>,
    mut card: Single<&mut VoidPanel, With<ResearchDetailViewSource<ResearchActive>>>,
) {
    card.set_border_surge(false);
}

// ============================================================================
// RUNES
// ============================================================================

fn on_research_unit_paid_spawn_rune(
    trigger: On<ResearchUnitPaid>,
    mut commands: Commands,
    views: Query<&ResearchDetailView, With<ResearchDetailViewSource<ResearchActive>>>,
    contents: Query<&ResearchDetailViewContent>,
    researches: Query<(&ResearchRuntime, &ContentId)>,
    nodes: Query<(&ComputedNode, &UiGlobalTransform)>,
    mut next_letter: Local<u32>,
) {
    let paid_research = trigger.research;
    let Ok((runtime, content_id)) = researches.get(paid_research) else { return };

    for view in views.iter() {
        let content_entity = view.content;
        let Ok(content) = contents.get(content_entity) else { continue };
        if content.research != paid_research { continue }

        let Ok((parent_node, parent_transform)) = nodes.get(content_entity) else { continue };
        let Ok((bar_node, bar_transform)) = nodes.get(content.progress_bar) else { continue };

        let flight = rune_flight(parent_node, parent_transform, bar_node, bar_transform, runtime.progress);

        *next_letter += 1;
        let mut rng = nanorand::tls_rng();
        let tilt = (rng.generate::<f32>() * 2.0 - 1.0) * RUNE_TILT_RANGE;

        commands.entity(content_entity).with_child(
            BuilderRune::new(rune_seed(content_id, *next_letter), flight)
                .with_size(RUNE_SIZE)
                .with_tilt(tilt),
        );
    }
}

// ============================================================================
// GEOMETRY
// ============================================================================

/// The flight from a random point on `parent`'s border to `bar`'s leading edge
/// at `progress`, vertically centred — both expressed in pixels local to
/// `parent`'s top-left, matching where the rune is spawned.
///
/// `UiGlobalTransform` stores each node's center in physical pixels; `Node`
/// style values (and so `RuneFlight`'s `from`/`to`) are logical pixels, so
/// every physical point is scaled by `ComputedNode::inverse_scale_factor`
/// before use.
fn rune_flight(
    parent_node: &ComputedNode,
    parent_transform: &UiGlobalTransform,
    bar_node: &ComputedNode,
    bar_transform: &UiGlobalTransform,
    progress: f32,
) -> RuneFlight {
    let scale = parent_node.inverse_scale_factor();

    let parent_top_left_physical = parent_transform.transform_point2(parent_node.size() * -0.5);
    let parent_top_left = parent_top_left_physical * scale;

    let bar_half = bar_node.size() * 0.5;
    let leading_local = Vec2::new(bar_node.size().x * progress.clamp(0.0, 1.0) - bar_half.x, 0.0);
    let leading_physical = bar_transform.transform_point2(leading_local);
    let to = leading_physical * scale - parent_top_left;

    let mut rng = nanorand::tls_rng();
    let from = random_border_point(&mut rng, parent_node.size() * scale, RUNE_SIZE * 0.5);

    RuneFlight { from, to, duration: RUNE_FLIGHT_DURATION, curve: RUNE_FLIGHT_CURVE }
}

/// A random point on the border of a `size`-sized rect anchored at the origin
/// — which is a parent-local pixel position when `size` is that parent's own.
///
/// `inset` pulls the border in on every side. The content node clips on Y, so a glyph
/// centred exactly on the top edge would be drawn with its upper half cut away; half a
/// glyph of inset keeps it whole from the first frame.
fn random_border_point<const OUTPUT: usize>(
    rng: &mut impl Rng<OUTPUT>,
    size: Vec2,
    inset: f32,
) -> Vec2 {
    let span = (size - Vec2::splat(inset * 2.0)).max(Vec2::ZERO);
    let along = rng.generate::<f32>();
    let corner = Vec2::splat(inset);
    corner + match rng.generate_range(0u8..4) {
        0 => Vec2::new(along * span.x, 0.0),
        1 => Vec2::new(span.x, along * span.y),
        2 => Vec2::new((1.0 - along) * span.x, span.y),
        _ => Vec2::new(0.0, (1.0 - along) * span.y),
    }
}

/// Picks one letter of a research's own alphabet.
///
/// The letter index chooses which of the six; the authored id decides what those six look
/// like, so a research shows the same glyphs in every session.
///
/// The hash is spelled out here because the mapping has to survive saves and toolchain
/// upgrades: change it and every research is given a new set of glyphs.
fn rune_seed(content: &ContentId, letter: u32) -> u32 {
    let mut base: u32 = 0x811C_9DC5;
    for byte in content.0.as_bytes() {
        base ^= *byte as u32;
        base = base.wrapping_mul(0x0100_0193);
    }

    let mut x = base ^ (letter % RUNE_ALPHABET_SIZE);
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^= x >> 16;
    x
}
