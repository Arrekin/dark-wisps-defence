//! # Construction Menu
//!
//! Side-menu UI for placing buildings, towers, wisps, and map objects.
//! Buttons are built as BSN scene functions (`construct_menu_button`,
//! `construct_object_button`) and spawned in `SideMenu::setup`.

use bevy::color::palettes::css::WHITE;
use bevy::ecs::template::TemplateContext;
use bevy::prelude::*;
use bevy::ui::FocusPolicy;

use almanach::prelude::*;
use game_core::prelude::*;
use grids::placement::GridObjectPlacerRequest;
use states::{AdminMode, prelude::UiInteraction};

const NOT_HOVERED_ALPHA: f32 = 0.2;
const CONSTRUCT_MENU_BUTTON_WIDTH: f32 = 65.;
const CONSTRUCT_MENU_BUTTON_HEIGHT: f32 = 64.;
const COLOR_PLAYER_ZONE: Color = Color::srgb(0.188, 0.82, 0.82); // Turquoise
const COLOR_ADMIN_ZONE: Color = Color::srgb(0.8, 0.3, 0.1);

pub struct ConstructionMenuPlugin;
impl Plugin for ConstructionMenuPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Startup, (
                SideMenu::setup,
            ))
            .add_systems(Update, (
                AdminOnly::on_admin_mode_change_update_visibility.run_if(state_changed::<AdminMode>),
            ));
    }
}

/// Resolves an `ImageNode` from `Almanach` placement info at spawn time. `template()` is used rather
/// than a bare `ImageNode { image }` because the image handle isn't known statically — it must be
/// read from the `Almanach` resource when the scene is spawned.
fn placement_image(object_type: MapObject) -> impl Scene {
    template(move |context: &mut TemplateContext| {
        let almanach = context.resource::<Almanach>();
        let handle = almanach.get_placement_info_for(object_type).preview_image;
        Ok(ImageNode::new(handle.unwrap_or_default()))
    })
}

#[derive(Component, Default, Clone)]
#[require(Button)]
pub(crate) struct ButtonConstructMenu;

/// Builds one top-level side-menu button: an icon plus a fly-out `ConstructMenuListPicker` holding
/// `picker`, with any `extra` scene patch layered on (e.g. `AdminOnly`, or an `on(..)` handler).
///
/// A free function rather than a `ButtonConstructMenu` method so it can be called directly as a
/// scene function inside `bsn!`. Inside the macro a `Type::method` path is parsed as a component
/// constructor, not a scene call; a plain lowercase function name is parsed as a scene function.
fn construct_menu_button(icon_path: &'static str, picker: impl SceneList, extra: impl Scene) -> impl Scene {
    bsn! {
        ButtonConstructMenu
        Node {
            width: Val::Px(CONSTRUCT_MENU_BUTTON_WIDTH),
            height: Val::Px(CONSTRUCT_MENU_BUTTON_HEIGHT),
        }
        ImageNode {
            image: {icon_path},
            color: {WHITE.with_alpha(NOT_HOVERED_ALPHA)},
        }
        on(ButtonConstructMenu::on_mouse_over_highlight_menu_button)
        on(ButtonConstructMenu::on_mouse_out_unhighlight_menu_button)
        {extra}
        Children [
            (
                ConstructMenuListPicker
                Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    left: Val::Px(64.),
                    padding: UiRect {
                        left: Val::Px(2.5),
                        right: Val::Px(2.5),
                        top: Val::ZERO,
                        bottom: Val::ZERO,
                    },
                }
                Visibility::Hidden
                BackgroundColor(Color::BLACK)
                GlobalZIndex(-1)
                Children [ {picker} ]
            )
        ]
    }
}

impl ButtonConstructMenu {
    fn on_mouse_over_highlight_menu_button(
        trigger: On<Pointer<Over>>,
        mut menu_buttons: Query<(&mut ImageNode, &Children), With<ButtonConstructMenu>>,
        mut list_pickers: Query<&mut Visibility, With<ConstructMenuListPicker>>,
    ) -> Result<()> {
        let entity = trigger.entity;
        let (mut ui_image, children) = menu_buttons.get_mut(entity)?;
        let list_picker_entity = children.first().ok_or("List picker not found")?;
        let mut list_picker_visibility = list_pickers.get_mut(*list_picker_entity)?;
        ui_image.color.set_alpha(1.);
        *list_picker_visibility = Visibility::Inherited;
        Ok(())
    }

    fn on_mouse_out_unhighlight_menu_button(
        trigger: On<Pointer<Out>>,
        mut menu_buttons: Query<(&mut ImageNode, &Children), With<ButtonConstructMenu>>,
        mut list_pickers: Query<&mut Visibility, With<ConstructMenuListPicker>>,
    ) -> Result<()> {
        let entity = trigger.entity;
        let (mut ui_image, children) = menu_buttons.get_mut(entity)?;
        let list_picker_entity = children.first().ok_or("List picker not found")?;
        let mut list_picker_visibility = list_pickers.get_mut(*list_picker_entity)?;
        ui_image.color.set_alpha(NOT_HOVERED_ALPHA);
        *list_picker_visibility = Visibility::Hidden;
        Ok(())
    }
}

#[derive(Component, Default, Clone)]
struct AdminOnly;
impl AdminOnly {
    fn on_admin_mode_change_update_visibility(
        admin_mode: Res<State<AdminMode>>,
        mut menu_buttons: Query<&mut Visibility, With<AdminOnly>>,
    ) {
        let new_visibility = if matches!(admin_mode.get(), AdminMode::Enabled) { Visibility::Inherited } else { Visibility::Hidden };
        for mut visibility in menu_buttons.iter_mut() {
            *visibility = new_visibility;
        }
    }
}

/// Opens the research panel on click (the research icon is a panel toggle, not a placement-list button).
struct ResearchMenuButton;
impl ResearchMenuButton {
    fn on_click_open_research_panel(
        _trigger: On<Pointer<Click>>,
        mut next_ui_state: ResMut<NextState<UiInteraction>>,
    ) {
        next_ui_state.set(UiInteraction::ResearchPanel);
    }
}

#[derive(Component, Default, Clone)]
#[require(Button)]
pub(crate) struct ConstructMenuListPicker;

/// Marker for a placement button. Carries no data of its own — the placement type is captured by
/// the click observer in `construct_object_button` — so it stays a plain unit marker and picks up a
/// `FromTemplate` from the blanket `Clone + Default` impl, needing no hand-written template.
#[derive(Component, Default, Clone)]
#[require(Button, FocusPolicy)]
struct ConstructObjectButton;

/// Builds one placement button for `object_type`: a coloured 48×48 cell whose preview image is
/// resolved from the `Almanach` at spawn time (see `placement_image`). Free function for the same
/// reason as `construct_menu_button` — so it reads as a scene function inside `bsn!`.
fn construct_object_button(object_type: MapObject, background_color: Color) -> impl Scene {
    bsn! {
        ConstructObjectButton
        Node {
            width: Val::Px(48.),
            height: Val::Px(48.),
            margin: UiRect {
                left: Val::Px(2.5),
                right: Val::Px(2.5),
                top: Val::ZERO,
                bottom: Val::ZERO,
            },
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
        }
        BackgroundColor({background_color})
        on(move |_: On<Pointer<Click>>,
              mut grid_object_placer_request: ResMut<GridObjectPlacerRequest>,
              mut list_pickers: Query<(&mut Interaction, &mut Visibility), With<ConstructMenuListPicker>>| {
            grid_object_placer_request.set(object_type);
            list_pickers.iter_mut().for_each(|(mut interaction, mut visibility)| { *visibility = Visibility::Hidden; *interaction = Interaction::None; });
        })
        Children [
            (
                Node { width: Val::Px(46.0), height: Val::Px(46.0) }
                {placement_image(object_type)}
            )
        ]
    }
}

#[derive(Component, Default, Clone)]
struct SideMenu;
impl SideMenu {
    pub fn setup(mut commands: Commands) {
        commands.spawn_scene(bsn! {
            SideMenu
            Node {
                position_type: PositionType::Absolute,
                top: Val::Percent(30.),
                left: Val::Px(5.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
            }
            Children [
                construct_menu_button("ui/side_menu_towers.png", bsn_list![
                    construct_object_button(MapObject::Building(BuildingType::Tower(TowerType::Blaster)), COLOR_PLAYER_ZONE),
                    construct_object_button(MapObject::Building(BuildingType::Tower(TowerType::Cannon)), COLOR_PLAYER_ZONE),
                    construct_object_button(MapObject::Building(BuildingType::Tower(TowerType::RocketLauncher)), COLOR_PLAYER_ZONE),
                    construct_object_button(MapObject::Building(BuildingType::Tower(TowerType::Emitter)), COLOR_PLAYER_ZONE),
                    construct_object_button(MapObject::Building(BuildingType::Tower(TowerType::Field)), COLOR_PLAYER_ZONE),
                ], bsn!{}),
                construct_menu_button("ui/side_menu_buildings.png", bsn_list![
                    construct_object_button(MapObject::Building(BuildingType::EnergyRelay), COLOR_PLAYER_ZONE),
                    construct_object_button(MapObject::Building(BuildingType::MiningComplex), COLOR_PLAYER_ZONE),
                    construct_object_button(MapObject::Building(BuildingType::ExplorationCenter), COLOR_PLAYER_ZONE),
                    construct_object_button(MapObject::Building(BuildingType::Forge), COLOR_PLAYER_ZONE),
                ], bsn!{}),
                construct_menu_button("ui/side_menu_research.png", bsn_list![], bsn!{ on(ResearchMenuButton::on_click_open_research_panel) }),
                construct_menu_button("ui/side_menu_upgrades.png", bsn_list![], bsn!{}),
                construct_menu_button("ui/side_menu_consumables.png", bsn_list![], bsn!{}),
                construct_menu_button("ui/side_menu_admin_objects.png", bsn_list![
                    construct_object_button(MapObject::Building(BuildingType::MainBase), COLOR_ADMIN_ZONE),
                    construct_object_button(MapObject::DarkOre, COLOR_ADMIN_ZONE),
                    construct_object_button(MapObject::Wall, COLOR_ADMIN_ZONE),
                    construct_object_button(MapObject::QuantumField, COLOR_ADMIN_ZONE),
                ], bsn!{ AdminOnly }),
                construct_menu_button("ui/side_menu_admin_wisps.png", bsn_list![
                    construct_object_button(MapObject::Wisp(WispType::Fire), COLOR_ADMIN_ZONE),
                    construct_object_button(MapObject::Wisp(WispType::Water), COLOR_ADMIN_ZONE),
                    construct_object_button(MapObject::Wisp(WispType::Light), COLOR_ADMIN_ZONE),
                    construct_object_button(MapObject::Wisp(WispType::Electric), COLOR_ADMIN_ZONE),
                ], bsn!{ AdminOnly }),
            ]
        });
    }
}
