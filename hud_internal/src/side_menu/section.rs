//! Side-menu section icons, flyout-strip ownership, and placement latching.
//!
//! Each icon owns one strip. Sections with fixed content provide its tiles directly; offering-backed
//! sections populate it from the Almanach.

use bevy::color::palettes::css::WHITE;
use bevy::ecs::component::ComponentIdFor;
use bevy::picking::hover::Hovered;
use bevy::prelude::*;

use game_core::prelude::MapObject;
use grids::placement::{GridObjectPlacer, StartPlacing, StopPlacing};
use states::{AdminMode, prelude::UiInteraction};

use widgets::prelude::{BuilderVoidPanel, VoidPanel};

use super::strip::{STRIP_EDGE_BRIGHTNESS, STRIP_TILE_INSET, SideMenuStrip};
use super::tile::{PlacementTile, Tile};

// Section frame
const NOT_HOVERED_ALPHA: f32 = 0.2;
pub(crate) const SIDE_MENU_SECTION_SIZE: f32 = 64.;

/// Overlaps the icon by 1px so crossing into the strip does not interrupt section hover.
const STRIP_LEFT: f32 = SIDE_MENU_SECTION_SIZE - 1.;

#[derive(Component, Default, Clone)]
#[require(Button, Hovered, SectionLatch)]
pub(crate) struct SideMenuSection;

/// Keeps a section open during placement. `OnObject` identifies the only tile shown after the
/// pointer leaves the section.
#[derive(Component, Clone, Copy, Default, PartialEq, Eq)]
#[component(immutable)]
pub(crate) enum SectionLatch {
    #[default]
    Free,
    OnObject(MapObject),
}

/// Selects the Almanach catalog used to populate a dynamic section.
#[derive(Component, Clone, Copy, Default)]
pub(crate) enum SectionOffering {
    #[default]
    Towers,
    Buildings,
}

/// Builds a section icon and its flyout strip. `extra` attaches section-specific markers or
/// observers.
///
/// This remains a free function because `bsn!` parses `Type::method` as a component constructor.
pub(crate) fn side_menu_section(icon_path: &'static str, content: impl SceneList, extra: impl Scene) -> impl Scene {
    bsn! {
        SideMenuSection
        Node {
            width: Val::Px(SIDE_MENU_SECTION_SIZE),
            height: Val::Px(SIDE_MENU_SECTION_SIZE),
        }
        ImageNode {
            image: {icon_path},
            color: {WHITE.with_alpha(NOT_HOVERED_ALPHA)},
        }
        {extra}
        Children [
            (
                SideMenuStrip
                Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    left: {Val::Px(STRIP_LEFT)},
                    padding: UiRect::all(Val::Px(STRIP_TILE_INSET)),
                    column_gap: Val::Px(STRIP_TILE_INSET),
                }
                Visibility::Hidden
                GlobalZIndex(-1)
                {template_value(
                    BuilderVoidPanel::default()
                        .with_corner_cut(0.)
                        .with_edge_brightness(STRIP_EDGE_BRIGHTNESS)
                        .with_rim_intensity(0.)
                        .with_hairline_strength(0.)
                )}
                Children [ {content} ]
            )
        ]
    }
}

/// A section is raised — icon lit, strip shown — while the pointer is inside it or while it is
/// latched.
///
/// `Hovered` includes descendants, so it remains set while the pointer crosses the icon, strip,
/// and tiles.
pub(crate) fn on_insert_section_state_manage_strip(
    trigger: On<Insert, (Hovered, SectionLatch)>,
    latch_id: ComponentIdFor<SectionLatch>,
    mut sections: Query<(&Hovered, &SectionLatch, &mut ImageNode, &Children), With<SideMenuSection>>,
    mut strips: Query<(&Children, &mut Visibility), With<SideMenuStrip>>,
    placement_tiles: Query<&PlacementTile>,
    mut all_tile_nodes: Query<&mut Node, With<Tile>>,
    mut all_tile_panels: Query<&mut VoidPanel, With<Tile>>,
) {
    let Ok((hovered, latch, mut icon, children)) = sections.get_mut(trigger.entity) else { return };
    let Some(&strip_entity) = children.first() else { return };
    let hovered = hovered.get();
    let raised = hovered || *latch != SectionLatch::Free;

    icon.color.set_alpha(if raised { 1.0 } else { NOT_HOVERED_ALPHA });

    let Ok((strip_children, mut strip_visibility)) = strips.get_mut(strip_entity) else { return };
    let wanted = if raised { Visibility::Inherited } else { Visibility::Hidden };
    if *strip_visibility != wanted {
        *strip_visibility = wanted;
    }

    // A latch insertion starts, stops, or restores a placement state. Collapse immediately around
    // an active object even if the pointer is still inside the section.
    let latch_was_set = trigger.trigger().components.contains(&latch_id.get());

    let latched_to = match *latch {
        SectionLatch::OnObject(map_object) => Some(map_object),
        SectionLatch::Free => None,
    };
    // The surge marks the latched object whether or not the strip is collapsed to it.
    let collapsed_to = latched_to.filter(|_| latch_was_set || !hovered);

    for tile_entity in strip_children.iter() {
        let tile_object = placement_tiles.get(tile_entity).ok().map(|tile| tile.0);

        if let Ok(mut node) = all_tile_nodes.get_mut(tile_entity) {
            let shown = collapsed_to.is_none_or(|placed| tile_object == Some(placed));
            let wanted = if shown { Display::Flex } else { Display::None };
            if node.display != wanted {
                node.display = wanted;
            }
        }
        if let Ok(mut panel) = all_tile_panels.get_mut(tile_entity) {
            panel.set_border_surge(tile_object.is_some() && tile_object == latched_to);
        }
    }
}

/// Latches the section containing the active object. Object identity keeps the latch valid across
/// placement shortcuts and tile rebuilds.
pub(crate) fn on_start_placing_latch_owning_section(
    _trigger: On<StartPlacing>,
    mut commands: Commands,
    placer: Single<&GridObjectPlacer>,
    sections: Query<(Entity, &SectionLatch, &Children), With<SideMenuSection>>,
    strips: Query<&Children, With<SideMenuStrip>>,
    tiles: Query<&PlacementTile>,
) {
    let Some(placed) = placer.map_object() else { return };

    for (section_entity, latch, children) in sections.iter() {
        let holds_tile = children.first()
            .and_then(|&strip_entity| strips.get(strip_entity).ok())
            .is_some_and(|strip_children| strip_children.iter()
                .any(|tile_entity| tiles.get(tile_entity).is_ok_and(|tile| tile.0 == placed)));

        let wanted = if holds_tile { SectionLatch::OnObject(placed) } else { SectionLatch::Free };
        if *latch != wanted {
            commands.entity(section_entity).insert(wanted);
        }
    }
}

/// Clicking the section latched to the running placement cancels the session.
pub(crate) fn on_click_section_cancel_placement(
    trigger: On<Pointer<Click>>,
    mut next_ui_state: ResMut<NextState<UiInteraction>>,
    sections: Query<&SectionLatch, With<SideMenuSection>>,
) {
    let Ok(latch) = sections.get(trigger.entity) else { return };
    if matches!(latch, SectionLatch::OnObject(_)) {
        next_ui_state.set(UiInteraction::Free);
    }
}

pub(crate) fn on_stop_placing_release_owning_section(
    _trigger: On<StopPlacing>,
    mut commands: Commands,
    sections: Query<(Entity, &SectionLatch), With<SideMenuSection>>,
) {
    for (section_entity, latch) in sections.iter() {
        if matches!(latch, SectionLatch::OnObject(_)) {
            commands.entity(section_entity).insert(SectionLatch::Free);
        }
    }
}

#[derive(Component, Default, Clone)]
pub(crate) struct AdminSection;
impl AdminSection {
    pub(crate) fn on_admin_mode_change_update_visibility(
        admin_mode: Res<State<AdminMode>>,
        mut sections: Query<&mut Visibility, With<AdminSection>>,
    ) {
        let new_visibility = if admin_mode.get().is_enabled() { Visibility::Inherited } else { Visibility::Hidden };
        for mut visibility in sections.iter_mut() {
            *visibility = new_visibility;
        }
    }
}

/// Opens the research panel on click. The research icon is a panel toggle, not a strip of
/// placeable tiles.
pub(crate) fn on_click_open_research_panel(
    _trigger: On<Pointer<Click>>,
    mut next_ui_state: ResMut<NextState<UiInteraction>>,
) {
    next_ui_state.set(UiInteraction::ResearchPanel);
}
