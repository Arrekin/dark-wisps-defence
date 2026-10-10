//! # Forging Panel
//!
//! Full-screen view of the global queue or a Forge queue, selected through the header
//! dropdown. Forge entries show coordinates, status, a preview placeholder, and a global-order toggle.
//! Toggling global orders does not change the selected queue.
//!
//! Three sections show:
//! - Shards to order: unlocked recipes; clicking adds an order to the selected queue.
//! - Forges: all Forges in the global view, or just the selected Forge. Clicking a tile toggles
//!   between its Forge queue and the global queue.
//! - Queued: root orders in the selected queue, including those in progress.
//!
//! Queue, Forge, blueprint, and selection changes trigger a full rebuild. Status, progress, and
//! colors update each frame while the panel is open. Closing resets selection to the global queue.

use bevy::{
    input_focus::{FocusCause, InputFocus, tab_navigation::{NavAction, TabIndex}},
    prelude::*,
    ui_widgets::{
        Activate, Button, MenuAction, MenuButton, MenuEvent, MenuFocusState, MenuItem, MenuPopup,
        popover::{Popover, PopoverAlign, PopoverPlacement, PopoverSide},
    },
};
use strum::IntoEnumIterator;

use almanach::prelude::*;
use buildings::Forge;
use game_core::prelude::{GridCoords, IsOperational, Shard, ShardTier};
use logging::prelude::*;
use resources::prelude::Stock;
use shards::{
    blueprints::ShardBlueprintAcquired,
    orders::{
        BuilderShardOrder, ForgeCurrentOrder, ForgingPanelSelectRequest, ForgingProgress, ForgingQueue,
        GlobalForgingQueue, InForgingQueue, OrderLink, ShardOrder, ShardOrderCancelMode, ShardOrderCancelRequest,
        WorksGlobalQueue,
    },
    prelude::*,
};
use states::prelude::UiInteraction;
use widgets::{
    common::utils::{recolor_background_on, set_text_if_changed, set_ui_free_on},
    palette::ABYSS_BACKGROUND,
    prelude::{BuilderCloseButton, BuilderFillBar, BuilderFullPriceCostStrip, FillBar, TextRole},
};

use crate::orders::OrderTreeParam;

pub(crate) struct ForgingPanelPlugin;
impl Plugin for ForgingPanelPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<ForgingPanelSelection>()
            .add_systems(Startup, spawn_forging_panel)
            .add_observer(on_forging_panel_select_request_do_so)
            .add_systems(OnEnter(UiInteraction::ForgingPanel), show_panel)
            .add_systems(OnExit(UiInteraction::ForgingPanel), hide_panel)
            .add_systems(Update, (
                rebuild_panel.run_if(resource_exists::<ForgingPanelStale>),
                update_queue_entries,
                update_forge_tiles,
                update_queued_order_tiles,
            ).chain().run_if(in_state(UiInteraction::ForgingPanel)))
            .add_observer(mark_panel_stale_on::<Insert<InForgingQueue>>)
            .add_observer(mark_panel_stale_on::<Remove<InForgingQueue>>)
            .add_observer(mark_panel_stale_on::<Add<Forge>>)
            .add_observer(mark_panel_stale_on::<Remove<Forge>>)
            .add_observer(mark_panel_stale_on::<ShardBlueprintAcquired>);
    }
}

// Layout
const PANEL_PADDING: f32 = 24.0;
const PANEL_Z_INDEX: i32 = 100;
const HEADER_HEIGHT: f32 = 48.0;
const HEADER_FONT_SIZE: f32 = 28.0;
const BAND_TITLE_FONT_SIZE: f32 = 16.0;
const CLOSE_BUTTON_SIZE: f32 = 28.0;
const TILE_GAP: f32 = 8.0;
const ICON_SIZE: f32 = 40.0;
const TEXT_FONT_SIZE: f32 = 12.0;
const PREVIEW_SIZE: f32 = 32.0;
const QUEUE_ENTRY_MIN_WIDTH: f32 = 320.0;
const QUEUE_LIST_GAP: f32 = 4.0;
const SOURCE_BORDER_WIDTH: f32 = 2.0;
const FILL_BAR_WIDTH: f32 = 140.0;
const FILL_BAR_HEIGHT: f32 = 10.0;

// Colors
const TILE_BACKGROUND: Color = Color::srgba(0.08, 0.08, 0.14, 0.95);
const TILE_HOVER_BACKGROUND: Color = Color::srgba(0.18, 0.2, 0.35, 0.95);
const UNAFFORDABLE_BACKGROUND: Color = Color::srgba(0.35, 0.08, 0.08, 0.95); // red
const CANCEL_BACKGROUND: Color = Color::srgba(0.4, 0.15, 0.15, 0.9);
const CANCEL_HOVER_BACKGROUND: Color = Color::srgba(0.55, 0.2, 0.2, 0.95);
const FILL_COLOR: Color = Color::srgba(0.8, 0.6, 0.1, 1.0); // amber
const FILL_BAR_BACKGROUND: Color = Color::srgba(0.1, 0.1, 0.1, 0.8);
const FILL_BAR_BORDER: Color = Color::srgba(0.4, 0.4, 0.3, 1.);
const STATUS_COLOR: Color = Color::srgb(0.6, 0.65, 0.75);
const QUEUE_LIST_BACKGROUND: Color = Color::srgba(0.04, 0.04, 0.08, 0.98);
const PREVIEW_BACKGROUND: Color = Color::srgba(0.02, 0.02, 0.04, 1.0);
const PREVIEW_BORDER: Color = Color::srgba(0.3, 0.3, 0.4, 1.0);
const TOGGLE_BACKGROUND: Color = Color::srgba(0.15, 0.15, 0.25, 0.9);
const TOGGLE_HOVER_BACKGROUND: Color = Color::srgba(0.25, 0.3, 0.5, 0.95);

/// Selected Forge, or `None` for the global queue.
#[derive(Resource, Default)]
pub(crate) struct ForgingPanelSelection(Option<Entity>);
impl ForgingPanelSelection {
    pub fn forge(&self) -> Option<Entity> {
        self.0
    }

    pub fn set(&mut self, forge: Option<Entity>) {
        self.0 = forge;
    }

    pub fn clear(&mut self) {
        self.0 = None;
    }
}

/// Requests a panel rebuild, coalescing multiple changes before the next update.
#[derive(Resource)]
struct ForgingPanelStale;

#[derive(Component)]
struct ForgingPanelRoot;

/// Content row of the band listing shards to order.
#[derive(Component)]
struct ShardsToOrderBand;

/// Content row for Forge status and job tiles.
#[derive(Component)]
struct ForgesBand;

/// Content row of the band listing the queued orders.
#[derive(Component)]
struct QueuedBand;

// ============================================================================
// SPAWN / SHOW / HIDE
// ============================================================================

fn spawn_forging_panel(mut commands: Commands) {
    let title = commands.spawn((
        Text::new("Forging"),
        TextRole::Heading.font(HEADER_FONT_SIZE),
    )).id();
    let close_button = commands.spawn((
        BuilderCloseButton::default(),
        Node { width: Val::Px(CLOSE_BUTTON_SIZE), height: Val::Px(CLOSE_BUTTON_SIZE), ..default() },
    )).observe(set_ui_free_on::<PointerClick>).id();
    let queue_dropdown = spawn_queue_dropdown(&mut commands);
    let header = commands.spawn(Node {
        width: Val::Percent(100.),
        height: Val::Px(HEADER_HEIGHT),
        flex_direction: FlexDirection::Row,
        justify_content: JustifyContent::SpaceBetween,
        align_items: AlignItems::Center,
        ..default()
    }).add_children(&[title, queue_dropdown, close_button]).id();

    let shards_to_order = spawn_band(&mut commands, "Shards to order", ShardsToOrderBand);
    let forges_band = spawn_band(&mut commands, "Forges", ForgesBand);
    let queued = spawn_band(&mut commands, "Queued", QueuedBand);

    commands.spawn((
        ForgingPanelRoot,
        Node {
            width: Val::Percent(100.),
            height: Val::Percent(100.),
            position_type: PositionType::Absolute,
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(Val::Px(PANEL_PADDING)),
            row_gap: Val::Px(PANEL_PADDING),
            display: Display::None,
            ..default()
        },
        BackgroundColor::from(ABYSS_BACKGROUND),
        GlobalZIndex(PANEL_Z_INDEX),
    )).add_children(&[header, shards_to_order, forges_band, queued]);
}

/// Spawns a heading and a horizontally scrollable row marked with `marker`.
fn spawn_band(commands: &mut Commands, title: &str, marker: impl Component) -> Entity {
    let title = commands.spawn((
        Text::new(title),
        TextRole::Heading.font(BAND_TITLE_FONT_SIZE),
    )).id();
    let content = commands.spawn((
        marker,
        Node {
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(TILE_GAP),
            overflow: Overflow::scroll_x(),
            ..default()
        },
        ScrollPosition::default(),
    )).id();
    commands.spawn(Node {
        flex_direction: FlexDirection::Column,
        row_gap: Val::Px(TILE_GAP),
        ..default()
    }).add_children(&[title, content]).id()
}

fn show_panel(mut commands: Commands, root: Single<&mut Node, With<ForgingPanelRoot>>) {
    root.into_inner().display = Display::Flex;
    commands.insert_resource(ForgingPanelStale);
}

fn hide_panel(mut selection: ResMut<ForgingPanelSelection>, root: Single<&mut Node, With<ForgingPanelRoot>>) {
    root.into_inner().display = Display::None;
    selection.clear();
}

fn on_forging_panel_select_request_do_so(
    trigger: On<ForgingPanelSelectRequest>,
    mut commands: Commands,
    mut selection: ResMut<ForgingPanelSelection>,
) {
    selection.set(trigger.forge);
    commands.insert_resource(ForgingPanelStale);
}

// ============================================================================
// STALENESS
// ============================================================================

fn mark_panel_stale_on<P: EventPattern>(_trigger: On<P>, mut commands: Commands) {
    commands.insert_resource(ForgingPanelStale);
}

// ============================================================================
// REBUILD
// ============================================================================

fn rebuild_panel(
    mut commands: Commands,
    almanach: Res<Almanach>,
    blueprints: Res<ShardBlueprints>,
    mut selection: ResMut<ForgingPanelSelection>,
    global_queue: Single<Entity, With<GlobalForgingQueue>>,
    queue_dropdown_button: Single<Entity, With<QueueDropdownButton>>,
    queue_dropdown_list: Single<Entity, With<QueueDropdownList>>,
    shards_to_order_band: Single<Entity, With<ShardsToOrderBand>>,
    forges_band: Single<Entity, With<ForgesBand>>,
    queued_band: Single<Entity, With<QueuedBand>>,
    forges: Query<(Entity, &GridCoords), With<Forge>>,
    queues: Query<&ForgingQueue>,
    orders: Query<&ShardOrder>,
) {
    commands.remove_resource::<ForgingPanelStale>();
    // Losing the selected Forge returns the panel to the global queue.
    if selection.forge().is_some_and(|forge| !forges.contains(forge)) {
        selection.clear();
    }
    let selected_forge = selection.forge();
    let shown_queue = selected_forge.unwrap_or(*global_queue);
    let mut sorted_forges: Vec<(Entity, GridCoords)> = forges.iter().map(|(forge, &coords)| (forge, coords)).collect();
    sorted_forges.sort_by_key(|&(_, coords)| (coords.y, coords.x));

    // Reuse the entry layout for the current selection and the dropdown options.
    let shown_entry = spawn_queue_entry(&mut commands, selected_forge.and_then(|selected_forge| sorted_forges.iter().copied().find(|&(forge, _)| forge == selected_forge)));
    commands.entity(*queue_dropdown_button).despawn_children().add_child(shown_entry);
    commands.entity(*queue_dropdown_list).despawn_children();
    for forge in std::iter::once(None).chain(sorted_forges.iter().copied().map(Some)) {
        let entry = spawn_queue_entry(&mut commands, forge);
        commands.entity(entry)
            .insert((QueueEntry { forge: forge.map(|(forge, _)| forge) }, MenuItem, TabIndex(0)))
            .observe(recolor_background_on::<PointerOver>(TILE_HOVER_BACKGROUND))
            .observe(recolor_background_on::<PointerOut>(TILE_BACKGROUND))
            .observe(on_activate_show_entry_queue);
        commands.entity(*queue_dropdown_list).add_child(entry);
    }

    // Shards to order: one column per unlocked shard type, one row per tier.
    commands.entity(*shards_to_order_band).despawn_children();
    for shard_type in blueprints.iter() {
        let column = commands.spawn(Node {
            flex_direction: FlexDirection::Column,
            flex_shrink: 0.,
            row_gap: Val::Px(TILE_GAP),
            ..default()
        }).id();
        commands.entity(*shards_to_order_band).add_child(column);
        for tier in ShardTier::iter() {
            let shard = Shard::new(shard_type, tier);
            if almanach.get_shard_info(shard).recipe.is_none() { continue; }
            let button = spawn_order_shard_button(&mut commands, &almanach, shard, shown_queue);
            commands.entity(column).add_child(button);
        }
    }

    // Show all Forges in the global view, including idle and non-operational ones; otherwise show only the selected Forge.
    commands.entity(*forges_band).despawn_children();
    for (forge, coords) in sorted_forges.into_iter().filter(|&(forge, _)| selected_forge.is_none_or(|selected_forge| selected_forge == forge)) {
        let tile = spawn_forge_tile(&mut commands, forge, coords);
        commands.entity(*forges_band).add_child(tile);
    }

    // Queued: the shown queue's orders in placement order.
    commands.entity(*queued_band).despawn_children();
    for order in queues.get(shown_queue).into_iter().flat_map(|queue| queue.iter()) {
        let Ok(&ShardOrder(shard)) = orders.get(order) else { continue; };
        let tile = spawn_queued_order_tile(&mut commands, &almanach, order, shard);
        commands.entity(*queued_band).add_child(tile);
    }
}

/// Lays out a shard icon, a text column, and trailing controls from left to right.
fn spawn_shard_tile(commands: &mut Commands, icon: Handle<Image>, texts: &[Entity], trailing: &[Entity]) -> Entity {
    let icon = commands.spawn((
        Node { width: Val::Px(ICON_SIZE), height: Val::Px(ICON_SIZE), ..default() },
        ImageNode::new(icon),
    )).id();
    let text_column = commands.spawn(Node {
        flex_direction: FlexDirection::Column,
        row_gap: Val::Px(2.),
        ..default()
    }).add_children(texts).id();
    let tile = commands.spawn((
        Node {
            flex_direction: FlexDirection::Row,
            flex_shrink: 0.,
            align_items: AlignItems::Center,
            column_gap: Val::Px(TILE_GAP),
            padding: UiRect::all(Val::Px(6.)),
            border_radius: BorderRadius::all(Val::Px(4.)),
            ..default()
        },
        BackgroundColor::from(TILE_BACKGROUND),
    )).add_children(&[icon, text_column]).id();
    commands.entity(tile).add_children(trailing);
    tile
}

fn spawn_text(commands: &mut Commands, text: impl Into<String>, color: Color) -> Entity {
    commands.spawn((
        Text::new(text),
        TextFont::from_font_size(TEXT_FONT_SIZE),
        TextColor::from(color),
        TextLayout::no_wrap(),
    )).id()
}

// ============================================================================
// QUEUE DROPDOWN
// ============================================================================

/// Dropdown button displaying the selected queue.
#[derive(Component)]
struct QueueDropdownButton;

/// Queue options displayed over the panel while the dropdown is open.
#[derive(Component)]
struct QueueDropdownList;

/// Selects a Forge queue, or the global queue for `None`.
#[derive(Component)]
struct QueueEntry {
    forge: Option<Entity>,
}

/// The status text in a Forge's entry.
#[derive(Component)]
struct ForgeStatusText {
    forge: Entity,
}

/// Toggles global orders for a Forge. The widget button stops event propagation so clicking it
/// does not select the containing queue entry.
#[derive(Component)]
#[require(Button)]
struct GlobalQueueToggle {
    forge: Entity,
}

fn spawn_queue_dropdown(commands: &mut Commands) -> Entity {
    let button = commands.spawn((
        QueueDropdownButton,
        MenuButton,
        Node { border_radius: BorderRadius::all(Val::Px(4.)), ..default() },
    )).id();
    let list = commands.spawn((
        QueueDropdownList,
        MenuPopup::default(),
        Node {
            position_type: PositionType::Absolute,
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(QUEUE_LIST_GAP),
            padding: UiRect::all(Val::Px(QUEUE_LIST_GAP)),
            border_radius: BorderRadius::all(Val::Px(4.)),
            ..default()
        },
        BackgroundColor::from(QUEUE_LIST_BACKGROUND),
        Visibility::Hidden,
        GlobalZIndex(PANEL_Z_INDEX + 1),
        Popover {
            positions: vec![
                PopoverPlacement { side: PopoverSide::Bottom, align: PopoverAlign::Start, gap: QUEUE_LIST_GAP },
                PopoverPlacement { side: PopoverSide::Top, align: PopoverAlign::Start, gap: QUEUE_LIST_GAP },
            ],
            window_margin: PANEL_PADDING,
        },
    )).id();
    commands.spawn(Node::default())
        .add_children(&[button, list])
        .observe(on_menu_event_open_or_close_queue_list)
        .id()
}

/// Builds one queue's entry: "Global queue", or a Forge with its preview placeholder, coordinates,
/// status and global-queue toggle.
fn spawn_queue_entry(commands: &mut Commands, forge: Option<(Entity, GridCoords)>) -> Entity {
    let entry = commands.spawn((
        Node {
            min_width: Val::Px(QUEUE_ENTRY_MIN_WIDTH),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(TILE_GAP),
            padding: UiRect::all(Val::Px(QUEUE_LIST_GAP)),
            border_radius: BorderRadius::all(Val::Px(4.)),
            ..default()
        },
        BackgroundColor::from(TILE_BACKGROUND),
    )).id();
    let Some((forge, coords)) = forge else {
        let label = spawn_text(commands, "Global queue", Color::WHITE);
        commands.entity(entry).add_child(label);
        return entry;
    };

    let preview = commands.spawn((
        Node {
            width: Val::Px(PREVIEW_SIZE),
            height: Val::Px(PREVIEW_SIZE),
            border: UiRect::all(Val::Px(1.)),
            ..default()
        },
        BackgroundColor::from(PREVIEW_BACKGROUND),
        BorderColor::all(PREVIEW_BORDER),
    )).id();
    let coords_text = spawn_text(commands, format!("Forge {coords}"), Color::WHITE);
    let status = spawn_text(commands, "", STATUS_COLOR);
    commands.entity(status).insert(ForgeStatusText { forge });
    let texts = commands.spawn(Node {
        flex_direction: FlexDirection::Column,
        flex_grow: 1.,
        row_gap: Val::Px(2.),
        ..default()
    }).add_children(&[coords_text, status]).id();
    let toggle = commands.spawn((
        GlobalQueueToggle { forge },
        Text::new(""),
        TextFont::from_font_size(TEXT_FONT_SIZE),
        TextLayout::no_wrap(),
        Node {
            padding: UiRect::axes(Val::Px(8.), Val::Px(4.)),
            border_radius: BorderRadius::all(Val::Px(3.)),
            ..default()
        },
        BackgroundColor::from(TOGGLE_BACKGROUND),
    ))
        .observe(recolor_background_on::<PointerOver>(TOGGLE_HOVER_BACKGROUND))
        .observe(recolor_background_on::<PointerOut>(TOGGLE_BACKGROUND))
        .observe(on_click_toggle_global_queue)
        .id();
    commands.entity(entry).add_children(&[preview, texts, toggle]);
    entry
}

/// Shows or hides the list. The menu widget sends these events from the button and the list.
fn on_menu_event_open_or_close_queue_list(
    mut trigger: On<MenuEvent>,
    mut commands: Commands,
    mut focus: ResMut<InputFocus>,
    list: Single<(Entity, &Visibility), With<QueueDropdownList>>,
    button: Single<Entity, With<QueueDropdownButton>>,
) {
    trigger.propagate(false);
    let (list, &visibility) = list.into_inner();
    match trigger.action {
        MenuAction::Open(navigation) => { commands.entity(list).insert((Visibility::Visible, MenuFocusState::Opening(navigation))); }
        MenuAction::Toggle if visibility == Visibility::Visible => { commands.entity(list).insert(Visibility::Hidden); }
        MenuAction::Toggle => { commands.entity(list).insert((Visibility::Visible, MenuFocusState::Opening(NavAction::First))); }
        MenuAction::CloseAll => { commands.entity(list).insert(Visibility::Hidden); }
        MenuAction::FocusRoot => focus.set(*button, FocusCause::Navigated),
    }
}

fn on_activate_show_entry_queue(
    trigger: On<Activate>,
    mut commands: Commands,
    entries: Query<&QueueEntry>,
) {
    let Ok(&QueueEntry { forge }) = entries.get(trigger.entity) else { return; };
    commands.trigger(ForgingPanelSelectRequest { forge });
}

#[log_tags(Tag::Forge)]
fn on_click_toggle_global_queue(
    trigger: On<PointerClick>,
    toggles: Query<&GlobalQueueToggle>,
    mut forges: Query<(&GridCoords, &mut WorksGlobalQueue)>,
) {
    let Ok(&GlobalQueueToggle { forge }) = toggles.get(trigger.entity) else { return; };
    let Ok((coords, mut works_global)) = forges.get_mut(forge) else { return; };
    works_global.toggle();
    info_player!("Forge at {coords}: {}", works_global.label());
}

/// Shows each Forge's status and global-queue setting in its entries.
fn update_queue_entries(
    almanach: Res<Almanach>,
    forges: Query<(Has<IsOperational>, Option<&ForgeCurrentOrder>, &WorksGlobalQueue), With<Forge>>,
    orders: Query<&ShardOrder>,
    mut status_texts: Query<(&ForgeStatusText, &mut Text), Without<GlobalQueueToggle>>,
    mut toggles: Query<(&GlobalQueueToggle, &mut Text), Without<ForgeStatusText>>,
) {
    for (&ForgeStatusText { forge }, mut text) in status_texts.iter_mut() {
        let Ok((operational, current_order, _)) = forges.get(forge) else { continue; };
        let job = current_order.and_then(|current_order| orders.get(current_order.order()).ok());
        let status = match (operational, job) {
            (false, _) => "Not operational".to_string(),
            (true, None) => "Idle".to_string(),
            (true, Some(&ShardOrder(shard))) => format!("Forging {}", almanach.get_resource_info(shard).name),
        };
        set_text_if_changed(&mut text, &status);
    }
    for (&GlobalQueueToggle { forge }, mut text) in toggles.iter_mut() {
        let Ok((_, _, works_global)) = forges.get(forge) else { continue; };
        set_text_if_changed(&mut text, works_global.label());
    }
}

// ============================================================================
// SHARDS TO ORDER
// ============================================================================

/// Places an order for its shard into `queue` on click.
#[derive(Component)]
#[require(Button)]
struct OrderShardButton {
    shard: Shard,
    queue: Entity,
}

fn spawn_order_shard_button(commands: &mut Commands, almanach: &Almanach, shard: Shard, queue: Entity) -> Entity {
    let info = almanach.get_resource_info(shard);
    let cost = almanach.get_shard_info(shard).recipe.as_ref().map(|recipe| recipe.cost().collect()).unwrap_or_default();
    let name = spawn_text(commands, info.name.clone(), Color::WHITE);
    let cost_strip = commands.spawn(BuilderFullPriceCostStrip(cost)).id();
    let button = spawn_shard_tile(commands, info.icon.clone(), &[name, cost_strip], &[]);
    commands.entity(button)
        .insert(OrderShardButton { shard, queue })
        .observe(recolor_background_on::<PointerOver>(TILE_HOVER_BACKGROUND))
        .observe(recolor_background_on::<PointerOut>(TILE_BACKGROUND))
        .observe(on_click_place_shard_order);
    button
}

#[log_tags(Tag::Shards)]
fn on_click_place_shard_order(
    trigger: On<PointerClick>,
    mut commands: Commands,
    buttons: Query<&OrderShardButton>,
) {
    let Ok(&OrderShardButton { shard, queue }) = buttons.get(trigger.entity) else { return; };
    #[info_player("Ordered {shard}")]
    commands.spawn(BuilderShardOrder::new(shard, OrderLink::Queue(queue)));
}

// ============================================================================
// FORGES
// ============================================================================

/// Displays a Forge's coordinates, status, and job progress. Clicking toggles selection.
#[derive(Component)]
#[require(Button)]
struct ForgeTile {
    forge: Entity,
    status: Entity,
    fill: Entity,
}

fn spawn_forge_tile(commands: &mut Commands, forge: Entity, coords: GridCoords) -> Entity {
    let coords_text = spawn_text(commands, format!("Forge {coords}"), Color::WHITE);
    let status = spawn_text(commands, "", STATUS_COLOR);
    let fill = commands.spawn(
        BuilderFillBar::default()
            .with_background_color(FILL_BAR_BACKGROUND)
            .with_border(FILL_BAR_BORDER, UiRect::all(Val::Px(1.)))
            .with_border_radius(BorderRadius::all(Val::Px(2.)))
            .with_fill_color(FILL_COLOR),
    ).id();
    let bar = commands.spawn(Node { width: Val::Px(FILL_BAR_WIDTH), height: Val::Px(FILL_BAR_HEIGHT), ..default() }).add_child(fill).id();
    commands.spawn((
        ForgeTile { forge, status, fill },
        Node {
            flex_direction: FlexDirection::Column,
            flex_shrink: 0.,
            row_gap: Val::Px(4.),
            padding: UiRect::all(Val::Px(6.)),
            border: UiRect::all(Val::Px(SOURCE_BORDER_WIDTH)),
            border_radius: BorderRadius::all(Val::Px(4.)),
            ..default()
        },
        BackgroundColor::from(TILE_BACKGROUND),
        BorderColor::all(Color::NONE),
    ))
        .add_children(&[coords_text, status, bar])
        .observe(recolor_background_on::<PointerOver>(TILE_HOVER_BACKGROUND))
        .observe(recolor_background_on::<PointerOut>(TILE_BACKGROUND))
        .observe(on_click_toggle_forge_selection)
        .id()
}

fn on_click_toggle_forge_selection(
    trigger: On<PointerClick>,
    mut commands: Commands,
    selection: Res<ForgingPanelSelection>,
    tiles: Query<&ForgeTile>,
) {
    let Ok(&ForgeTile { forge, .. }) = tiles.get(trigger.entity) else { return; };
    let forge = (selection.forge() != Some(forge)).then_some(forge);
    commands.trigger(ForgingPanelSelectRequest { forge });
}

/// Shows each Forge's job: its status, its progress, and its source queue as the tile's border.
fn update_forge_tiles(
    mut tiles: Query<(&ForgeTile, &mut BorderColor)>,
    forges: Query<(Has<IsOperational>, Option<&ForgeCurrentOrder>), With<Forge>>,
    jobs: Query<(Entity, &ShardOrder, &ForgingProgress)>,
    order_tree: OrderTreeParam,
    mut texts: Query<&mut Text>,
    mut fills: Query<&mut FillBar>,
) {
    for (tile, mut border) in tiles.iter_mut() {
        let Ok((operational, current_order)) = forges.get(tile.forge) else { continue; };
        // Ingredient jobs inherit their root order's queue and display context.
        let job = current_order.and_then(|current_order| jobs.get(current_order.order()).ok())
            .map(|(order, &ShardOrder(shard), progress)| (order, order_tree.parents.root_ancestor(order), shard, progress));

        let status = match (operational, job) {
            (false, _) => "Not operational".to_string(),
            (true, None) => "Idle".to_string(),
            (true, Some((order, root, shard, progress))) => {
                let name = &order_tree.almanach.get_resource_info(shard).name;
                // Identify the root order so players can see why an ingredient is being forged.
                match order_tree.orders.get(root) {
                    Ok(&ShardOrder(root_shard)) if root != order => format!(
                        "{name} for {} — {:.1}s",
                        order_tree.almanach.get_resource_info(root_shard).name,
                        progress.remaining_secs(),
                    ),
                    _ => format!("{name} — {:.1}s", progress.remaining_secs()),
                }
            }
        };
        if let Ok(mut text) = texts.get_mut(tile.status) {
            set_text_if_changed(&mut text, &status);
        }
        let source_color = job.and_then(|(_, root, _, _)| order_tree.queue_links.get(root).ok()).map_or(Color::NONE, |queue| queue.source_color(tile.forge));
        border.set_if_neq(BorderColor::all(source_color));
        if let Ok(mut fill) = fills.get_mut(tile.fill) {
            fill.fill_fraction = job.map_or(0., |(_, _, _, progress)| progress.fraction());
        }
    }
}

// ============================================================================
// QUEUED
// ============================================================================

/// A root order's tile. Red while waiting for ingredients or pickup resources.
#[derive(Component)]
struct QueuedOrderTile {
    order: Entity,
    status: Entity,
}

fn spawn_queued_order_tile(commands: &mut Commands, almanach: &Almanach, order: Entity, shard: Shard) -> Entity {
    let info = almanach.get_resource_info(shard);
    let cost = almanach.get_shard_info(shard).recipe.as_ref().map(|recipe| recipe.cost().collect()).unwrap_or_default();
    let name = spawn_text(commands, info.name.clone(), Color::WHITE);
    let status = spawn_text(commands, "", STATUS_COLOR);
    let cost_strip = commands.spawn(BuilderFullPriceCostStrip(cost)).id();

    let cancel_button = spawn_cancel_control(commands, "Cancel", CancelControl::Cancel(order));
    let cancel_controls = commands.spawn(Node {
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        column_gap: Val::Px(TILE_GAP),
        ..default()
    }).add_child(cancel_button).id();

    let tile = spawn_shard_tile(commands, info.icon.clone(), &[name, status, cost_strip], &[cancel_controls]);
    commands.entity(tile).insert(QueuedOrderTile { order, status });
    tile
}

/// A waiting order is affordable when `Stock` holds its pickup cost and its ingredient orders are
/// ready.
fn update_queued_order_tiles(
    stock: Res<Stock>,
    mut tiles: Query<(&QueuedOrderTile, &mut BackgroundColor)>,
    orders: Query<(&ShardOrder, Option<&ForgingProgress>)>,
    order_tree: OrderTreeParam,
    mut texts: Query<&mut Text>,
) {
    for (tile, mut background) in tiles.iter_mut() {
        let Ok((&ShardOrder(shard), progress)) = orders.get(tile.order) else { continue; };
        let ingredients_ready = order_tree.are_ready(tile.order);
        let pickup_affordable = order_tree.almanach.get_shard_info(shard).recipe.as_ref().is_some_and(|recipe| stock.has_all(&recipe.pickup_cost));

        let status = match progress {
            Some(progress) => format!("In progress — {:.1}s", progress.remaining_secs()),
            None if !ingredients_ready => "Gathering ingredients".to_string(),
            None if !pickup_affordable => "Not enough resources".to_string(),
            None => "Waiting for a Forge".to_string(),
        };
        if let Ok(mut text) = texts.get_mut(tile.status) {
            set_text_if_changed(&mut text, &status);
        }
        let affordable = ingredients_ready && pickup_affordable;
        let wanted = if progress.is_none() && !affordable { UNAFFORDABLE_BACKGROUND } else { TILE_BACKGROUND };
        background.set_if_neq(BackgroundColor::from(wanted));
    }
}

/// A cancellation action. Confirmation buttons replace the contents of the parent control row.
#[derive(Component, Clone, Copy)]
#[require(Button)]
enum CancelControl {
    Cancel(Entity),
    Confirm(Entity, ShardOrderCancelMode),
    Keep(Entity),
}

fn spawn_cancel_control(commands: &mut Commands, label: &str, control: CancelControl) -> Entity {
    let label = spawn_text(commands, label, Color::WHITE);
    commands.spawn((
        control,
        Node {
            padding: UiRect::axes(Val::Px(8.), Val::Px(4.)),
            border_radius: BorderRadius::all(Val::Px(3.)),
            ..default()
        },
        BackgroundColor::from(CANCEL_BACKGROUND),
    ))
        .add_child(label)
        .observe(recolor_background_on::<PointerOver>(CANCEL_HOVER_BACKGROUND))
        .observe(recolor_background_on::<PointerOut>(CANCEL_BACKGROUND))
        .observe(on_click_cancel_control)
        .id()
}

fn on_click_cancel_control(
    trigger: On<PointerClick>,
    mut commands: Commands,
    buttons: Query<(&CancelControl, &ChildOf)>,
    order_tree: OrderTreeParam,
) {
    let Ok((&control, &ChildOf(controls))) = buttons.get(trigger.entity) else { return; };
    match control {
        CancelControl::Cancel(order) if order_tree.in_progress.contains(order) => {
            let question = [
                spawn_text(&mut commands, "Cancelling loses progress and spent resources.", STATUS_COLOR),
                spawn_cancel_control(&mut commands, "Discard job", CancelControl::Confirm(order, ShardOrderCancelMode::Hard)),
                spawn_cancel_control(&mut commands, "Keep order", CancelControl::Keep(order)),
            ];
            commands.entity(controls).despawn_children().add_children(&question);
        }
        CancelControl::Cancel(order) if order_tree.has_jobs_in_progress_below(order) => {
            let question = [
                spawn_text(&mut commands, "Ingredients are being forged. Finish them for stock or discard without refunds?", STATUS_COLOR),
                spawn_cancel_control(&mut commands, "Finish jobs", CancelControl::Confirm(order, ShardOrderCancelMode::Soft)),
                spawn_cancel_control(&mut commands, "Discard jobs", CancelControl::Confirm(order, ShardOrderCancelMode::Hard)),
                spawn_cancel_control(&mut commands, "Keep order", CancelControl::Keep(order)),
            ];
            commands.entity(controls).despawn_children().add_children(&question);
        }
        CancelControl::Cancel(order) => commands.trigger(ShardOrderCancelRequest { order, mode: ShardOrderCancelMode::Hard }),
        CancelControl::Confirm(order, mode) => commands.trigger(ShardOrderCancelRequest { order, mode }),
        CancelControl::Keep(order) => {
            let cancel_button = spawn_cancel_control(&mut commands, "Cancel", CancelControl::Cancel(order));
            commands.entity(controls).despawn_children().add_child(cancel_button);
        }
    }
}
