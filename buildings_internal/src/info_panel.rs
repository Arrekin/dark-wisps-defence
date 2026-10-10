use bevy::{
    color::palettes::css::{BLUE, WHITE},
    prelude::*,
    ui::FocusPolicy,
    ui_widgets::Button,
};

use almanach::prelude::*;
use buildings::prelude::*;
use game_core::prelude::*;
use hud::prelude::*;
use logging::prelude::*;
use resources::prelude::*;
use shards::{
    orders::{AwaitsShardOrder, BuilderShardOrder, ForgingProgress, GLOBAL_QUEUE_COLOR, GlobalForgingQueue, OrderLink, ShardOrder},
    prelude::*,
    sockets::ShardSocketOf,
};
use states::prelude::*;
use widgets::{
    common::utils::{recolor_background_on, set_text_if_changed},
    prelude::{BuilderHealthbar, Healthbar},
};

use crate::common::*;

pub(crate) struct InfoPanelPlugin;
impl Plugin for InfoPanelPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(PostStartup, initialize_building_panel_content)
            .add_systems(Update, (
                update_building_info_panel,
                ShardSocketTile::update_pending_order_statuses,
            ).run_if(in_state(UiInteraction::DisplayInfoPanel)))
            .add_observer(on_insert_focused_map_object_show_building_info_panel)
            .add_observer(on_building_info_panel_enabled_toggle_tower_subpanel)
            .add_observer(on_rebuild_tower_shard_sockets_ui_do_so)
            .add_observer(ShardSocketTile::on_add_construct_socket_tile_ui)
            .add_observer(ShardSocketTile::rebuild_tower_shard_sockets_ui_on::<Insert<ShardSocketState>>)
            .add_observer(ShardSocketTile::rebuild_tower_shard_sockets_ui_on::<Remove<SocketedShard>>)
            .add_observer(ShardSocketTile::rebuild_tower_shard_sockets_ui_on::<Insert<AwaitsShardOrder>>)
            .add_observer(ShardSocketTile::rebuild_tower_shard_sockets_ui_on::<Remove<AwaitsShardOrder>>)
            .add_observer(ShardSelectionPanel::on_add_construct_shard_selection_panel)
            .add_observer(ShardPickerItem::on_add_construct_shard_picker_item)
            .add_observer(ShardSelectionPanel::on_remove_focused_map_object_close_shard_selection_panel)
            .add_observer(BuildingInfoPanelDisableButton::on_add_construct_disable_button)
            .add_observer(BuildingInfoPanelDestroyButton::on_add_construct_destroy_button);
    }
}

// Building header
const HEADER_BUTTON_SIZE: f32 = 32.0;
const DISABLE_ICON_ACTIVE_ALPHA: f32 = 1.0;
const DISABLE_ICON_INACTIVE_ALPHA: f32 = 0.35;

// Shard picker
const PICKER_WIDTH: f32 = 220.0;
const PICKER_LIST_MAX_HEIGHT: f32 = 240.0;
const PICKER_TEXT_FONT_SIZE: f32 = 12.0;
const PICKER_BACKGROUND: Color = Color::srgba(0.05, 0.05, 0.1, 0.97);
const PICKER_BORDER: Color = Color::srgba(0.3, 0.3, 0.5, 1.0);
const PICKER_EMPTY_TEXT_COLOR: Color = Color::srgb(0.5, 0.5, 0.5);
const HELD_ROW_BACKGROUND: Color = Color::srgba(0.15, 0.15, 0.25, 0.9);
const HELD_ROW_HOVER_BACKGROUND: Color = Color::srgba(0.25, 0.3, 0.5, 0.95);
const HELD_ROW_BORDER: Color = Color::srgba(0.3, 0.3, 0.5, 1.0);
const ORDER_ROW_HOVER_BACKGROUND: Color = Color::srgba(0.35, 0.27, 0.08, 0.95);
const FOOTER_BUTTON_BACKGROUND: Color = Color::srgba(0.2, 0.2, 0.1, 0.9);
const FOOTER_BUTTON_HOVER_BACKGROUND: Color = Color::srgba(0.3, 0.3, 0.15, 0.95);
const FOOTER_BUTTON_BORDER: Color = Color::srgba(0.5, 0.5, 0.2, 1.0);
const CLOSE_BUTTON_BACKGROUND: Color = Color::srgba(0.2, 0.1, 0.1, 0.9);
const CLOSE_BUTTON_HOVER_BACKGROUND: Color = Color::srgba(0.3, 0.15, 0.15, 0.95);
const CLOSE_BUTTON_BORDER: Color = Color::srgba(0.5, 0.2, 0.2, 1.0);

// Socket tiles and picker rows
const ORDERED_BACKGROUND: Color = Color::srgba(0.2, 0.15, 0.05, 0.9); // brown
const SOCKET_TILE_WIDTH: f32 = 84.0;
const SOCKET_TILE_MIN_HEIGHT: f32 = 48.0;
const SOCKETED_TILE_BACKGROUND: Color = Color::srgba(0.1, 0.3, 0.1, 0.9); // green
const SOCKETED_TILE_BORDER: Color = Color::srgba(0.2, 0.6, 0.2, 1.0);
const EMPTY_TILE_BACKGROUND: Color = Color::srgba(0.15, 0.15, 0.15, 0.9); // grey
const EMPTY_TILE_BORDER: Color = Color::srgba(0.4, 0.4, 0.4, 1.0);
const SOCKET_TILE_HOVER_BACKGROUND: Color = Color::srgba(0.2, 0.2, 0.4, 0.9);
const SOCKET_DESCRIPTION_COLOR: Color = Color::srgb(0.6, 0.6, 0.6);

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
struct TowerShardSocketsContainer;
#[derive(Event)]
struct RebuildTowerShardSocketsUi;

fn update_building_info_panel(
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
    trigger: On<Insert<FocusedMapObject>>,
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
            disable_button_icon.into_inner().color.set_alpha(if is_disabled { DISABLE_ICON_ACTIVE_ALPHA } else { DISABLE_ICON_INACTIVE_ALPHA });
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

fn initialize_building_panel_content(
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
        commands.trigger(RebuildTowerShardSocketsUi);
    } else {
        tower_subpanel_root.into_inner().display = Display::None;
    }
}

fn on_rebuild_tower_shard_sockets_ui_do_so(
    _trigger: On<RebuildTowerShardSocketsUi>,
    mut commands: Commands,
    focused_tower: Single<&ShardSockets, With<FocusedMapObject>>,
    shards_container: Single<Entity, With<TowerShardSocketsContainer>>,
    visible_sockets: Query<Entity, Without<RemovedSocket>>,
) {
    let tower_sockets = focused_tower.into_inner();
    let container_entity = shards_container.into_inner();
    commands.entity(container_entity).despawn_children();

    for socket in visible_sockets.iter_many(tower_sockets.iter()).matched() {
        commands.entity(container_entity).with_child(ShardSocketTile { socket });
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
                TowerShardSocketsContainer,
            ),
        ],
    )
}

/// Shows a socket's stat and held or ordered shard. Opens the shard picker on click.
#[derive(Component)]
#[require(Button)]
struct ShardSocketTile {
    socket: Entity,
}

/// The status line of a socket tile awaiting an order.
#[derive(Component)]
struct PendingOrderStatusText {
    order: Entity,
}

impl ShardSocketTile {
    fn on_add_construct_socket_tile_ui(
        trigger: On<Add<ShardSocketTile>>,
        mut commands: Commands,
        almanach: Res<Almanach>,
        tiles: Query<&ShardSocketTile>,
        sockets: Query<(&ShardSocket, Option<&SocketedShard>, Option<&AwaitsShardOrder>)>,
        orders: Query<&ShardOrder>,
    ) {
        let entity = trigger.entity;
        let Ok(tile) = tiles.get(entity) else { return };
        let Ok((socket, socketed, awaited_order)) = sockets.get(tile.socket) else { return };
        let ordered = awaited_order.and_then(|&AwaitsShardOrder(order)| orders.get(order).ok().map(|&ShardOrder(shard)| (order, shard)));
        let (content, background, border) = match (socketed, ordered) {
            (_, Some((_, shard))) => (
                almanach.get_resource_info(shard).name.clone(),
                ORDERED_BACKGROUND,
                GLOBAL_QUEUE_COLOR,
            ),
            (Some(&SocketedShard(shard)), None) => (
                almanach.get_resource_info(shard).name.clone(),
                SOCKETED_TILE_BACKGROUND,
                SOCKETED_TILE_BORDER,
            ),
            (None, None) => (
                format!("+ {}", socket.shard_type),
                EMPTY_TILE_BACKGROUND,
                EMPTY_TILE_BORDER,
            ),
        };
        commands.entity(entity)
            .insert((
                Node {
                    width: Val::Px(SOCKET_TILE_WIDTH),
                    min_height: Val::Px(SOCKET_TILE_MIN_HEIGHT),
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
            .observe(recolor_background_on::<PointerOver>(SOCKET_TILE_HOVER_BACKGROUND))
            .observe(recolor_background_on::<PointerOut>(background))
            .with_children(|parent| {
                parent.spawn((
                    Text::new(content),
                    TextLayout::no_wrap(),
                    TextFont::from_font_size(11.0),
                    TextColor::from(Color::WHITE),
                ));
                if let Some((order, _)) = ordered {
                    parent.spawn((
                        PendingOrderStatusText { order },
                        Text::new(""),
                        TextLayout::no_wrap(),
                        TextFont::from_font_size(9.0),
                        TextColor::from(GLOBAL_QUEUE_COLOR),
                    ));
                }
                parent.spawn((
                    Text::new(socket.description.clone()),
                    TextLayout { justify: Justify::Center, ..default() },
                    TextFont::from_font_size(9.0),
                    TextColor::from(SOCKET_DESCRIPTION_COLOR),
                ));
            });
    }

    fn on_click_open_shard_selection_panel(
        trigger: On<PointerClick>,
        mut commands: Commands,
        tiles: Query<&ShardSocketTile>,
        existing_panel: Option<Single<Entity, With<ShardSelectionPanel>>>,
    ) {
        let entity = trigger.entity;
        let Ok(tile) = tiles.get(entity) else { return };
        if let Some(panel) = existing_panel {
            commands.entity(panel.into_inner()).despawn();
        }
        commands.spawn(ShardSelectionPanel { socket: tile.socket });
    }

    /// Shows remaining forge time for active orders, or "Queued" for waiting orders.
    fn update_pending_order_statuses(
        mut status_texts: Query<(&PendingOrderStatusText, &mut Text)>,
        progresses: Query<&ForgingProgress>,
    ) {
        for (&PendingOrderStatusText { order }, mut text) in status_texts.iter_mut() {
            let status = match progresses.get(order) {
                Ok(progress) => format!("Forging — {:.1}s", progress.remaining_secs()),
                Err(_) => "Queued".to_string(),
            };
            set_text_if_changed(&mut text, &status);
        }
    }

    /// Rebuilds the tiles when the event targets a socket of the focused tower.
    /// Closes the picker opened for that socket, since its actions no longer match the contents.
    fn rebuild_tower_shard_sockets_ui_on<P: EventPattern<Event: EntityEvent>>(
        trigger: On<P>,
        mut commands: Commands,
        focused_tower: Option<Single<Entity, (With<FocusedMapObject>, With<Tower>)>>,
        picker: Option<Single<(Entity, &ShardSelectionPanel)>>,
        sockets: Query<&ShardSocketOf>,
    ) {
        let Some(focused_tower) = focused_tower else { return };
        let socket = trigger.event_target();
        let Ok(&ShardSocketOf(holder)) = sockets.get(socket) else { return };
        if holder != *focused_tower { return; }
        commands.trigger(RebuildTowerShardSocketsUi);
        if let Some((picker_entity, picker)) = picker.map(Single::into_inner) && picker.socket == socket {
            commands.entity(picker_entity).despawn();
        }
    }
}

/// Modal panel for selecting a shard from the stock or ordering a missing one.
#[derive(Component)]
struct ShardSelectionPanel {
    socket: Entity,
}
impl ShardSelectionPanel {
    fn on_add_construct_shard_selection_panel(
        trigger: On<Add<ShardSelectionPanel>>,
        mut commands: Commands,
        almanach: Res<Almanach>,
        blueprints: Res<ShardBlueprints>,
        stock: Res<Stock>,
        panels: Query<&ShardSelectionPanel>,
        sockets: Query<(&ShardSocket, Has<SocketedShard>, Has<AwaitsShardOrder>)>,
    ) {
        let entity = trigger.entity;
        let Ok(panel) = panels.get(entity) else { return };
        let Ok((socket, is_occupied, awaits_order)) = sockets.get(panel.socket) else { return };
        let footer_action = if awaits_order { Some("Cancel order") } else if is_occupied { Some("Unsocket") } else { None };

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
                        width: Val::Px(PICKER_WIDTH),
                        flex_direction: FlexDirection::Column,
                        padding: UiRect::all(Val::Px(8.)),
                        row_gap: Val::Px(4.),
                        border: UiRect::all(Val::Px(1.)),
                        ..default()
                    },
                    BackgroundColor(PICKER_BACKGROUND),
                    BorderColor::all(PICKER_BORDER),
                ))
                .with_children(|parent| {
                    parent.spawn((
                        Text::new(format!("{} shard: {}", socket.shard_type, socket.description)),
                        TextFont::from_font_size(14.0),
                        TextColor::from(BLUE),
                        Node { margin: UiRect::bottom(Val::Px(4.)), ..default() },
                    ));

                    // Offer stock first; otherwise offer an order if the blueprint and recipe are available.
                    let mut items: Vec<ShardPickerItem> = almanach.shards()
                        .filter(|&(shard, _)| socket.accepts(shard))
                        .filter_map(|(shard, info)| {
                            let count = stock.get(shard);
                            let kind = if count > 0 {
                                ShardPickerItemKind::Socket { count }
                            } else if blueprints.is_unlocked(shard.shard_type) && info.recipe.is_some() {
                                ShardPickerItemKind::Order
                            } else {
                                return None;
                            };
                            Some(ShardPickerItem { shard, kind })
                        })
                        .collect();
                    items.sort_unstable_by_key(|item| item.shard);

                    parent.spawn((
                        Node {
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(4.),
                            max_height: Val::Px(PICKER_LIST_MAX_HEIGHT),
                            overflow: Overflow::scroll_y(),
                            ..default()
                        },
                        ScrollPosition::default(),
                    )).with_children(|list| {
                        if items.is_empty() {
                            list.spawn((
                                Text::new("No shards available"),
                                TextFont::from_font_size(PICKER_TEXT_FONT_SIZE),
                                TextColor::from(PICKER_EMPTY_TEXT_COLOR),
                            ));
                        }
                        for item in items {
                            list.spawn(item);
                        }
                    });

                    if let Some(footer_action) = footer_action {
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
                            BackgroundColor(FOOTER_BUTTON_BACKGROUND),
                            BorderColor::all(FOOTER_BUTTON_BORDER),
                        ))
                        .observe(Self::on_click_cancel_order_or_unsocket)
                        .observe(recolor_background_on::<PointerOver>(FOOTER_BUTTON_HOVER_BACKGROUND))
                        .observe(recolor_background_on::<PointerOut>(FOOTER_BUTTON_BACKGROUND))
                        .with_children(|parent| {
                            parent.spawn((
                                Text::new(footer_action),
                                TextFont::from_font_size(PICKER_TEXT_FONT_SIZE),
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
                        BackgroundColor(CLOSE_BUTTON_BACKGROUND),
                        BorderColor::all(CLOSE_BUTTON_BORDER),
                    ))
                    .observe(Self::on_click_close_shard_selection_panel)
                    .observe(recolor_background_on::<PointerOver>(CLOSE_BUTTON_HOVER_BACKGROUND))
                    .observe(recolor_background_on::<PointerOut>(CLOSE_BUTTON_BACKGROUND))
                    .with_children(|parent| {
                        parent.spawn((
                            Text::new("Close"),
                            TextFont::from_font_size(PICKER_TEXT_FONT_SIZE),
                            TextColor::from(Color::WHITE),
                        ));
                    });
                });
            });
    }

    /// Releases the held shard or detaches the awaited order, allowing active jobs to finish into stock.
    fn on_click_cancel_order_or_unsocket(
        _trigger: On<PointerClick>,
        mut commands: Commands,
        panel: Single<(Entity, &ShardSelectionPanel)>,
    ) {
        let (panel_entity, panel) = panel.into_inner();
        commands.entity(panel_entity).despawn();
        commands.trigger(ShardSocketOperation::unsocket(panel.socket));
    }

    fn on_click_close_shard_selection_panel(
        _trigger: On<PointerClick>,
        mut commands: Commands,
        panel: Single<Entity, With<ShardSelectionPanel>>,
    ) {
        commands.entity(panel.into_inner()).despawn();
    }

    fn on_remove_focused_map_object_close_shard_selection_panel(
        _trigger: On<Remove<FocusedMapObject>>,
        mut commands: Commands,
        panel: Single<Entity, With<ShardSelectionPanel>>,
    ) {
        commands.entity(panel.into_inner()).despawn();
    }
}

/// Selectable shard entry inside `ShardSelectionPanel`.
#[derive(Component)]
struct ShardPickerItem {
    shard: Shard,
    kind: ShardPickerItemKind,
}

/// Action offered by a shard picker row.
#[derive(Clone, Copy)]
enum ShardPickerItemKind {
    /// Sockets a shard held in `Stock`.
    Socket { count: i32 },
    /// Empties the socket and orders a replacement through the global queue.
    Order,
}

impl ShardPickerItem {
    fn on_add_construct_shard_picker_item(
        trigger: On<Add<ShardPickerItem>>,
        mut commands: Commands,
        almanach: Res<Almanach>,
        items: Query<&ShardPickerItem>,
    ) {
        let entity = trigger.entity;
        let Ok(item) = items.get(entity) else { return };
        let name = &almanach.get_resource_info(item.shard).name;
        let (label, background, hover_background, border) = match item.kind {
            ShardPickerItemKind::Socket { count } => (
                format!("{name} x{count}"),
                HELD_ROW_BACKGROUND,
                HELD_ROW_HOVER_BACKGROUND,
                HELD_ROW_BORDER,
            ),
            ShardPickerItemKind::Order => (
                format!("{name} — Order"),
                ORDERED_BACKGROUND,
                ORDER_ROW_HOVER_BACKGROUND,
                GLOBAL_QUEUE_COLOR,
            ),
        };
        commands.entity(entity)
            .insert((
                Button,
                Node {
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    flex_shrink: 0.,
                    padding: UiRect::axes(Val::Px(6.), Val::Px(4.)),
                    border: UiRect::all(Val::Px(1.)),
                    ..default()
                },
                BackgroundColor(background),
                BorderColor::all(border),
            ))
            .observe(Self::on_click_socket_or_order_shard)
            .observe(recolor_background_on::<PointerOver>(hover_background))
            .observe(recolor_background_on::<PointerOut>(background))
            .with_children(|parent| {
                parent.spawn((
                    Text::new(label),
                    TextFont::from_font_size(PICKER_TEXT_FONT_SIZE),
                    TextColor::from(Color::WHITE),
                ));
            });
    }

    #[log_tags(Tag::Shards)]
    fn on_click_socket_or_order_shard(
        trigger: On<PointerClick>,
        mut commands: Commands,
        mut stock: ResMut<Stock>,
        global_queue: Single<Entity, With<GlobalForgingQueue>>,
        items: Query<&ShardPickerItem>,
        panel: Single<(Entity, &ShardSelectionPanel)>,
    ) {
        let entity = trigger.entity;
        let Ok(&ShardPickerItem { shard, kind }) = items.get(entity) else { return };
        let (panel_entity, panel) = panel.into_inner();
        commands.entity(panel_entity).despawn();
        match kind {
            ShardPickerItemKind::Socket { .. } => {
                #[warn_player("{shard} is no longer in stock")]
                if !stock.try_remove((shard, 1)) { return; }
                commands.trigger(ShardSocketOperation::socket(panel.socket, shard));
            }
            ShardPickerItemKind::Order => {
                #[info_player("Ordered {shard}")]
                let order = commands.spawn(BuilderShardOrder::new(shard, OrderLink::Queue(*global_queue))).id();
                commands.trigger(ShardSocketOperation::await_order(panel.socket, order));
            }
        }
    }
}

/// Toggles whether the player disabled the focused building.
#[derive(Component)]
#[require(Button)]
struct BuildingInfoPanelDisableButton;
#[derive(Component)]
struct BuildingInfoPanelDisableButtonIcon;
impl BuildingInfoPanelDisableButton {
    fn on_add_construct_disable_button(
        trigger: On<Add<BuildingInfoPanelDisableButton>>,
        mut commands: Commands,
        asset_server: Res<AssetServer>,
    ) {
        let entity = trigger.entity;
        commands
            .entity(entity)
            .insert((
                Node {
                    width: Val::Px(HEADER_BUTTON_SIZE),
                    height: Val::Px(HEADER_BUTTON_SIZE),
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
                    ImageNode::new(asset_server.load("indicators/disabled.png")).with_color(WHITE.with_alpha(DISABLE_ICON_INACTIVE_ALPHA).into()),
                    BuildingInfoPanelDisableButtonIcon,
                ));
            });
    }

    #[log_tags(Tag::Build)]
    fn on_click_toggle_building_disabled(
        _trigger: On<PointerClick>,
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
        icon.into_inner().color.set_alpha(if is_disabled { DISABLE_ICON_INACTIVE_ALPHA } else { DISABLE_ICON_ACTIVE_ALPHA });
    }
}

/// Requests destroying the focused building.
#[derive(Component)]
#[require(Button)]
struct BuildingInfoPanelDestroyButton;
impl BuildingInfoPanelDestroyButton {
    fn on_add_construct_destroy_button(
        trigger: On<Add<BuildingInfoPanelDestroyButton>>,
        mut commands: Commands,
        asset_server: Res<AssetServer>,
    ) {
        let entity = trigger.entity;
        commands
            .entity(entity)
            .insert((
                Node {
                    width: Val::Px(HEADER_BUTTON_SIZE),
                    height: Val::Px(HEADER_BUTTON_SIZE),
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
        _trigger: On<PointerClick>,
        mut commands: Commands,
        focused_building: Single<Entity, With<FocusedMapObject>>,
    ) {
        commands.trigger(BuildingDestroyRequest(focused_building.into_inner()));
    }
}
