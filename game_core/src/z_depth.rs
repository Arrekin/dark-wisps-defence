use core::ops::{Index, IndexMut};

use bevy::prelude::*;
use strum::{EnumCount, EnumIter, IntoEnumIterator};

/// Number of bits used for the entity's fractional depth within a layer.
const LAYER_OFFSET_BITS: u32 = 16;
const LAYER_OFFSET_STEPS: u32 = 1 << LAYER_OFFSET_BITS;

// A layer's world z is exact in f32 only while its integer part and the offset fit the 24-bit
// significand together. More layers than this would make neighbouring offsets collapse.
const _: () = assert!(
    (ZDepth::COUNT as u32 + 1) < 1 << (24 - LAYER_OFFSET_BITS),
    "too many ZDepth layers for LAYER_OFFSET_BITS: lower the bits or merge layers",
);

/// World-space render layer.
///
/// Variant order is draw order. Each layer occupies a z range of width 1.0. The entity index
/// selects a stable fractional offset within that range, reducing depth ties within a layer.
/// Use generic layers unless a visual needs a specific draw order or a dedicated layer for batching.
///
/// Bevy batches adjacent paint instances of the same lane kind in the sorted phase.
/// A dedicated layer keeps those instances together; a shared layer can split them into several draws.
///
/// An insertion observer sets local z, which gives the correct world depth for root entities.
/// Each frame, `enforce_zdepth_world_z` runs after transform propagation in `PostUpdate` and
/// sets the z translation of `GlobalTransform` to [`ZDepth::world_z`].
///
/// Movement systems should write only x/y and read depth through this component.
/// A child with its own `ZDepth` keeps its assigned world depth while inheriting the parent's
/// x/y, rotation and scale. Children without `ZDepth` use normal transform propagation;
/// they do not receive the parent's subsequent depth correction.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, EnumIter, EnumCount)]
#[component(immutable)]
#[require(Transform)]
pub enum ZDepth {
    MapGround,
    OverlayEmissions,
    DarkOre,
    Obstacle,
    GroundDecal,
    FireballScorch,
    FlameScorch,
    FlameGlow,
    OverlayEnergySupply,
    OverlayTowerRanges,
    Building,
    BuildingEffect,
    WispFire,
    WispWater,
    WispLight,
    WispElectric,
    GroundEffect,
    FireballGlow,
    FireballBlast,
    FlameJet,
    FlameImpact,
    FlamePilot,
    TowerTop,
    MapUi,
    AirEffect,
    AerialUnit,
    ProjectileUnder,
    Projectile,
    FireballBody,
    // Keep the placement ghost above the map and its validity overlay above the ghost.
    GridPlacerGhost,
    GridPlacer,
}
impl ZDepth {
    /// Lowest world z of this layer. The first layer starts at 1.0, above the default z of 0.0.
    pub fn base_z(self) -> f32 {
        (self as u32 + 1) as f32
    }

    /// World z of `entity` on this layer.
    pub fn world_z(self, entity: Entity) -> f32 {
        self.base_z() + (entity.index_u32() % LAYER_OFFSET_STEPS) as f32 / LAYER_OFFSET_STEPS as f32
    }

    pub fn band(self) -> Band {
        match self {
            Self::MapGround
            | Self::OverlayEmissions
            | Self::DarkOre
            | Self::Obstacle
            | Self::GroundDecal
            | Self::FireballScorch
            | Self::FlameScorch
            | Self::FlameGlow
            | Self::OverlayEnergySupply
            | Self::OverlayTowerRanges => Band::Ground,
            Self::Building
            | Self::BuildingEffect
            | Self::WispFire
            | Self::WispWater
            | Self::WispLight
            | Self::WispElectric
            | Self::GroundEffect
            | Self::FireballGlow
            | Self::FireballBlast
            | Self::FlameJet
            | Self::FlameImpact
            | Self::FlamePilot
            | Self::TowerTop
            | Self::MapUi => Band::Surface,
            Self::AirEffect
            | Self::AerialUnit
            | Self::ProjectileUnder
            | Self::Projectile
            | Self::FireballBody => Band::Air,
            Self::GridPlacerGhost
            | Self::GridPlacer => Band::Placement,
        }
    }

    /// Layers of `band`, in draw order.
    pub fn of_band(band: Band) -> impl Iterator<Item = Self> {
        Self::iter().filter(move |layer| layer.band() == band)
    }
}

/// Contiguous range of render layers. A cut before the band can distort or capture
/// the scene drawn by lower bands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, EnumIter, EnumCount)]
pub enum Band {
    Ground,
    Surface,
    Air,
    Placement,
}
impl Band {
    /// Lowest world z of the band: the base of its first layer.
    pub fn base_z(self) -> f32 {
        ZDepth::of_band(self).next().expect("every band holds at least one layer").base_z()
    }
}

/// One `T` per [`Band`], indexed by band.
#[derive(Clone, Debug, Default)]
pub struct PerBand<T>([T; Band::COUNT]);
impl<T> PerBand<T> {
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.0.iter_mut()
    }
}
impl<T> Index<Band> for PerBand<T> {
    type Output = T;
    fn index(&self, band: Band) -> &T {
        &self.0[band as usize]
    }
}
impl<T> IndexMut<Band> for PerBand<T> {
    fn index_mut(&mut self, band: Band) -> &mut T {
        &mut self.0[band as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bands_are_contiguous_in_layer_order() {
        let bands: Vec<Band> = ZDepth::iter().map(ZDepth::band).collect();
        assert!(bands.is_sorted(), "a layer is ordered into the middle of another band: {bands:?}");
    }
}
