use bevy::{
    color::palettes::css::{BLUE, WHITE},
    prelude::*,
    ui::FocusPolicy,
};

use almanach::prelude::*;
use buildings::prelude::*;
use game_core::prelude::*;
use hud::prelude::*;
use shards::prelude::*;
use states::prelude::*;
use widgets::{
    prelude::Healthbar,
    utils::recolor_background_on,
};

use crate::common::*;

pub struct InfoPanelPlugin;
impl Plugin for InfoPanelPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(PostStartup, initialize_building_panel_content_system)
            .add_systems(Update, update_building_info_panel_system.run_if(in_state(UiInteraction::DisplayInfoPanel)))
            .add_observer(on_focused_map_object_insert_display_building_info_panel)
            .add_observer(on_building_info_panel_enabled_for_towers_toggle_subpanel_visibility)
            .add_observer(on_rebuild_tower_shard_slots)
            .add_observer(ShardSlotOccupied::on_add_construct_occupied_slot_ui)
            .add_observer(ShardSlotEmpty::on_add_construct_empty_slot_ui)
            .add_observer(ShardSelectionPanel::on_add_construct_shard_selection_panel)
            .add_observer(ShardPickerItem::on_add_construct_shard_picker_item)
            .add_observer(ShardSelectionPanel::on_focused_map_object_removed_close_shard_selection_panel)
            .add_observer(BuildingInfoPanelDisableButton::on_add_construct_disable_button)
            .add_observer(BuildingInfoPanelDestroyButton::on_add_construct_destroy_button)
            ;
    }
}

/// Common
#[derive(Component)]
pub(crate) struct BuildingInfoPanel;
#[derive(Component)]
pub(crate) struct BuildingInfoPanelNameText;
#[derive(Component)]
pub(crate) struct BuildingInfoPanelHealthbar;
#[derive(EntityEvent)]
pub(crate) struct BuildingInfoPanelEnabledTrigger { pub entity: Entity }

// Tower Subpanel
#[derive(Component)]
struct BuildingInfoPanelTowerRoot;
#[derive(Component)]
struct TowerShardSlotsContainer;
#[derive(Event)]
struct RebuildTowerShardSlotsUi;

fn update_building_info_panel_system(
    focused_building: Single<&IntegrityPoints, (With<FocusedMapObject>, With<Building>)>,
    healthbar: Single<&mut Healthbar, With<BuildingInfoPanelHealthbar>>,
) {
    let integrity_points = focused_building.into_inner();
    // Update the healthbar
    let mut healthbar = healthbar.into_inner();
    healthbar.value = integrity_points.get_current();
    healthbar.max_value = integrity_points.get_max();
    let integrity_points_percentage = integrity_points.get_percent();
    healthbar.color = Color::linear_rgba(1. - integrity_points_percentage, integrity_points_percentage, 0., 1.);
}

fn on_focused_map_object_insert_display_building_info_panel(
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

fn initialize_building_panel_content_system(
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
                            TextLayout::no_wrap(),
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
                super::forge::ForgeInfoPanel::subpanel_content_bundle(),
            ],
        ));
    });
}

fn on_building_info_panel_enabled_for_towers_toggle_subpanel_visibility(
    trigger: On<BuildingInfoPanelEnabledTrigger>,
    mut commands: Commands,
    tower_subpanel_root: Single<&mut Node, With<BuildingInfoPanelTowerRoot>>,
    towers: Query<(), With<Tower>>,
) {
    let focused_entity = trigger.entity;
    if towers.contains(focused_entity) {
        tower_subpanel_root.into_inner().display = Display::Flex;
        commands.trigger(RebuildTowerShardSlotsUi);
    } else {
        tower_subpanel_root.into_inner().display = Display::None;
    }
}

fn on_rebuild_tower_shard_slots(
    _trigger: On<RebuildTowerShardSlotsUi>,
    mut commands: Commands,
    focused_tower: Single<(Entity, &ShardSlots), With<FocusedMapObject>>,
    shards_container: Single<(Entity, Option<&Children>), With<TowerShardSlotsContainer>>,
    existing_selection_panel: Option<Single<Entity, With<ShardSelectionPanel>>>,
) {
    let (shard_target, shard_slots) = focused_tower.into_inner();
    let (container_entity, children) = shards_container.into_inner();

    if let Some(children) = children {
        for child in children.iter() {
            commands.entity(child).despawn();
        }
    }
    if let Some(panel) = existing_selection_panel {
        commands.entity(panel.into_inner()).despawn();
    }

    for slot_index in 0..shard_slots.capacity() {
        match shard_slots.get(slot_index) {
            Some(shard_type) => { commands.entity(container_entity).with_child(ShardSlotOccupied { shard_type }); }
            None => { commands.entity(container_entity).with_child(ShardSlotEmpty { shard_target, slot_index }); }
        }
    }
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
                Text::new("--- Shards ---"),
                TextColor::from(BLUE),
                TextLayout::no_wrap(),
                Node {
                    margin: UiRect{ left: Val::Px(4.), right: Val::Px(4.), ..default() },
                    ..default()
                },
            ),
            (
                Node {
                    width: Val::Percent(100.),
                    flex_direction: FlexDirection::Row,
                    flex_wrap: FlexWrap::Wrap,
                    ..default()
                },
                TowerShardSlotsContainer,
            ),
        ],
    )
}

// Shard slot: filled slot (read-only display)
#[derive(Component)]
struct ShardSlotOccupied {
    shard_type: ShardType,
}
impl ShardSlotOccupied {
    fn on_add_construct_occupied_slot_ui(
        trigger: On<Add, ShardSlotOccupied>,
        mut commands: Commands,
        slots: Query<&ShardSlotOccupied>,
    ) {
        let entity = trigger.entity;
        let Ok(slot) = slots.get(entity) else { return };
        let shard_name = slot.shard_type.to_string();
        commands.entity(entity)
            .insert((
                Node {
                    width: Val::Px(48.),
                    height: Val::Px(48.),
                    margin: UiRect::all(Val::Px(3.)),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border: UiRect::all(Val::Px(1.)),
                    ..default()
                },
                BackgroundColor(Color::linear_rgba(0.1, 0.3, 0.1, 0.9)),
                BorderColor::all(Color::linear_rgba(0.2, 0.6, 0.2, 1.0)),
            ))
            .with_children(|parent| {
                parent.spawn((
                    Text::new(shard_name),
                    TextLayout::no_wrap(),
                    TextFont::default().with_font_size(10.0),
                    TextColor::from(Color::WHITE),
                ));
            });
    }
}

// Shard slot: empty slot (clickable)
#[derive(Component)]
#[require(Button)]
struct ShardSlotEmpty {
    shard_target: Entity,
    slot_index: usize,
}
impl ShardSlotEmpty {
    fn on_add_construct_empty_slot_ui(
        trigger: On<Add, ShardSlotEmpty>,
        mut commands: Commands,
    ) {
        let entity = trigger.entity;
        commands.entity(entity)
            .insert((
                Node {
                    width: Val::Px(48.),
                    height: Val::Px(48.),
                    margin: UiRect::all(Val::Px(3.)),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border: UiRect::all(Val::Px(1.)),
                    ..default()
                },
                BackgroundColor(Color::linear_rgba(0.15, 0.15, 0.15, 0.9)),
                BorderColor::all(Color::linear_rgba(0.4, 0.4, 0.4, 1.0)),
            ))
            .observe(Self::on_click_open_shard_selection_panel)
            .observe(recolor_background_on::<Pointer<Over>>(Color::linear_rgba(0.2, 0.2, 0.4, 0.9)))
            .observe(recolor_background_on::<Pointer<Out>>(Color::linear_rgba(0.15, 0.15, 0.15, 0.9)))
            .with_children(|parent| {
                parent.spawn((
                    Text::new("+"),
                    TextFont::default().with_font_size(20.0),
                    TextColor::from(Color::WHITE),
                ));
            });
    }

    fn on_click_open_shard_selection_panel(
        trigger: On<Pointer<Click>>,
        mut commands: Commands,
        slots: Query<&ShardSlotEmpty>,
        existing_panel: Option<Single<Entity, With<ShardSelectionPanel>>>,
    ) {
        let entity = trigger.entity;
        let Ok(slot) = slots.get(entity) else { return };
        if let Some(panel) = existing_panel {
            commands.entity(panel.into_inner()).despawn();
        }
        commands.spawn(ShardSelectionPanel { shard_target: slot.shard_target, slot_index: slot.slot_index });
    }
}

// Modal panel for selecting a shard from inventory
#[derive(Component)]
struct ShardSelectionPanel {
    shard_target: Entity,
    slot_index: usize,
}
impl ShardSelectionPanel {
    fn on_add_construct_shard_selection_panel(
        trigger: On<Add, ShardSelectionPanel>,
        mut commands: Commands,
        inventory: Res<ShardInventory>,
        panels: Query<&ShardSelectionPanel>,
    ) {
        let entity = trigger.entity;
        let Ok(panel) = panels.get(entity) else { return };
        let shard_target = panel.shard_target;
        let slot_index = panel.slot_index;

        commands.entity(entity)
            .insert((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.),
                    top: Val::Px(0.),
                    width: Val::Percent(100.),
                    height: Val::Percent(100.),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                GlobalZIndex(100),
                FocusPolicy::Pass,
                Pickable::IGNORE,
            ))
            .with_children(|parent| {
                parent.spawn((
                    Node {
                        width: Val::Px(220.),
                        flex_direction: FlexDirection::Column,
                        padding: UiRect::all(Val::Px(8.)),
                        row_gap: Val::Px(4.),
                        border: UiRect::all(Val::Px(1.)),
                        ..default()
                    },
                    BackgroundColor(Color::linear_rgba(0.05, 0.05, 0.1, 0.97)),
                    BorderColor::all(Color::linear_rgba(0.3, 0.3, 0.5, 1.0)),
                ))
                .with_children(|parent| {
                    parent.spawn((
                        Text::new("Select Shard"),
                        TextFont::default().with_font_size(14.0),
                        TextColor::from(BLUE),
                        Node { margin: UiRect::bottom(Val::Px(4.)), ..default() },
                    ));

                    let available: Vec<(ShardType, usize)> = inventory.iter()
                        .filter(|(_, count)| *count > 0)
                        .collect();

                    if available.is_empty() {
                        parent.spawn((
                            Text::new("No shards available"),
                            TextFont::default().with_font_size(12.0),
                            TextColor::from(Color::srgb(0.5, 0.5, 0.5)),
                        ));
                    } else {
                        for (shard_type, count) in available {
                            parent.spawn(ShardPickerItem { shard_target, slot_index, shard_type, count });
                        }
                    }

                    parent.spawn((
                        Button,
                        Node {
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            padding: UiRect::axes(Val::Px(8.), Val::Px(4.)),
                            margin: UiRect::top(Val::Px(4.)),
                            border: UiRect::all(Val::Px(1.)),
                            ..default()
                        },
                        BackgroundColor(Color::linear_rgba(0.2, 0.1, 0.1, 0.9)),
                        BorderColor::all(Color::linear_rgba(0.5, 0.2, 0.2, 1.0)),
                    ))
                    .observe(Self::on_cancel_click_close_shard_selection_panel)
                    .observe(recolor_background_on::<Pointer<Over>>(Color::linear_rgba(0.3, 0.15, 0.15, 0.95)))
                    .observe(recolor_background_on::<Pointer<Out>>(Color::linear_rgba(0.2, 0.1, 0.1, 0.9)))
                    .with_children(|parent| {
                        parent.spawn((
                            Text::new("Cancel"),
                            TextFont::default().with_font_size(12.0),
                            TextColor::from(Color::WHITE),
                        ));
                    });
                });
            });
    }

    fn on_cancel_click_close_shard_selection_panel(
        _trigger: On<Pointer<Click>>,
        mut commands: Commands,
        panel: Single<Entity, With<ShardSelectionPanel>>,
    ) {
        commands.entity(panel.into_inner()).despawn();
    }

    fn on_focused_map_object_removed_close_shard_selection_panel(
        _trigger: On<Remove, FocusedMapObject>,
        mut commands: Commands,
        panel: Single<Entity, With<ShardSelectionPanel>>,
    ) {
        commands.entity(panel.into_inner()).despawn();
    }
}

// Selectable shard entry inside ShardSelectionPanel
#[derive(Component)]
struct ShardPickerItem {
    shard_target: Entity,
    slot_index: usize,
    shard_type: ShardType,
    count: usize,
}
impl ShardPickerItem {
    fn on_add_construct_shard_picker_item(
        trigger: On<Add, ShardPickerItem>,
        mut commands: Commands,
        items: Query<&ShardPickerItem>,
    ) {
        let entity = trigger.entity;
        let Ok(item) = items.get(entity) else { return };
        let label = format!("{} x{}", item.shard_type, item.count);
        commands.entity(entity)
            .insert((
                Button,
                Node {
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    padding: UiRect::axes(Val::Px(6.), Val::Px(4.)),
                    border: UiRect::all(Val::Px(1.)),
                    ..default()
                },
                BackgroundColor(Color::linear_rgba(0.15, 0.15, 0.25, 0.9)),
                BorderColor::all(Color::linear_rgba(0.3, 0.3, 0.5, 1.0)),
            ))
            .observe(Self::on_click_equip_shard)
            .observe(recolor_background_on::<Pointer<Over>>(Color::linear_rgba(0.25, 0.3, 0.5, 0.95)))
            .observe(recolor_background_on::<Pointer<Out>>(Color::linear_rgba(0.15, 0.15, 0.25, 0.9)))
            .with_children(|parent| {
                parent.spawn((
                    Text::new(label),
                    TextFont::default().with_font_size(12.0),
                    TextColor::from(Color::WHITE),
                ));
            });
    }

    fn on_click_equip_shard(
        trigger: On<Pointer<Click>>,
        mut commands: Commands,
        mut inventory: ResMut<ShardInventory>,
        items: Query<&ShardPickerItem>,
        mut shard_slots: Query<&mut ShardSlots>,
        panel: Single<Entity, With<ShardSelectionPanel>>,
    ) {
        let entity = trigger.entity;
        let Ok(item) = items.get(entity) else { return };

        if !inventory.has(item.shard_type) { return; }
        let Ok(mut slots) = shard_slots.get_mut(item.shard_target) else { return; };
        if slots.get(item.slot_index).is_some() { return; }

        inventory.remove(item.shard_type);
        slots.insert_at(item.slot_index, item.shard_type, item.shard_target, &mut commands);

        commands.entity(panel.into_inner()).despawn();
        commands.trigger(RebuildTowerShardSlotsUi);
    }
}

// Disable/Enable button
#[derive(Component)]
#[require(Button)]
struct BuildingInfoPanelDisableButton;
#[derive(Component)]
struct BuildingInfoPanelDisableButtonIcon;
impl BuildingInfoPanelDisableButton {
    fn on_add_construct_disable_button(
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
            .observe(Self::on_click_toggle_building_disabled)
            .with_children(|parent| {
                parent.spawn((
                    ImageNode::new(asset_server.load("indicators/disabled.png")).with_color(WHITE.with_alpha(0.35).into()),
                    BuildingInfoPanelDisableButtonIcon,
                ));
            });
    }

    fn on_click_toggle_building_disabled(
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
    fn on_add_construct_destroy_button(
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
            .observe(Self::on_click_request_building_destroy)
            .with_children(|parent| {
                parent.spawn((
                    ImageNode::new(asset_server.load("ui/building_destroy.png")),
                    BuildingInfoPanelDestroyButtonIcon,
                ));
            });
    }

    fn on_click_request_building_destroy(
        _trigger: On<Pointer<Click>>,
        mut commands: Commands,
        focused_building: Single<Entity, With<FocusedMapObject>>,
    ) {
        let focused_entity = focused_building.into_inner();
        
        // Emit building destroy request event
        commands.trigger(BuildingDestroyRequest(focused_entity));
    }
}