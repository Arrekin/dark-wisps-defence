//! # Forge
//!
//! A building that lets the player craft shards from resources.
//! One shard job runs at a time per Forge; parallelism means building more Forges.
//! Shard metadata and recipes live in the shard catalog (`shards_internal/src/shard_catalog.rs`);
//! the Forge only reads them from the [`Almanach`].

use std::time::Duration;

use bevy::{
    platform::collections::HashMap,
    prelude::*,
};

use alteration::{
    effects::prelude::*,
    modifiers::prelude::*,
};
use almanach::prelude::*;
use buildings::prelude::*;
use game_core::prelude::*;
use grids::placement::{annotate_non_empty, PlacementChannel, PlaceRequest};
use hud::prelude::*;
use logging::prelude::*;
use persistence::{
    prelude::{AppGameLoadSaveExtension, CollectSave, GameDbHelpers, LoadContext, SaveWriter},
    rusqlite,
};
use resources::prelude::*;
use shards::prelude::*;
use states::prelude::*;
use widgets::{
    prelude::{
        BuilderChipStrip, BuilderCostChip, BuilderFillBar, CostChipVisualFullPrice, FillBar,
        TooltipBundle, TooltipOf,
    },
    common::utils::recolor_background_on,
};

use crate::{
    common::*,
    info_panel::BuildingInfoPanelEnabledTrigger,
};

pub struct ForgePlugin;
impl Plugin for ForgePlugin {
    fn build(&self, app: &mut App) {
        let asset_server = app.world().resource::<AssetServer>();
        let almanach_info = BuilderForge::almanach_info(asset_server);
        app
            .add_systems(Update, (
                forge_crafting_system.run_if(in_state(GameState::Running)),
                ForgeInfoPanel::update_progress.run_if(in_state(UiInteraction::DisplayInfoPanel)),
            ))
            .add_observer(BuilderForge::on_builder_add_spawn_forge)
            .add_observer(on_forge_place_request_do_so)
            .add_observer(on_start_forge_do_so)
            .add_observer(on_cancel_forge_do_so)
            .add_observer(ForgeInfoPanel::on_building_info_panel_enabled_toggle_subpanel_visibility)
            .add_observer(ForgeInfoPanel::on_rebuild_forge_ui)
            .add_observer(ForgeShardButton::on_add_construct_forge_shard_button)
            .add_observer(on_insert_forge_job_rebuild_ui)
            .add_observer(on_remove_forge_job_rebuild_ui)
            .add_systems(CollectSave, collect_forges)
            .register_loader(MapLoadingStage::SpawnMapElements, "forges", load_forges)
            .register_building(BuildingType::Forge, almanach_info)
            ;
    }
}


// ============================================================================
// FORGE JOB
// ============================================================================

/// An in-progress forging job on a Forge entity.
///
/// Presence means the Forge is currently crafting; absence means it is idle.
/// Progress pauses while the Forge is unpowered or disabled.
#[derive(Component)]
pub(crate) struct ForgeJob {
    shard_type: ShardType,
    timer: Timer,
}
impl ForgeJob {
    pub fn new(shard_type: ShardType, duration: Duration) -> Self {
        Self {
            shard_type,
            timer: Timer::new(duration, TimerMode::Once),
        }
    }

    /// Reconstructs a job that was already running, with `remaining_secs` left of `duration`.
    /// Elapsed is derived against the current `duration` so the progress fraction stays correct
    /// even if the recipe duration changed since the job was saved.
    pub fn resumed(shard_type: ShardType, duration: Duration, remaining_secs: f32) -> Self {
        let mut timer = Timer::new(duration, TimerMode::Once);
        let elapsed = (duration.as_secs_f32() - remaining_secs).clamp(0.0, duration.as_secs_f32());
        timer.set_elapsed(Duration::from_secs_f32(elapsed));
        Self { shard_type, timer }
    }

    pub fn shard_type(&self) -> ShardType {
        self.shard_type
    }

    /// Progress through the job, from 0.0 (just started) to 1.0 (complete).
    pub fn fraction(&self) -> f32 {
        self.timer.fraction()
    }

    /// Seconds remaining until the job completes.
    pub fn remaining_secs(&self) -> f32 {
        self.timer.remaining_secs()
    }
}


// ============================================================================
// REQUEST EVENTS
// ============================================================================

/// Trigger to start a forging job on a Forge entity.
///
/// Validates the blueprint, affordability, and idle state before committing.
/// No-op if any check fails.
#[derive(Event)]
pub(crate) struct StartForgeRequest {
    pub forge: Entity,
    pub shard_type: ShardType,
}

/// Trigger to cancel the active forging job on a Forge entity.
///
/// No refund is issued — resources are consumed at job start.
#[derive(Event)]
pub(crate) struct CancelForgeRequest {
    pub forge: Entity,
}

fn on_start_forge_do_so(
    trigger: On<StartForgeRequest>,
    mut commands: Commands,
    almanach: Res<Almanach>,
    blueprints: Res<ShardBlueprints>,
    mut stock: ResMut<Stock>,
    idle_forges: Query<(), (With<Forge>, Without<ForgeJob>)>,
) {
    let event = trigger.event();
    let forge = event.forge;
    let shard_type = event.shard_type;

    if idle_forges.get(forge).is_err() { return; }
    if !blueprints.is_unlocked(shard_type) { return; }

    let info = almanach.get_shard_info(shard_type);
    let Some(recipe) = &info.recipe else { return };

    if !stock.try_pay_costs(&recipe.cost) { return; }

    commands.entity(forge).insert(ForgeJob::new(shard_type, recipe.duration));
    Log::info().player().tag(Tag::Forge).message(format!("Forge {forge} started forging {shard_type}"));
}

fn on_cancel_forge_do_so(
    trigger: On<CancelForgeRequest>,
    mut commands: Commands,
    forge_jobs: Query<(), With<ForgeJob>>,
) {
    let forge = trigger.event().forge;
    if forge_jobs.get(forge).is_ok() {
        commands.entity(forge).remove::<ForgeJob>();
        Log::info().player().tag(Tag::Forge).message(format!("Forge {forge} cancelled forging"));
    }
}


// ============================================================================
// CRAFT TICK SYSTEM
// ============================================================================

fn forge_crafting_system(
    mut commands: Commands,
    mut shard_inventory: ResMut<ShardInventory>,
    time: Res<Time>,
    mut forges: Query<(Entity, &mut ForgeJob), (With<Forge>, With<IsOperational>)>,
) {
    for (entity, mut job) in forges.iter_mut() {
        job.timer.tick(time.delta());
        if job.timer.just_finished() {
            let shard_type = job.shard_type();
            shard_inventory.add(shard_type, 1);
            commands.entity(entity).remove::<ForgeJob>();
            Log::info().player().tag(Tag::Forge).message(format!("Forge {entity} finished forging {shard_type}"));
        }
    }
}


#[derive(Component, SSS)]
pub(crate) struct BuilderForge {
    pub grid_position: GridCoords,
    /// Saved integrity points. `None` ⇒ defer to baseline (fresh spawn);
    /// `Some` ⇒ override with saved value (restore).
    pub integrity_points: Option<f32>,
    /// Whether the player disabled this building. False on fresh spawn.
    pub disabled_by_player: bool,
    /// In-progress craft to restore, or `None` when idle.
    pub forging: Option<(ShardType, f32)>,
}
impl BuilderForge {
    pub fn almanach_info(asset_server: &AssetServer) -> BuildingInfo {
        BuildingInfo {
            name: "Forge".to_string(),
            sprite: asset_server.load("buildings/forge.png"),
            top_sprite: None,
            grid_imprint: GridImprint::Rectangle { width: 3, height: 3 },
            cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 100 }],
            baseline: HashMap::from([(ModifierType::MaxIntegrityPoints, 100.)]),
            validate: building_validator,
            annotate: annotate_non_empty,
            placement: PlacementChannel::of::<Forge>(),
        }
    }

    pub fn new(grid_position: GridCoords) -> Self {
        Self { grid_position, integrity_points: None, disabled_by_player: false, forging: None }
    }
    pub fn with_integrity_points(mut self, v: f32) -> Self { self.integrity_points = Some(v); self }
    pub fn with_disabled_by_player(mut self) -> Self { self.disabled_by_player = true; self }
    pub fn with_forging(mut self, shard_type: ShardType, remaining_secs: f32) -> Self {
        self.forging = Some((shard_type, remaining_secs));
        self
    }

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

        let mut entity_commands = commands.entity(entity);
        if let Some(ip) = builder.integrity_points {
            entity_commands.insert(IntegrityPoints::new(ip));
        }
        if builder.disabled_by_player {
            entity_commands.insert(DisabledByPlayer);
        }
        if let Some((shard_type, remaining_secs)) = builder.forging
            && let Some(recipe) = &almanach.get_shard_info(shard_type).recipe {
                entity_commands.insert(ForgeJob::resumed(shard_type, recipe.duration, remaining_secs));
                Log::debug().dev().tag(Tag::Forge).message(format!("Forge {entity} resumed forging {shard_type} ({remaining_secs:.1}s left)"));
            }

        entity_commands
            .remove::<BuilderForge>()
            .insert((
                Forge,
                Sprite {
                    image: building_info.sprite.clone(),
                    custom_size: Some(grid_imprint.world_size()),
                    ..Default::default()
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
    _trigger: On<PlaceRequest<Forge>>,
    mut commands: Commands,
    mut placement: BuildingPlacementManager,
) {
    let Some(coords) = placement.claim(BuildingType::Forge) else { return };
    commands.spawn(BuilderForge::new(coords));
}

fn collect_forges(
    forges: Query<(Entity, &GridCoords, &IntegrityPoints, Has<DisabledByPlayer>, Option<&ForgeJob>), With<Forge>>,
    mut save: SaveWriter,
) {
    if forges.is_empty() { return; }
    let rows: Vec<(i64, i32, i32, f32, bool, Option<(ShardType, f32)>)> = forges
        .iter()
        .map(|(entity, coords, integrity_points, disabled_by_player, forge_job)| {
            (
                entity.index_u32() as i64,
                coords.x,
                coords.y,
                integrity_points.get_current(),
                disabled_by_player,
                forge_job.map(|job| (job.shard_type(), job.remaining_secs())),
            )
        })
        .collect();
    Log::debug().dev().tag(Tag::GameSave).message(format!("Saving {} forges", rows.len()));
    save.submit(move |tx| {
        for (id, gx, gy, integrity_points, disabled_by_player, forging) in rows {
            tx.save_marker("forges", id)?;
            tx.save_grid_coords(id, GridCoords { x: gx, y: gy })?;
            tx.save_integrity_points(id, integrity_points)?;
            if disabled_by_player {
                tx.save_disabled_by_player(id)?;
            }
            if let Some((shard_type, remaining_secs)) = forging {
                tx.execute(
                    "UPDATE forges SET forging_shard_type = ?1, forging_remaining_secs = ?2 WHERE id = ?3",
                    rusqlite::params![shard_type.to_string(), remaining_secs, id],
                )?;
            }
        }
        Ok(())
    });
}

fn load_forges(ctx: &mut LoadContext) -> rusqlite::Result<()> {
    let mut stmt = ctx.conn.prepare("SELECT id, forging_shard_type, forging_remaining_secs FROM forges")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let old_id: i64 = row.get(0)?;
        let forging_shard_type: Option<String> = row.get(1)?;
        let forging_remaining_secs: Option<f32> = row.get(2)?;
        let grid_position = ctx.conn.get_grid_coords(old_id)?;
        let integrity_points = ctx.conn.get_integrity_points(old_id)?;
        let disabled_by_player = ctx.conn.get_disabled_by_player(old_id)?;

        let Some(entity) = ctx.entity(old_id) else {
            Log::warn().dev().tag(Tag::GameLoad).message(format!("Forge with old ID {old_id} has no corresponding new entity"));
            continue;
        };
        let mut builder = BuilderForge::new(grid_position)
            .with_integrity_points(integrity_points);
        if disabled_by_player {
            builder = builder.with_disabled_by_player();
        }
        if let (Some(shard_str), Some(remaining_secs)) = (forging_shard_type, forging_remaining_secs)
            && let Ok(shard_type) = shard_str.parse::<ShardType>() {
                builder = builder.with_forging(shard_type, remaining_secs);
            }
        ctx.insert(entity, builder);
    }
    Ok(())
}


// ============================================================================
// INFO PANEL
// ============================================================================

const FORGE_SLOT_SIZE: f32 = 64.0;
const FORGE_SLOT_GAP: f32 = 8.0;

/// Trigger to rebuild the forge info panel content based on current job state.
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
                    Text::new("Forge Shards"),
                    TextLayout::no_wrap(),
                    Node {
                        margin: UiRect::top(Val::Px(4.)),
                        ..default()
                    },
                    TextFont::from_font_size(14.),
                ),
                (
                    Node {
                        flex_direction: FlexDirection::Row,
                        justify_content: JustifyContent::Start,
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(FORGE_SLOT_GAP),
                        margin: UiRect::vertical(Val::Px(4.)),
                        ..default()
                    },
                    ForgeContentContainer,
                ),
            ],
        )
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

    fn on_rebuild_forge_ui(
        _trigger: On<RebuildForgeUi>,
        mut commands: Commands,
        almanach: Res<Almanach>,
        blueprints: Res<ShardBlueprints>,
        stock: Res<Stock>,
        focused_forge: Single<(Entity, Option<&ForgeJob>), (With<Forge>, With<FocusedMapObject>)>,
        container: Single<Entity, With<ForgeContentContainer>>,
    ) {
        let (forge_entity, maybe_job) = focused_forge.into_inner();
        let container_entity = *container;

        commands.entity(container_entity).despawn_related::<Children>();

        if let Some(job) = maybe_job {
            // Forging view
            let info = almanach.get_shard_info(job.shard_type());
            let icon = info.icon.clone();
            let name = info.name.clone();
            let fraction = job.fraction();
            let remaining = job.remaining_secs();

            commands.entity(container_entity).with_children(|parent| {
                // Shard icon
                parent.spawn((
                    Node {
                        width: Val::Px(FORGE_SLOT_SIZE),
                        height: Val::Px(FORGE_SLOT_SIZE),
                        border_radius: BorderRadius::all(Val::Px(4.)),
                        ..default()
                    },
                    ImageNode::new(icon),
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
                        // Shard name text
                        (
                            Text::new(name),
                            TextFont::from_font_size(12.),
                            TextLayout::no_wrap(),
                        ),
                        // Progress bar
                        (
                            Node {
                                width: Val::Px(120.),
                                height: Val::Px(12.),
                                ..default()
                            },
                            children![(
                                BuilderFillBar::default()
                                    .with_background_color(Color::linear_rgba(0.1, 0.1, 0.1, 0.8))
                                    .with_border(Color::linear_rgba(0.4, 0.4, 0.3, 1.), UiRect::all(Val::Px(1.)))
                                    .with_border_radius(BorderRadius::all(Val::Px(2.)))
                                    .with_fill_color(Color::linear_rgba(0.8, 0.6, 0.1, 1.0))
                                    .with_fill_fraction(fraction),
                                ForgeProgressFill,
                            )],
                        ),
                        // Countdown text
                        (
                            Text::new(format!("{:.1}s", remaining)),
                            TextFont::from_font_size(11.),
                            TextLayout::no_wrap(),
                            ForgeCountdownText,
                        ),
                    ],
                ));
            });

            // Cancel button spawned separately so it sits after the column
            let cancel_button = commands.spawn((
                ForgeCancelButton { forge: forge_entity },
                Node {
                    padding: UiRect::axes(Val::Px(8.), Val::Px(4.)),
                    border_radius: BorderRadius::all(Val::Px(3.)),
                    align_self: AlignSelf::Center,
                    ..default()
                },
                BackgroundColor::from(Color::linear_rgba(0.4, 0.15, 0.15, 0.9)),
                children![(
                    Text::new("Cancel"),
                    TextFont::from_font_size(12.),
                )],
            )).observe(ForgeCancelButton::on_click_request_cancel_forge).id();
            commands.entity(container_entity).add_child(cancel_button);
        } else {
            // Idle view: one button per unlocked blueprint that has a recipe
            for shard_type in blueprints.iter() {
                let Some(recipe) = &almanach.get_shard_info(shard_type).recipe else { continue };
                let affordable = stock.can_cover_all(&recipe.cost);
                commands.entity(container_entity).with_child(
                    ForgeShardButton { forge: forge_entity, shard_type, affordable },
                );
            }
        }
    }

    /// Per-frame system: updates the progress bar fill fraction and countdown
    /// text while a forge is actively running.
    fn update_progress(
        focused_job: Single<&ForgeJob, With<FocusedMapObject>>,
        mut progress_fill: Single<&mut FillBar, With<ForgeProgressFill>>,
        mut countdown_text: Single<&mut Text, With<ForgeCountdownText>>,
    ) {
        progress_fill.fill_fraction = focused_job.fraction();
        countdown_text.0 = format!("{:.1}s", focused_job.remaining_secs());
    }
}

/// Marks the container node that holds the idle buttons or the forging view.
#[derive(Component)]
struct ForgeContentContainer;

/// Marks the `FillBar` in the forging-view progress bar.
#[derive(Component)]
struct ForgeProgressFill;

/// Marks the countdown text node in the forging view.
#[derive(Component)]
struct ForgeCountdownText;

/// Cancel button inside the forging view.
#[derive(Component)]
#[require(Button)]
struct ForgeCancelButton {
    forge: Entity,
}
impl ForgeCancelButton {
    fn on_click_request_cancel_forge(
        trigger: On<Pointer<Click>>,
        mut commands: Commands,
        buttons: Query<&ForgeCancelButton>,
    ) {
        let Ok(button) = buttons.get(trigger.entity) else { return };
        commands.trigger(CancelForgeRequest { forge: button.forge });
    }
}


// ============================================================================
// SHARD BUTTON
// ============================================================================

/// Icon button in the idle view representing one forgeable shard type.
///
/// Affordable buttons are clickable with hover highlight; unaffordable buttons
/// are dimmed and non-interactive. Both show a hover tooltip.
#[derive(Component)]
#[require(Button)]
struct ForgeShardButton {
    forge: Entity,
    shard_type: ShardType,
    affordable: bool,
}
impl ForgeShardButton {
    fn on_add_construct_forge_shard_button(
        trigger: On<Add, ForgeShardButton>,
        mut commands: Commands,
        almanach: Res<Almanach>,
        buttons: Query<&ForgeShardButton>,
    ) {
        let entity = trigger.entity;
        let Ok(button) = buttons.get(entity) else { return };
        let shard_type = button.shard_type;
        let affordable = button.affordable;

        let info = almanach.get_shard_info(shard_type);
        let Some(recipe) = info.recipe.as_ref() else {
            Log::error().dev().tag(Tag::Forge).message(format!("ForgeShardButton spawned for {shard_type} with no recipe"));
            return;
        };

        let icon_color = if affordable {
            Color::WHITE
        } else {
            Color::linear_rgba(1., 1., 1., 0.3)
        };

        let bg_color_normal = if affordable {
            Color::linear_rgba(0.15, 0.15, 0.25, 0.9)
        } else {
            Color::linear_rgba(0.1, 0.1, 0.1, 0.7)
        };

        // Button face: the shard icon on a tinted square; dimmed and non-interactive when unaffordable.
        commands.entity(entity).insert((
            Node {
                width: Val::Px(FORGE_SLOT_SIZE),
                height: Val::Px(FORGE_SLOT_SIZE),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                border_radius: BorderRadius::all(Val::Px(4.)),
                ..default()
            },
            BackgroundColor::from(bg_color_normal),
            ImageNode::new(info.icon.clone()).with_color(icon_color),
        ));
        if affordable {
            commands.entity(entity)
                .observe(recolor_background_on::<Pointer<Over>>(Color::linear_rgba(0.25, 0.3, 0.5, 0.95)))
                .observe(recolor_background_on::<Pointer<Out>>(bg_color_normal))
                .observe(Self::on_click_request_start_forge);
        }

        // Hover tooltip. `TooltipBundle` groups the required components;
        // the positioning system in `widgets_internal` places it above the slot.
        commands.spawn(TooltipBundle {
            tooltip_of: TooltipOf(entity),
            node: Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                padding: UiRect::all(Val::Px(6.)),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Start,
                row_gap: Val::Px(4.),
                border_radius: BorderRadius::all(Val::Px(4.)),
                min_width: Val::Px(140.),
                ..default()
            },
            background_color: BackgroundColor::from(Color::linear_rgba(0.1, 0.1, 0.2, 0.95)),
        })
        .with_children(|tooltip| {
            tooltip.spawn((
                Text::new(info.name.clone()),
                TextFont::from_font_size(12.),
                TextColor::from(Color::WHITE),
                TextLayout::no_wrap(),
            ));
            tooltip.spawn((
                Text::new(info.description.clone()),
                TextFont::from_font_size(10.),
                TextColor::from(Color::linear_rgba(0.8, 0.8, 0.8, 1.)),
            ));
            tooltip.spawn((
                Text::new(format!("Forge time: {:.0}s", recipe.duration.as_secs_f32())),
                TextFont::from_font_size(10.),
                TextColor::from(Color::linear_rgba(0.7, 0.8, 0.7, 1.)),
                TextLayout::no_wrap(),
            ));
            tooltip.spawn(BuilderChipStrip)
                .with_children(|cost_row| {
                    for cost in recipe.cost.iter().copied() {
                        cost_row.spawn((
                            BuilderCostChip::from(cost),
                            CostChipVisualFullPrice,
                        ));
                    }
                });
        });
    }

    fn on_click_request_start_forge(
        trigger: On<Pointer<Click>>,
        mut commands: Commands,
        buttons: Query<&ForgeShardButton>,
    ) {
        let Ok(button) = buttons.get(trigger.entity) else { return };
        commands.trigger(StartForgeRequest { forge: button.forge, shard_type: button.shard_type });
    }
}


// ============================================================================
// FORGE JOB INSERT / REMOVE OBSERVERS
// ============================================================================

/// When a ForgeJob is inserted on the focused forge, rebuild the panel.
fn on_insert_forge_job_rebuild_ui(
    trigger: On<Insert, ForgeJob>,
    mut commands: Commands,
    focused: Single<Entity, With<FocusedMapObject>>,
) {
    if trigger.entity == *focused {
        commands.trigger(RebuildForgeUi);
    }
}

/// When a ForgeJob is removed from the focused forge, rebuild the panel.
fn on_remove_forge_job_rebuild_ui(
    trigger: On<Remove, ForgeJob>,
    mut commands: Commands,
    focused: Single<Entity, With<FocusedMapObject>>,
) {
    if trigger.entity == *focused {
        commands.trigger(RebuildForgeUi);
    }
}
