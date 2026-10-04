use bevy::{
    color::palettes::css::{BLUE, WHITE},
    prelude::*,
    ui::FocusPolicy,
};

use almanach::prelude::*;
use buildings::prelude::*;
use game_core::prelude::*;
use hud::prelude::*;
use logging::prelude::*;
use resources::prelude::*;
use shards::prelude::*;
use states::prelude::*;
use widgets::{
    common::utils::recolor_background_on,
    prelude::{BuilderHealthbar, Healthbar},
};

use crate::common::*;

pub(crate) struct InfoPanelPlugin;
impl Plugin for InfoPanelPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(PostStartup, initialize_building_panel_content_system)
            .add_systems(Update, update_building_info_panel_system.run_if(in_state(UiInteraction::DisplayInfoPanel)))
            .add_observer(on_insert_focused_map_object_show_building_info_panel)
            .add_observer(on_building_info_panel_enabled_toggle_tower_subpanel)
            .add_observer(on_rebuild_tower_shard_slots_ui_do_so)
            .add_observer(ShardSocketSlot::on_add_construct_socket_slot_ui)
            .add_observer(ShardSelectionPanel::on_add_construct_shard_selection_panel)
            .add_observer(ShardPickerItem::on_add_construct_shard_picker_item)
            .add_observer(ShardSelectionPanel::on_remove_focused_map_object_close_shard_selection_panel)
            .add_observer(BuildingInfoPanelDisableButton::on_add_construct_disable_button)
            .add_observer(BuildingInfoPanelDestroyButton::on_add_construct_destroy_button);
    }
}

// Common
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
    let mut healthbar = healthbar.into_inner();
    healthbar.value = integrity_points.get_current();
    healthbar.max_value = integrity_points.get_max();
    let integrity_points_percentage = integrity_points.get_percent();
    healthbar.color = Color::srgba(1. - integrity_points_percentage, integrity_points_percentage, 0., 1.);
}

fn on_insert_focused_map_object_show_building_info_panel(
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
                                margin: UiRect::horizontal(Val::Px(4.)),
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
                            children![(
                                BuilderHealthbar::default(),
                                BuildingInfoPanelHealthbar,
                            )],
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

fn on_building_info_panel_enabled_toggle_tower_subpanel(
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

fn on_rebuild_tower_shard_slots_ui_do_so(
    _trigger: On<RebuildTowerShardSlotsUi>,
    mut commands: Commands,
    focused_tower: Single<(Entity, &ShardSlots), With<FocusedMapObject>>,
    shards_container: Single<Entity, With<TowerShardSlotsContainer>>,
    existing_selection_panel: Option<Single<Entity, With<ShardSelectionPanel>>>,
) {
    let (shard_target, shard_slots) = focused_tower.into_inner();
    let container_entity = shards_container.into_inner();
    commands.entity(container_entity).despawn_children();
    if let Some(panel) = existing_selection_panel { commands.entity(panel.into_inner()).despawn(); }

    for (slot_index, shard) in shard_slots.iter().enumerate() {
        commands.entity(container_entity).with_child(ShardSocketSlot { shard_target, slot_index, shard });
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
                    margin: UiRect::horizontal(Val::Px(4.)),
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

// Shard socket slot: shows the socket's stat and the socketed shard, opens the picker on click
#[derive(Component)]
#[require(Button)]
struct ShardSocketSlot {
    shard_target: Entity,
    slot_index: usize,
    shard: Option<Shard>,
}
impl ShardSocketSlot {
    fn on_add_construct_socket_slot_ui(
        trigger: On<Add, ShardSocketSlot>,
        mut commands: Commands,
        almanach: Res<Almanach>,
        slots: Query<&ShardSocketSlot>,
        building_types: Query<&BuildingType>,
    ) {
        let entity = trigger.entity;
        let Ok(slot) = slots.get(entity) else { return };
        let Ok(building_type) = building_types.get(slot.shard_target) else { return };
        let Some(socket) = almanach.get_building_info(*building_type).sockets.get(slot.slot_index) else { return };
        let (content, background, border) = match slot.shard {
            Some(shard) => (
                almanach.get_resource_info(ResourceType::Shard(shard)).name.clone(),
                Color::srgba(0.1, 0.3, 0.1, 0.9),
                Color::srgba(0.2, 0.6, 0.2, 1.0),
            ),
            None => (
                format!("+ {}", socket.shard_type),
                Color::srgba(0.15, 0.15, 0.15, 0.9),
                Color::srgba(0.4, 0.4, 0.4, 1.0),
            ),
        };
        commands.entity(entity)
            .insert((
                Node {
                    width: Val::Px(84.),
                    min_height: Val::Px(48.),
                    margin: UiRect::all(Val::Px(3.)),
                    padding: UiRect::all(Val::Px(3.)),
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    row_gap: Val::Px(2.),
                    border: UiRect::all(Val::Px(1.)),
                    ..default()
                },
                BackgroundColor(background),
                BorderColor::all(border),
            ))
            .observe(Self::on_click_open_shard_selection_panel)
            .observe(recolor_background_on::<Pointer<Over>>(Color::srgba(0.2, 0.2, 0.4, 0.9)))
            .observe(recolor_background_on::<Pointer<Out>>(background))
            .with_children(|parent| {
                parent.spawn((
                    Text::new(content),
                    TextLayout::no_wrap(),
                    TextFont::default().with_font_size(11.0),
                    TextColor::from(Color::WHITE),
                ));
                parent.spawn((
                    Text::new(socket.description.clone()),
                    TextLayout { justify: Justify::Center, ..default() },
                    TextFont::default().with_font_size(9.0),
                    TextColor::from(Color::srgb(0.6, 0.6, 0.6)),
                ));
            });
    }

    fn on_click_open_shard_selection_panel(
        trigger: On<Pointer<Click>>,
        mut commands: Commands,
        slots: Query<&ShardSocketSlot>,
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

// Modal panel for selecting a shard from the stock
#[derive(Component)]
struct ShardSelectionPanel {
    shard_target: Entity,
    slot_index: usize,
}
impl ShardSelectionPanel {
    fn on_add_construct_shard_selection_panel(
        trigger: On<Add, ShardSelectionPanel>,
        mut commands: Commands,
        almanach: Res<Almanach>,
        stock: Res<Stock>,
        panels: Query<&ShardSelectionPanel>,
        shard_targets: Query<(&BuildingType, &ShardSlots)>,
    ) {
        let entity = trigger.entity;
        let Ok(panel) = panels.get(entity) else { return };
        let shard_target = panel.shard_target;
        let slot_index = panel.slot_index;
        let Ok((building_type, shard_slots)) = shard_targets.get(shard_target) else { return };
        let Some(socket) = almanach.get_building_info(*building_type).sockets.get(slot_index) else { return };
        let is_occupied = shard_slots.shard(slot_index).is_some();

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
                    BackgroundColor(Color::srgba(0.05, 0.05, 0.1, 0.97)),
                    BorderColor::all(Color::srgba(0.3, 0.3, 0.5, 1.0)),
                ))
                .with_children(|parent| {
                    parent.spawn((
                        Text::new(format!("{} shard: {}", socket.shard_type, socket.description)),
                        TextFont::default().with_font_size(14.0),
                        TextColor::from(BLUE),
                        Node { margin: UiRect::bottom(Val::Px(4.)), ..default() },
                    ));

                    let mut available: Vec<(Shard, i32)> = stock.iter()
                        .filter_map(|entry| match entry.resource_type {
                            ResourceType::Shard(shard) if entry.amount > 0 && socket.accepts(shard) => Some((shard, entry.amount)),
                            _ => None,
                        })
                        .collect();
                    available.sort_unstable();

                    if available.is_empty() {
                        parent.spawn((
                            Text::new("No shards available"),
                            TextFont::default().with_font_size(12.0),
                            TextColor::from(Color::srgb(0.5, 0.5, 0.5)),
                        ));
                    } else {
                        for (shard, count) in available {
                            parent.spawn(ShardPickerItem { shard, count });
                        }
                    }

                    if is_occupied {
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
                            BackgroundColor(Color::srgba(0.2, 0.2, 0.1, 0.9)),
                            BorderColor::all(Color::srgba(0.5, 0.5, 0.2, 1.0)),
                        ))
                        .observe(Self::on_click_unsocket_shard)
                        .observe(recolor_background_on::<Pointer<Over>>(Color::srgba(0.3, 0.3, 0.15, 0.95)))
                        .observe(recolor_background_on::<Pointer<Out>>(Color::srgba(0.2, 0.2, 0.1, 0.9)))
                        .with_children(|parent| {
                            parent.spawn((
                                Text::new("Unsocket"),
                                TextFont::default().with_font_size(12.0),
                                TextColor::from(Color::WHITE),
                            ));
                        });
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
                        BackgroundColor(Color::srgba(0.2, 0.1, 0.1, 0.9)),
                        BorderColor::all(Color::srgba(0.5, 0.2, 0.2, 1.0)),
                    ))
                    .observe(Self::on_click_close_shard_selection_panel)
                    .observe(recolor_background_on::<Pointer<Over>>(Color::srgba(0.3, 0.15, 0.15, 0.95)))
                    .observe(recolor_background_on::<Pointer<Out>>(Color::srgba(0.2, 0.1, 0.1, 0.9)))
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

    fn on_click_unsocket_shard(
        _trigger: On<Pointer<Click>>,
        mut commands: Commands,
        panel: Single<(Entity, &ShardSelectionPanel)>,
    ) {
        let (panel_entity, panel) = panel.into_inner();
        commands.trigger(ShardSocketOperation::unsocket(panel.shard_target, panel.slot_index));
        commands.entity(panel_entity).despawn();
        commands.trigger(RebuildTowerShardSlotsUi);
    }

    fn on_click_close_shard_selection_panel(
        _trigger: On<Pointer<Click>>,
        mut commands: Commands,
        panel: Single<Entity, With<ShardSelectionPanel>>,
    ) {
        commands.entity(panel.into_inner()).despawn();
    }

    fn on_remove_focused_map_object_close_shard_selection_panel(
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
    shard: Shard,
    count: i32,
}
impl ShardPickerItem {
    fn on_add_construct_shard_picker_item(
        trigger: On<Add, ShardPickerItem>,
        mut commands: Commands,
        almanach: Res<Almanach>,
        items: Query<&ShardPickerItem>,
    ) {
        let entity = trigger.entity;
        let Ok(item) = items.get(entity) else { return };
        let label = format!("{} x{}", almanach.get_resource_info(ResourceType::Shard(item.shard)).name, item.count);
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
                BackgroundColor(Color::srgba(0.15, 0.15, 0.25, 0.9)),
                BorderColor::all(Color::srgba(0.3, 0.3, 0.5, 1.0)),
            ))
            .observe(Self::on_click_socket_shard)
            .observe(recolor_background_on::<Pointer<Over>>(Color::srgba(0.25, 0.3, 0.5, 0.95)))
            .observe(recolor_background_on::<Pointer<Out>>(Color::srgba(0.15, 0.15, 0.25, 0.9)))
            .with_children(|parent| {
                parent.spawn((
                    Text::new(label),
                    TextFont::default().with_font_size(12.0),
                    TextColor::from(Color::WHITE),
                ));
            });
    }

    fn on_click_socket_shard(
        trigger: On<Pointer<Click>>,
        mut commands: Commands,
        items: Query<&ShardPickerItem>,
        panel: Single<(Entity, &ShardSelectionPanel)>,
    ) {
        let entity = trigger.entity;
        let Ok(item) = items.get(entity) else { return };
        let (panel_entity, panel) = panel.into_inner();
        commands.trigger(ShardSocketOperation::socket(panel.shard_target, panel.slot_index, item.shard));

        commands.entity(panel_entity).despawn();
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
        commands
            .entity(entity)
            .insert((
                Node {
                    width: Val::Px(32.),
                    height: Val::Px(32.),
                    margin: UiRect::left(Val::Px(2.)),
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

    #[log_tags(Tag::Build)]
    fn on_click_toggle_building_disabled(
        _trigger: On<Pointer<Click>>,
        mut commands: Commands,
        almanach: Res<Almanach>,
        focused_building: Single<(Entity, &BuildingType, Has<DisabledByPlayer>), With<FocusedMapObject>>,
        icon: Single<&mut ImageNode, With<BuildingInfoPanelDisableButtonIcon>>,
    ) {
        let (focused_entity, building_type, is_disabled) = focused_building.into_inner();
        let building_name = &almanach.get_building_info(*building_type).name;
        if is_disabled {
            #[info_player("'{building_name}' enabled")]
            commands.entity(focused_entity).remove::<DisabledByPlayer>();
        } else {
            #[info_player("'{building_name}' disabled")]
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
                    margin: UiRect::left(Val::Px(2.)),
                    align_self: AlignSelf::Center,
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
            ))
            .observe(Self::on_click_request_building_destroy)
            .with_children(|parent| {
                parent.spawn(ImageNode::new(asset_server.load("ui/building_destroy.png")));
            });
    }

    fn on_click_request_building_destroy(
        _trigger: On<Pointer<Click>>,
        mut commands: Commands,
        focused_building: Single<Entity, With<FocusedMapObject>>,
    ) {
        commands.trigger(BuildingDestroyRequest(focused_building.into_inner()));
    }
}
