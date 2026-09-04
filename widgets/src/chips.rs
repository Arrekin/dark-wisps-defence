//! Compact icon-and-text chips, horizontal chip strips, and data-specific chip behavior.
//!
//! [`Chip`] owns only the shared icon/text tree recorded in [`ChipChildren`]. Specialization
//! components update that tree and provide their own tooltips or visual states:
//!
//! - [`CostChip`] receives amounts from its owner; visual markers color its border by affordability.
//! - [`BuilderDisplayChip`] binds icon, name, and description to another entity's display components.
//!
//! Specializations are ordinary components and may be combined.
//!
//! # Usage
//! ```
//! // A cost, coloured green while the whole price is affordable.
//! parent.spawn((BuilderCostChip::from(cost), CostChipVisualFullPrice));
//!
//! // Bind a chip to an entity's display metadata.
//! parent.spawn(BuilderDisplayChip(outcome));
//! ```

use bevy::prelude::*;

use resources::prelude::{Cost, ResourceType};

// ============================================================================
// Strip
// ============================================================================

/// Spawn contract for a chip strip: a single scrollable row. Removed once the
/// widget has applied its layout — there is no runtime component, because there
/// is no runtime state.
///
/// Deliberately dumb. It does not know what a chip is and does not order its
/// children; callers spawn them in the order they want them.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct BuilderChipStrip;

/// Expands into a horizontal strip with one full-price affordability chip per cost.
#[derive(Component, Clone, Debug)]
pub struct BuilderFullPriceCostStrip(pub Vec<Cost>);

// ============================================================================
// Chip core
// ============================================================================

/// Spawn contract for a chip. Replaced by [`Chip`] once the widget has built
/// its tree.
///
/// Specializations expand into this rather than building their own tree, so
/// every chip has the same shape regardless of flavour.
#[derive(Component, Clone, Debug, Default)]
pub struct BuilderChip {
    pub icon: Handle<Image>,
    /// `None` spawns no text node at all — an icon-only chip, which is the
    /// shape a bound chip uses since its detail lives in the tooltip.
    pub text: Option<String>,
}

/// Runtime marker for a built chip. Carries no data: what a chip shows belongs
/// to whichever specialization is driving it.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Chip;

/// The nodes a chip spawned. Specializations write these directly; nothing is
/// searched for at runtime.
///
/// `text` is `None` when the chip was built icon-only.
#[derive(Component, Clone, Copy, Debug)]
pub struct ChipChildren {
    pub icon: Entity,
    pub text: Option<Entity>,
}

/// Fades the icon and label of every chip under this entity to `0.0`..=`1.0` of full
/// brightness. Put it on a [`BuilderChipStrip`] to fade a whole row, or on a single chip.
/// Remove it to go back to full brightness.
///
/// A chip's white icon and label are the brightest thing in it, so a strip of them will
/// stand out against a panel whose other content has been dimmed. The chip background and
/// border are left alone: the border is how a cost chip shows whether you can afford it,
/// and fading it here would overwrite that.
#[derive(Component, Clone, Copy, Debug)]
pub struct ChipsFaded(pub f32);

impl BuilderChip {
    pub fn new(icon: Handle<Image>) -> Self {
        Self { icon, text: None }
    }

    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }
}

// ============================================================================
// Cost specialization — content pushed by the owner
// ============================================================================

/// Spawn contract for a cost chip. Expands into [`BuilderChip`] plus the
/// runtime [`CostChip`].
#[derive(Component, Clone, Copy, Debug)]
pub struct BuilderCostChip {
    pub resource_type: ResourceType,
    pub amount: i32,
}

/// Runtime component. The owner writes `amount`; the specialization re-renders
/// the amount text and re-evaluates the border on change.
///
/// The widget never derives its own amount, because "what does this chip show"
/// has too many answers — a fixed recipe price, a research's remaining cost —
/// for it to pick one.
#[derive(Component, Clone, Copy, Debug)]
pub struct CostChip {
    pub resource_type: ResourceType,
    pub amount: i32,
}

/// Visual specialization: affordable when the stock covers the whole displayed
/// amount. For costs paid in one go — a forge recipe, a quantum field layer.
#[derive(Component, Clone, Copy, Debug)]
pub struct CostChipVisualFullPrice;

/// Visual specialization: affordable when the stock covers the next whole unit,
/// which is what a pay-as-you-go tick actually needs. An amount of 0 counts as
/// affordable — nothing is owed, so nothing blocks.
#[derive(Component, Clone, Copy, Debug)]
pub struct CostChipVisualUnitAvailable;

impl BuilderCostChip {
    pub fn new(resource_type: ResourceType, amount: i32) -> Self {
        Self { resource_type, amount }
    }
}

impl From<Cost> for BuilderCostChip {
    fn from(cost: Cost) -> Self {
        Self::new(cost.resource_type, cost.amount)
    }
}

// ============================================================================
// Display specialization — content bound to a subject entity
// ============================================================================

/// Spawn contract for a chip bound to `subject`. Expands into [`BuilderChip`]
/// plus the [`DisplayChipOf`] relationship, then is removed.
///
/// The subject supplies `DisplayIcon` for the chip and `DisplayName` /
/// `DisplayDescription` for the tooltip. Any subset may be absent.
#[derive(Component, Clone, Copy, Debug)]
pub struct BuilderDisplayChip(pub Entity);

/// Source side: "this chip shows subject S." Kept after the build so display
/// changes on the subject can be pushed into the chip.
///
/// The chip is also a `ChildOf` its UI container, and both lifetimes are safe:
/// the container despawning takes the chip, and the subject despawning takes it
/// too via `linked_spawn` on [`DisplayChips`].
#[derive(Component)]
#[relationship(relationship_target = DisplayChips)]
pub struct DisplayChipOf(pub Entity);

/// Target side: every chip currently showing this entity. There may be several —
/// the same subject can appear in more than one panel at once.
#[derive(Component)]
#[relationship_target(relationship = DisplayChipOf, linked_spawn)]
pub struct DisplayChips(Vec<Entity>);

/// The tooltip nodes a display chip spawned, so the sync systems can rewrite
/// one aspect without touching the others.
#[derive(Component, Clone, Copy, Debug)]
pub struct DisplayChipChildren {
    pub tooltip_title: Entity,
    pub tooltip_body: Entity,
}
