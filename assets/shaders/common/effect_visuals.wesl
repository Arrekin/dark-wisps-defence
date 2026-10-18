// Active effect data embedded in lane instances.

// Mirrors `EFFECT_VISUAL_SLOTS` in the alteration crate.
const EFFECT_VISUAL_SLOTS: u32 = 8u;

// Mirrors `BRITTLE_BIT` in alteration/src/effects/visual.rs.
const BRITTLE_BIT: u32 = 1u;

// Field order and types mirror `EffectVisuals` in visuals/src/effect_visuals.rs.
struct EffectVisuals {
    mask: u32, // one bit per active effect type
    params: array<vec4<f32>, EFFECT_VISUAL_SLOTS>,
};
