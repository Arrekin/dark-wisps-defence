//! # Forge
//!
//! Spawns and saves Forge buildings and displays their current job and queue controls.
//! Order processing lives in `shards_internal/src/orders.rs`.

use bevy::{
    platform::collections::HashMap,
    prelude::*,
};

use alteration::{
    effects::prelude::*,
    modifiers::prelude::*,
};
use almanach::{BuildingInfo, ObjectPresentation, prelude::*};
use buildings::prelude::*;
use game_core::prelude::*;
use grids::placement::{annotate_non_empty, PlacementModes, PlaceRequest};
use hud::prelude::*;
use logging::prelude::*;
use persistence::prelude::*;
use resources::prelude::*;
use shards::orders::{ForgeCurrentOrder, ForgingPanelSelectRequest, ForgingProgress, InForgingQueue, IngredientOf, ShardOrder, WorksGlobalQueue};
use states::prelude::*;
use widgets::{
    prelude::{BuilderFillBar, FillBar},
    common::utils::{recolor_background_on, set_text_if_changed},
};

use crate::{
    common::*,
    info_panel::BuildingInfoPanelEnabledTrigger,
    tooltip::building_tooltip,
};

pub(crate) struct ForgePlugin;
impl Plugin for ForgePlugin {
    fn build(&self, app: &mut App) {
        let asset_server = app.world().resource::<AssetServer>();
        let almanach_info = BuilderForge::almanach_info(asset_server);
        app
            .add_systems(Update, (
                ForgeInfoPanel::update_progress,
                ForgeInfoPanel::update_global_queue_toggle_label,
            ).run_if(in_state(UiInteraction::DisplayInfoPanel)))
            .add_observer(BuilderForge::on_builder_add_spawn_forge)
            .add_observer(on_forge_place_request_do_so)
            .add_observer(ForgeInfoPanel::on_building_info_panel_enabled_toggle_subpanel_visibility)
            .add_observer(ForgeInfoPanel::on_rebuild_forge_ui_do_so)
            .add_observer(GlobalQueueToggleButton::on_add_watch_click)
            .add_observer(OpenForgingPanelButton::on_add_watch_click)
            .add_observer(rebuild_forge_ui_on::<Insert, ForgeCurrentOrder>)
            .add_observer(rebuild_forge_ui_on::<Remove, ForgeCurrentOrder>)
            .add_systems(CollectSave, collect_forges)
            .register_loader(MapLoadingStage::SpawnMapElements, "forges", load_forges)
            .register_building(BuildingType::Forge, almanach_info);
    }
}

#[derive(Component, SSS)]
pub(crate) struct BuilderForge {
    pub grid_position: GridCoords,
    /// Restored integrity points, or `None` to use the building baseline.
    pub integrity_points: Option<IntegrityPoints>,
    /// Set when the player disabled this building. `None` on fresh spawn.
    pub disabled_by_player: Option<DisabledByPlayer>,
}
impl BuilderForge {
    pub fn almanach_info(asset_server: &AssetServer) -> BuildingInfo {
        BuildingInfo {
            name: "Forge".to_string(),
            description: "Forges shards from resources.".to_string(),
            sprite: asset_server.load("buildings/forge.png"),
            top_sprite: None,
            grid_imprint: GridImprint::Rectangle { width: 3, height: 3 },
            cost: vec![ResourceAmount::new(ResourceType::DarkOre, 100)],
            baseline: HashMap::from([(ModifierType::MaxIntegrityPoints, 100.)]),
            sockets: vec![],
            validate: building_validator,
            annotate: annotate_non_empty,
            placement: PlacementModes::default(),
            presentation: ObjectPresentation {
                tooltip: Some(building_tooltip),
            },
        }
    }

    pub fn new(grid_position: GridCoords) -> Self {
        Self { grid_position, integrity_points: None, disabled_by_player: None }
    }
    pub fn with_integrity_points(mut self, integrity_points: f32) -> Self { self.integrity_points = Some(IntegrityPoints::new(integrity_points)); self }
    pub fn with_disabled_by_player(mut self, disabled_by_player: bool) -> Self { self.disabled_by_player = disabled_by_player.then_some(DisabledByPlayer); self }

    pub fn on_builder_add_spawn_forge(
        trigger: On<Add, BuilderForge>,
        mut commands: Commands,
        almanach: Res<Almanach>,
        builders: Query<&BuilderForge>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return; };

        let building_info = almanach.get_building_info(BuildingType::Forge);
        let grid_imprint = building_info.grid_imprint;

        commands.entity(entity)
            .remove::<BuilderForge>()
            .insert_some(builder.integrity_points)
            .insert_some(builder.disabled_by_player)
            .insert((
                Forge,
                WorksGlobalQueue::default(),
                Sprite {
                    image: building_info.sprite.clone(),
                    custom_size: Some(grid_imprint.world_size()),
                    ..default()
                },
                builder.grid_position,
                grid_imprint,
                NeedsPower,
                related![Indicators[
                    IndicatorType::NoPower,
                    IndicatorType::DisabledByPlayer,
                ]],
                related![EffectInstances[
                    (ModifierContributions(building_info.baseline.clone()), BaselineEffect),
                ]],
                children![
                    IndicatorDisplay::default(),
                ],
            ))
            .observe(on_technical_state_changed_recompute_operational);
        commands.trigger(TechnicalStateChanged { entity, kind: TechnicalChange::JustSpawned });
    }
}

fn on_forge_place_request_do_so(
    trigger: On<PlaceRequest>,
    mut commands: Commands,
    mut placement: BuildingPlacementManager,
) {
    let PlaceRequest(MapObject::Building(BuildingType::Forge)) = *trigger else { return };
    let Some(coords) = placement.claim(BuildingType::Forge) else { return };
    commands.spawn(BuilderForge::new(coords));
}

#[log_tags(Tag::GameSave)]
fn collect_forges(
    forges: Query<(Entity, &GridCoords, &IntegrityPoints, Has<DisabledByPlayer>), With<Forge>>,
    mut save: SaveWriter,
) {
    if forges.is_empty() { return; }

    #[debug_dev("Saving {} forges", rows.len())]
    let rows: Vec<(u32, GridCoords, f32, bool)> = forges
        .iter()
        .map(|(entity, coords, integrity_points, disabled_by_player)| {
            (entity.index_u32(), *coords, integrity_points.get_current(), disabled_by_player)
        })
        .collect();
    save.submit(move |ctx| {
        for (id, coords, integrity_points, disabled_by_player) in rows {
            ctx.save_marker("forges", id)?;
            ctx.save_grid_coords(id, coords)?;
            ctx.save_integrity_points(id, integrity_points)?;
            if disabled_by_player {
                ctx.save_disabled_by_player(id)?;
            }
        }
        Ok(())
    });
}

fn load_forges(ctx: &mut LoadContext) -> LoadResult {
    ctx.for_each_entity("SELECT id FROM forges", |ctx, old_id, entity, _row| {
        let builder = BuilderForge::new(ctx.grid_coords(old_id)?)
            .with_integrity_points(ctx.integrity_points(old_id)?)
            .with_disabled_by_player(ctx.disabled_by_player(old_id)?);
        ctx.insert(entity, builder);
        Ok(())
    })
}

// ============================================================================
// INFO PANEL
// ============================================================================

// Layout
const FORGE_SLOT_SIZE: f32 = 64.0;
const FORGE_SLOT_GAP: f32 = 8.0;
const PROGRESS_BAR_WIDTH: f32 = 120.0;
const PROGRESS_BAR_HEIGHT: f32 = 12.0;
const TEXT_FONT_SIZE: f32 = 12.0;
const COUNTDOWN_FONT_SIZE: f32 = 11.0;

// Colors
const BUTTON_BACKGROUND: Color = Color::srgba(0.15, 0.15, 0.25, 0.9);
const BUTTON_HOVER_BACKGROUND: Color = Color::srgba(0.25, 0.3, 0.5, 0.95);
const PROGRESS_BAR_BACKGROUND: Color = Color::srgba(0.1, 0.1, 0.1, 0.8);
const PROGRESS_BAR_BORDER: Color = Color::srgba(0.4, 0.4, 0.3, 1.);
const PROGRESS_FILL_COLOR: Color = Color::srgba(0.8, 0.6, 0.1, 1.0); // amber

/// Requests a rebuild of the current-job display.
#[derive(Event)]
struct RebuildForgeUi;

/// Root node of the Forge subpanel inside the building info panel.
#[derive(Component)]
pub(crate) struct ForgeInfoPanel;
impl ForgeInfoPanel {
    pub fn subpanel_content_bundle() -> impl Bundle {
        (
            Node {
                display: Display::None,
                width: Val::Percent(100.),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Start,
                align_items: AlignItems::Center,
                ..default()
            },
            ForgeInfoPanel,
            children![
                (
                    Node {
                        flex_direction: FlexDirection::Row,
                        justify_content: JustifyContent::Start,
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(FORGE_SLOT_GAP),
                        margin: UiRect::vertical(Val::Px(4.)),
                        padding: UiRect::all(Val::Px(4.)),
                        border: UiRect::all(Val::Px(2.)),
                        border_radius: BorderRadius::all(Val::Px(4.)),
                        ..default()
                    },
                    BorderColor::all(Color::NONE),
                    ForgeContentContainer,
                ),
                (
                    GlobalQueueToggleButton,
                    Node {
                        padding: UiRect::axes(Val::Px(8.), Val::Px(4.)),
                        margin: UiRect::vertical(Val::Px(4.)),
                        border_radius: BorderRadius::all(Val::Px(3.)),
                        ..default()
                    },
                    BackgroundColor::from(BUTTON_BACKGROUND),
                    children![(
                        Text::new(""),
                        TextFont::from_font_size(TEXT_FONT_SIZE),
                        GlobalQueueToggleLabel,
                    )],
                ),
                (
                    OpenForgingPanelButton,
                    Node {
                        padding: UiRect::axes(Val::Px(8.), Val::Px(4.)),
                        margin: UiRect::vertical(Val::Px(4.)),
                        border_radius: BorderRadius::all(Val::Px(3.)),
                        ..default()
                    },
                    BackgroundColor::from(BUTTON_BACKGROUND),
                    children![(
                        Text::new("Open Forging"),
                        TextFont::from_font_size(TEXT_FONT_SIZE),
                    )],
                ),
            ],
        )
    }

    /// Shows the focused Forge's global-queue setting.
    fn update_global_queue_toggle_label(
        focused_forge: Single<&WorksGlobalQueue, (With<Forge>, With<FocusedMapObject>)>,
        label: Single<&mut Text, With<GlobalQueueToggleLabel>>,
    ) {
        set_text_if_changed(&mut label.into_inner(), focused_forge.label());
    }

    fn on_building_info_panel_enabled_toggle_subpanel_visibility(
        trigger: On<BuildingInfoPanelEnabledTrigger>,
        mut commands: Commands,
        forges: Query<(), With<Forge>>,
        panel: Single<&mut Node, With<ForgeInfoPanel>>,
    ) {
        let focused_entity = trigger.entity;
        if forges.contains(focused_entity) {
            panel.into_inner().display = Display::Flex;
            commands.trigger(RebuildForgeUi);
        } else {
            panel.into_inner().display = Display::None;
        }
    }

    fn on_rebuild_forge_ui_do_so(
        _trigger: On<RebuildForgeUi>,
        mut commands: Commands,
        almanach: Res<Almanach>,
        focused_forge: Single<(Entity, Option<&ForgeCurrentOrder>), (With<Forge>, With<FocusedMapObject>)>,
        container: Single<Entity, With<ForgeContentContainer>>,
        orders: Query<(Entity, &ShardOrder, &ForgingProgress)>,
        parents: Query<&IngredientOf>,
        queue_links: Query<&InForgingQueue>,
    ) {
        let container_entity = *container;
        commands.entity(container_entity).despawn_children().insert(BorderColor::all(Color::NONE));

        let (forge, current_order) = focused_forge.into_inner();
        let job = current_order.and_then(|current_order| orders.get(current_order.order()).ok());
        let Some((order, &ShardOrder(shard), progress)) = job else {
            commands.entity(container_entity).with_child((
                Text::new("Idle"),
                TextFont::from_font_size(TEXT_FONT_SIZE),
                TextLayout::no_wrap(),
            ));
            return;
        };

        // Ingredient jobs use their root order's queue color.
        if let Ok(queue) = queue_links.get(parents.root_ancestor(order)) {
            commands.entity(container_entity).insert(BorderColor::all(queue.source_color(forge)));
        }

        let info = almanach.get_resource_info(shard);
        commands.entity(container_entity).with_children(|parent| {
            // Shard icon
            parent.spawn((
                Node {
                    width: Val::Px(FORGE_SLOT_SIZE),
                    height: Val::Px(FORGE_SLOT_SIZE),
                    border_radius: BorderRadius::all(Val::Px(4.)),
                    ..default()
                },
                ImageNode::new(info.icon.clone()),
            ));

            // Right column: name, progress bar, countdown
            parent.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Start,
                    row_gap: Val::Px(4.),
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                children![
                    (
                        Text::new(info.name.clone()),
                        TextFont::from_font_size(TEXT_FONT_SIZE),
                        TextLayout::no_wrap(),
                    ),
                    (
                        Node {
                            width: Val::Px(PROGRESS_BAR_WIDTH),
                            height: Val::Px(PROGRESS_BAR_HEIGHT),
                            ..default()
                        },
                        children![(
                            BuilderFillBar::default()
                                .with_background_color(PROGRESS_BAR_BACKGROUND)
                                .with_border(PROGRESS_BAR_BORDER, UiRect::all(Val::Px(1.)))
                                .with_border_radius(BorderRadius::all(Val::Px(2.)))
                                .with_fill_color(PROGRESS_FILL_COLOR)
                                .with_fill_fraction(progress.fraction()),
                            ForgeProgressFill,
                        )],
                    ),
                    (
                        Text::new(format!("{:.1}s", progress.remaining_secs())),
                        TextFont::from_font_size(COUNTDOWN_FONT_SIZE),
                        TextLayout::no_wrap(),
                        ForgeCountdownText,
                    ),
                ],
            ));
        });
    }

    /// Updates the focused Forge's progress bar and remaining time.
    fn update_progress(
        focused_forge: Single<&ForgeCurrentOrder, With<FocusedMapObject>>,
        orders: Query<&ForgingProgress>,
        mut progress_fill: Single<&mut FillBar, With<ForgeProgressFill>>,
        mut countdown_text: Single<&mut Text, With<ForgeCountdownText>>,
    ) {
        let Ok(progress) = orders.get(focused_forge.order()) else { return; };
        progress_fill.fill_fraction = progress.fraction();
        set_text_if_changed(&mut countdown_text, &format!("{:.1}s", progress.remaining_secs()));
    }
}

/// Marks the container node that holds the current job or "Idle".
#[derive(Component)]
struct ForgeContentContainer;

/// Marks the `FillBar` in the job progress bar.
#[derive(Component)]
struct ForgeProgressFill;

#[derive(Component)]
struct ForgeCountdownText;

/// Turns the focused Forge's global-queue setting on or off.
#[derive(Component)]
#[require(Button)]
struct GlobalQueueToggleButton;
impl GlobalQueueToggleButton {
    fn on_add_watch_click(trigger: On<Add, GlobalQueueToggleButton>, mut commands: Commands) {
        commands.entity(trigger.entity)
            .observe(recolor_background_on::<Pointer<Over>>(BUTTON_HOVER_BACKGROUND))
            .observe(recolor_background_on::<Pointer<Out>>(BUTTON_BACKGROUND))
            .observe(Self::on_click_toggle_global_queue);
    }

    #[log_tags(Tag::Forge)]
    fn on_click_toggle_global_queue(
        _trigger: On<Pointer<Click>>,
        focused_forge: Single<(&GridCoords, &mut WorksGlobalQueue), (With<Forge>, With<FocusedMapObject>)>,
    ) {
        let (coords, mut works_global) = focused_forge.into_inner();
        works_global.toggle();
        info_player!("Forge at {coords}: {}", works_global.label());
    }
}

#[derive(Component)]
struct GlobalQueueToggleLabel;

/// Opens the Forging panel with the focused Forge selected.
#[derive(Component)]
#[require(Button)]
struct OpenForgingPanelButton;
impl OpenForgingPanelButton {
    fn on_add_watch_click(trigger: On<Add, OpenForgingPanelButton>, mut commands: Commands) {
        commands.entity(trigger.entity)
            .observe(recolor_background_on::<Pointer<Over>>(BUTTON_HOVER_BACKGROUND))
            .observe(recolor_background_on::<Pointer<Out>>(BUTTON_BACKGROUND))
            .observe(Self::on_click_open_forging_panel);
    }

    fn on_click_open_forging_panel(
        _trigger: On<Pointer<Click>>,
        mut commands: Commands,
        mut next_ui_state: ResMut<NextState<UiInteraction>>,
        focused_forge: Single<Entity, (With<Forge>, With<FocusedMapObject>)>,
    ) {
        commands.trigger(ForgingPanelSelectRequest { forge: Some(*focused_forge) });
        next_ui_state.set(UiInteraction::ForgingPanel);
    }
}

/// Rebuilds the panel when the event targets the focused Forge.
fn rebuild_forge_ui_on<E: EntityEvent, B: Bundle>(
    trigger: On<E, B>,
    mut commands: Commands,
    focused: Single<Entity, With<FocusedMapObject>>,
) {
    if trigger.event_target() == *focused {
        commands.trigger(RebuildForgeUi);
    }
}
