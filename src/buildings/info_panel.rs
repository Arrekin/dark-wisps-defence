use bevy::color::palettes::css::{BLUE, WHITE};
use lib_ui::prelude::{Healthbar, UpgradeLineBuilder};

use crate::prelude::*;
use crate::ui::display_info_panel::{DisplayPanelMainContentRoot, FocusedMapObject};

pub struct InfoPanelPlugin;
impl Plugin for InfoPanelPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(PostStartup, initialize_building_panel_content_system)
            .add_systems(Update, update_building_info_panel_system.run_if(in_state(UiInteraction::DisplayInfoPanel)))
            .add_observer(on_focused_map_object_insert)
            .add_observer(on_building_info_panel_enabled_for_towers_trigger)
            .add_observer(BuildingInfoPanelTowerUpgradeCountText::refresh_upgrade_count_on::<BuildingInfoPanelEnabledTrigger, ()>) // Refresh upgrade text on panel enabled
            .add_observer(BuildingInfoPanelTowerUpgradeCountText::refresh_upgrade_count_on::<LevelUpUpgradeAppliedEvent, ()>) // Refresh upgrade text after upgrade applied
            .add_observer(BuildingInfoPanelDisableButton::on_add)
            .add_observer(BuildingInfoPanelDestroyButton::on_add)
            ;
    }
}

/// Common
#[derive(Component)]
pub struct BuildingInfoPanel;
#[derive(Component)]
pub struct BuildingInfoPanelNameText;
#[derive(Component)]
pub struct BuildingInfoPanelHealthbar;
#[derive(EntityEvent)]
pub struct BuildingInfoPanelEnabledTrigger { pub entity: Entity }

// Tower Subpanel
#[derive(Component)]
struct BuildingInfoPanelTowerRoot;
#[derive(Component)]
struct BuildingInfoPanelTowerUpgradeCountText;
impl BuildingInfoPanelTowerUpgradeCountText {
    fn refresh_upgrade_count_on<T: Event, B: Bundle> (
        _trigger: On<T, B>,
        focused_tower: Single<&Upgrades, With<FocusedMapObject>>,
        upgrade_count_text: Single<&mut Text, With<BuildingInfoPanelTowerUpgradeCountText>>,
    ) {
        let upgrades = focused_tower.into_inner();
        let purchased = upgrades.total_upgrades_purchased();
        let available = upgrades.total_upgrades_available();

        upgrade_count_text.into_inner().0 = format!("--- Upgrades {} / {} ---", purchased, available);
    }
}
#[derive(Component)]
struct BuildingInfoPanelTowerUpgradesContainer;

fn update_building_info_panel_system(
    focused_building: Single<&Health, (With<FocusedMapObject>, With<Building>)>,
    healthbar: Single<&mut Healthbar, With<BuildingInfoPanelHealthbar>>,
) {
    let health = focused_building.into_inner();
    // Update the healthbar
    let mut healthbar = healthbar.into_inner();
    healthbar.value = health.get_current() as f32;
    healthbar.max_value = health.get_max() as f32;
    let health_percentage = health.get_percent();
    healthbar.color = Color::linear_rgba(1. - health_percentage, health_percentage, 0., 1.);
}

fn on_focused_map_object_insert(
    trigger: On<Insert, FocusedMapObject>,
    mut commands: Commands,
    almanach: Res<Almanach>,
    building_name_text: Single<&mut Text, With<BuildingInfoPanelNameText>>,
    building_panel_entity: Single<Entity, With<BuildingInfoPanel>>,
    disable_button_entity: Single<Entity, With<BuildingInfoPanelDisableButton>>,
    disable_button_icon: Single<&mut ImageNode, With<BuildingInfoPanelDisableButtonIcon>>,
    destroy_button_entity: Single<Entity, With<BuildingInfoPanelDestroyButton>>,
    buildings: Query<&BuildingType>,
    disabled_by_player: Query<(), With<DisabledByPlayer>>,
    mut nodes: Query<&mut Node>,
) {
    let focused_entity = trigger.entity;
    let Ok(mut building_panel) = nodes.get_mut(building_panel_entity.into_inner()) else { return; };
    let Ok(building_type) = buildings.get(focused_entity) else { 
        building_panel.display = Display::None;
        return; 
    };
    
    building_panel.display = Display::Flex;
    commands.trigger(BuildingInfoPanelEnabledTrigger { entity: focused_entity });

    // Update the building name
    building_name_text.into_inner().0 = almanach.get_building_info(*building_type).name.to_string();

    // Manage the Disable button
    let is_main_base = matches!(building_type, &BuildingType::MainBase);
    if let Ok(mut disable_button) = nodes.get_mut(disable_button_entity.into_inner()) {
        disable_button.display = if is_main_base { 
            // MainBase cannot be disabled
            Display::None 
        } else { 
            let is_disabled = disabled_by_player.contains(focused_entity);
            disable_button_icon.into_inner().color.set_alpha(if is_disabled { 1.0 } else { 0.35 });
            Display::Flex 
        };
    }

    // Manage the Destroy button
    if let Ok(mut destroy_button) = nodes.get_mut(destroy_button_entity.into_inner()) {
        destroy_button.display = if is_main_base { 
            // MainBase cannot be destroyed
            Display::None 
        } else { 
            Display::Flex 
        };
    }
}

pub fn initialize_building_panel_content_system(
    mut commands: Commands,
    display_info_panel_main_content_root: Single<Entity, With<DisplayPanelMainContentRoot>>,
) {
    commands.entity(display_info_panel_main_content_root.into_inner()).with_children(|parent| {
        parent.spawn((
            Node {
                display: Display::None,
                height: Val::Percent(100.),
                width: Val::Percent(100.),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Start,
                align_items: AlignItems::Start,
                padding: UiRect::all(Val::Px(2.0)),
                ..default()
            },
            BuildingInfoPanel,
            children![
                // Top line of the panel
                (
                    Node {
                        width: Val::Percent(100.),
                        flex_direction: FlexDirection::Row,
                        justify_content: JustifyContent::Start,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    children![
                        // Building name
                        (
                            Text::new("### Building Name ###"),
                            TextColor::from(BLUE),
                            TextLayout::new_with_linebreak(LineBreak::NoWrap),
                            Node {
                                margin: UiRect{ left: Val::Px(4.), right: Val::Px(4.), ..default() },
                                ..default()
                            },
                            BuildingInfoPanelNameText,
                        ),
                        // Building Healthbar
                        (
                            Node {
                                width: Val::Percent(100.),
                                height: Val::Percent(100.),
                                ..default()
                            },
                            Healthbar::default(),
                            BuildingInfoPanelHealthbar,
                        ),
                        // Disable/Enable button
                        (
                            BuildingInfoPanelDisableButton,
                        ),
                        // Destroy button
                        (
                            BuildingInfoPanelDestroyButton,
                        ),
                    ],
                ),
                // Specialized panels depending on the building type
                tower_subpanel_content_bundle(),
                super::exploration_center::ExplorationCenterInfoPanel::subpanel_content_bundle(),
            ],
        ));
    });
}

// Tower subpanel section
fn on_building_info_panel_enabled_for_towers_trigger(
    trigger: On<BuildingInfoPanelEnabledTrigger>,
    mut commands: Commands,
    tower_subpanel_root: Single<&mut Node, With<BuildingInfoPanelTowerRoot>>,
    towers: Query<&Upgrades, With<Tower>>,
    upgrades_container: Single<Entity, With<BuildingInfoPanelTowerUpgradesContainer>>,
){
    let focused_entity = trigger.entity;
    let Ok(upgrades) = towers.get(focused_entity) else {
        tower_subpanel_root.into_inner().display = Display::None;
        return;
    };
    tower_subpanel_root.into_inner().display = Display::Flex;

    // Rebuild the upgrades container
    commands.entity(upgrades_container.into_inner())
        // Clear all existing children
        .despawn_related::<Children>()
        // Create upgrade buttons for each available upgrade
        .with_children(|parent| {
            for upgrade_type in upgrades.upgrades.keys().copied() {
                parent.spawn(UpgradeLineBuilder {
                    target_entity: focused_entity,
                    upgrade_type,
                });
            }
        });
}

fn tower_subpanel_content_bundle() -> impl Bundle {
    (
        Node {
            width: Val::Percent(100.),
            height: Val::Percent(100.),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Start,
            align_items: AlignItems::Start,
            ..default()
        },
        BuildingInfoPanelTowerRoot,
        children![
            (
                Text::new("--- Upgrades ##/## ---"),
                TextColor::from(BLUE),
                TextLayout::new_with_linebreak(LineBreak::NoWrap),
                Node {
                    margin: UiRect{ left: Val::Px(4.), right: Val::Px(4.), ..default() },
                    ..default()
                },
                BuildingInfoPanelTowerUpgradeCountText,
            ),
            (
                Node {
                    width: Val::Percent(100.),
                    justify_items: JustifyItems::Center,
                    flex_direction: FlexDirection::Column,
                    ..default()
                },
                BuildingInfoPanelTowerUpgradesContainer,
            ),
        ],
    )
}

// Disable/Enable button
#[derive(Component)]
#[require(Button)]
struct BuildingInfoPanelDisableButton;
#[derive(Component)]
struct BuildingInfoPanelDisableButtonIcon;
impl BuildingInfoPanelDisableButton {
    fn on_add(
        trigger: On<Add, BuildingInfoPanelDisableButton>,
        mut commands: Commands,
        asset_server: Res<AssetServer>,
    ) {
        let entity = trigger.entity;
        // Style the button and attach click handler
        commands
            .entity(entity)
            .insert((
                Node {
                    width: Val::Px(32.),
                    height: Val::Px(32.),
                    margin: UiRect { left: Val::Px(2.), ..default() },
                    align_self: AlignSelf::Center,
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
            ))
            .observe(Self::on_click)
            .with_children(|parent| {
                parent.spawn((
                    ImageNode::new(asset_server.load("indicators/disabled.png")).with_color(WHITE.with_alpha(0.35).into()),
                    BuildingInfoPanelDisableButtonIcon,
                ));
            });
    }

    fn on_click(
        _trigger: On<Pointer<Click>>,
        mut commands: Commands,
        focused_building: Single<(Entity, Has<DisabledByPlayer>), With<FocusedMapObject>>,
        icon: Single<&mut ImageNode, With<BuildingInfoPanelDisableButtonIcon>>,
    ) {
        let (focused_entity, is_disabled) = focused_building.into_inner();
        if is_disabled {
            commands.entity(focused_entity).remove::<DisabledByPlayer>();
        } else {
            commands.entity(focused_entity).insert(DisabledByPlayer);
        }

        // Update icon alpha to reflect state after toggle
        icon.into_inner().color.set_alpha(if is_disabled { 0.35 } else { 1.0 });
    }
}

// Destroy button
#[derive(Component)]
#[require(Button)]
struct BuildingInfoPanelDestroyButton;
#[derive(Component)]
struct BuildingInfoPanelDestroyButtonIcon;
impl BuildingInfoPanelDestroyButton {
    fn on_add(
        trigger: On<Add, BuildingInfoPanelDestroyButton>,
        mut commands: Commands,
        asset_server: Res<AssetServer>,
    ) {
        let entity = trigger.entity;
        commands
            .entity(entity)
            .insert((
                Node {
                    width: Val::Px(32.),
                    height: Val::Px(32.),
                    margin: UiRect { left: Val::Px(2.), ..default() },
                    align_self: AlignSelf::Center,
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
            ))
            .observe(Self::on_click)
            .with_children(|parent| {
                parent.spawn((
                    ImageNode::new(asset_server.load("ui/building_destroy.png")),
                    BuildingInfoPanelDestroyButtonIcon,
                ));
            });
    }

    fn on_click(
        _trigger: On<Pointer<Click>>,
        mut commands: Commands,
        focused_building: Single<Entity, With<FocusedMapObject>>,
    ) {
        let focused_entity = focused_building.into_inner();
        
        // Emit building destroy request event
        commands.trigger(BuildingDestroyRequest(focused_entity));
    }
}