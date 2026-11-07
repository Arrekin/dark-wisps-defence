//! World depth combines a visual's altitude ([`DrawLevel`]) and render layer ([`ZDepth`]).
//! Each supported pair receives a separate depth range, ordered by altitude and then layer.
//! [`Band`] groups layers for shared scene snapshots and distortion passes.

use core::ops::{Index, IndexMut};

use bevy::prelude::*;
use strum::{EnumCount, EnumIter, VariantArray};

use crate::altitude::{FloorLevel, VolumeLevel};

/// Number of bits used for the entity's fractional depth within a layer.
const LAYER_OFFSET_BITS: u32 = 16;
const LAYER_OFFSET_STEPS: u32 = 1 << LAYER_OFFSET_BITS;

// The slot and fractional offset must fit together in f32's 24-bit significand.
// Above this limit, distinct offsets could round to the same depth.
const _: () = assert!(
    SLOT_TABLE.count + 1 < 1 << (24 - LAYER_OFFSET_BITS),
    "too many draw slots for LAYER_OFFSET_BITS: lower the bits, merge layers or narrow their draw levels",
);

/// A visual's altitude in the draw order.
///
/// Variants are ordered from bottom to top. All layers at one level draw before any layer
/// at the next level.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash, EnumIter, EnumCount, VariantArray)]
#[component(immutable)]
pub enum DrawLevel {
    UndergroundFloor,
    Underground,
    GroundFloor,
    Elevation,
    ElevationFloor,
    OnElevation,
    Airborne,
}

impl From<VolumeLevel> for DrawLevel {
    fn from(level: VolumeLevel) -> Self {
        match level {
            VolumeLevel::Underground => Self::Underground,
            VolumeLevel::Elevation => Self::Elevation,
            VolumeLevel::OnElevation => Self::OnElevation,
        }
    }
}
impl From<FloorLevel> for DrawLevel {
    fn from(level: FloorLevel) -> Self {
        match level {
            FloorLevel::UndergroundFloor => Self::UndergroundFloor,
            FloorLevel::GroundFloor => Self::GroundFloor,
            FloorLevel::ElevationFloor => Self::ElevationFloor,
        }
    }
}

/// A set of draw levels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrawLevels(u8);
impl DrawLevels {
    pub const FLOORS: Self = Self::only(DrawLevel::UndergroundFloor)
        .with(DrawLevel::GroundFloor)
        .with(DrawLevel::ElevationFloor);
    pub const VOLUMES: Self = Self::only(DrawLevel::Underground)
        .with(DrawLevel::Elevation)
        .with(DrawLevel::OnElevation);

    pub const fn only(level: DrawLevel) -> Self {
        Self(1 << level as u8)
    }
    pub const fn with(self, level: DrawLevel) -> Self {
        Self(self.0 | Self::only(level).0)
    }
    pub const fn contains(self, level: DrawLevel) -> bool {
        self.0 & Self::only(level).0 != 0
    }
}

/// Sentinel for an unsupported layer/level pair in [`SlotTable::slots`].
const NO_SLOT: u8 = u8::MAX;
const SLOT_TABLE: SlotTable = SlotTable::build();

/// Depth slots for supported layer/level pairs, computed at compile time.
struct SlotTable {
    /// Indexed by level, then layer. Slots follow [`DrawLevel`] order, then [`ZDepth`] order.
    slots: [[u8; ZDepth::COUNT]; DrawLevel::COUNT],
    count: u32,
    /// First slot belonging to each band, across all draw levels.
    band_bases: [u8; Band::COUNT],
}
impl SlotTable {
    const fn build() -> Self {
        let mut table = Self { slots: [[NO_SLOT; ZDepth::COUNT]; DrawLevel::COUNT], count: 0, band_bases: [NO_SLOT; Band::COUNT] };
        let mut level = 0;
        while level < DrawLevel::COUNT {
            let mut layer = 0;
            while layer < ZDepth::COUNT {
                let z_depth = ZDepth::VARIANTS[layer];
                if z_depth.draw_levels().contains(DrawLevel::VARIANTS[level]) {
                    table.slots[level][layer] = table.count as u8;
                    let band = z_depth.band() as usize;
                    if table.band_bases[band] == NO_SLOT {
                        table.band_bases[band] = table.count as u8;
                    }
                    table.count += 1;
                }
                layer += 1;
            }
            level += 1;
        }
        let mut band = 0;
        while band < Band::COUNT {
            assert!(table.band_bases[band] != NO_SLOT, "every band holds at least one layer");
            band += 1;
        }
        table
    }
}

/// World-space render layer.
///
/// Variant order determines draw order within a [`DrawLevel`]. A visual needs both components,
/// and its level must be supported by [`ZDepth::draw_levels`]. Each supported layer/level pair
/// occupies a z range of width 1.0. The entity index selects a stable fractional offset within
/// that range to reduce depth ties.
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
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, EnumIter, EnumCount, VariantArray)]
#[component(immutable)]
#[require(Transform)]
pub enum ZDepth {
    MapGround,
    OverlayEmissions,
    DarkOre,
    Obstacle,
    FloorCover,
    FloorDecal,
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
    // The sea's shadow falls on everything in the world beneath it.
    VoidSeaShadow,
    // The world lies inside the sea, so nothing in it draws over the sea.
    VoidSea,
}
impl ZDepth {
    /// Draw levels supported by this layer.
    pub const fn draw_levels(self) -> DrawLevels {
        match self {
            Self::MapGround
            | Self::OverlayEmissions
            | Self::DarkOre => DrawLevels::only(DrawLevel::GroundFloor),
            Self::FloorCover
            | Self::FloorDecal
            | Self::FireballScorch
            | Self::FlameScorch => DrawLevels::FLOORS,
            Self::OverlayEnergySupply
            | Self::OverlayTowerRanges => DrawLevels::only(DrawLevel::Elevation),
            Self::Obstacle
            | Self::FlameGlow
            | Self::Building
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
            | Self::MapUi => DrawLevels::VOLUMES,
            Self::AirEffect
            | Self::AerialUnit
            | Self::ProjectileUnder
            | Self::Projectile
            | Self::FireballBody
            | Self::GridPlacerGhost
            | Self::GridPlacer
            | Self::VoidSeaShadow
            | Self::VoidSea => DrawLevels::only(DrawLevel::Airborne),
        }
    }

    /// Lowest world z of this layer at `level`. Depth slots start at 1.0.
    ///
    /// Panics if this layer does not support `level`.
    pub fn base_z(self, level: DrawLevel) -> f32 {
        let slot = SLOT_TABLE.slots[level as usize][self as usize];
        assert!(slot != NO_SLOT, "{self:?} cannot draw at {level:?}");
        (slot as u32 + 1) as f32
    }

    /// World z of `entity` on this layer at `level`.
    pub fn world_z(self, level: DrawLevel, entity: Entity) -> f32 {
        self.base_z(level) + (entity.index_u32() % LAYER_OFFSET_STEPS) as f32 / LAYER_OFFSET_STEPS as f32
    }

    pub const fn band(self) -> Band {
        match self {
            Self::MapGround
            | Self::OverlayEmissions
            | Self::DarkOre
            | Self::Obstacle
            | Self::FloorCover
            | Self::FloorDecal
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
            | Self::GridPlacer
            | Self::VoidSeaShadow
            | Self::VoidSea => Band::Placement,
        }
    }
}

/// A group of render layers sharing one scene snapshot and distortion pass.
///
/// Layers are contiguous in [`ZDepth`] order, but may repeat at several draw levels.
/// The cut occurs once, before the band's lowest depth across all levels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, EnumIter, EnumCount)]
pub enum Band {
    Ground,
    Surface,
    Air,
    Placement,
}
impl Band {
    /// Lowest world z occupied by this band across all draw levels.
    pub const fn base_z(self) -> f32 {
        (SLOT_TABLE.band_bases[self as usize] as u32 + 1) as f32
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
    use strum::IntoEnumIterator;

    use super::*;

    #[test]
    fn bands_are_contiguous_in_layer_order() {
        let bands: Vec<Band> = ZDepth::iter().map(ZDepth::band).collect();
        assert!(bands.is_sorted(), "a layer is ordered into the middle of another band: {bands:?}");
    }
}
