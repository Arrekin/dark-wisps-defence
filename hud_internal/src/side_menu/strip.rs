//! Flyout strips and Almanach-backed tile population for side-menu sections.

use bevy::prelude::*;

use almanach::prelude::*;
use game_core::prelude::*;
use states::AdminMode;

use super::section::{SectionLatch, SectionOffering};
use super::tile::PlacementTile;

/// Clearance between the tiles and the strip edge, and between one tile and the next.
pub(crate) const STRIP_TILE_INSET: f32 = 5.;

/// The strip's own edge, dimmer than a tile's.
pub(crate) const STRIP_EDGE_BRIGHTNESS: f32 = 0.15;

#[derive(Component, Default, Clone)]
#[require(Button)]
pub(crate) struct SideMenuStrip;

#[derive(Event, Clone, Copy)]
pub(crate) struct OfferingChanged;

pub(crate) fn trigger_offering_changed(mut commands: Commands) {
    commands.trigger(OfferingChanged);
}

/// Refills every offering-backed strip from the Almanach.
pub(crate) fn on_offering_changed_refill_strips(
    _trigger: On<OfferingChanged>,
    mut commands: Commands,
    almanach: Res<Almanach>,
    admin_mode: Res<State<AdminMode>>,
    sections: Query<(Entity, &SectionOffering, &SectionLatch, &Children)>,
) {
    let access = if admin_mode.get().is_enabled() { AccessPattern::Admin } else { AccessPattern::Player };

    for (section_entity, section_offering, latch, children) in sections.iter() {
        let Some(&strip_entity) = children.first() else { continue };

        let map_objects: Vec<MapObject> = match section_offering {
            SectionOffering::Towers => almanach.constructible_towers(access)
                .map(MapObject::Building)
                .collect(),
            SectionOffering::Buildings => almanach.constructible_buildings(access)
                .map(MapObject::Building)
                .collect(),
        };

        commands.entity(strip_entity).despawn_children();
        commands.entity(strip_entity).with_children(|strip| {
            for map_object in map_objects {
                strip.spawn(PlacementTile(map_object));
            }
        });

        // Reapply the latch after replacing the children so visibility and selection state use the
        // new tile entities.
        commands.entity(section_entity).insert(*latch);
    }
}
