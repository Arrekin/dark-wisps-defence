//! # Exploration Center
//!
//! Building that houses and manages expedition drones for scanning ExpeditionZone targets.
//!
//! ## Ownership Model
//!
//! ExplorationCenter → (HomeBaseLinkedObjects) → ExpeditionDrone(s) → (HomeBase) → back
//!
//! Drones are spawned as separate entities with a `HomeBase` relationship pointing to their
//! center. The center tracks capacity via `max_drone_slots`. Drones can only refuel and
//! redeploy when their home center is powered and enabled.
//!
//! ## UI Structure
//!
//! When focused in DisplayInfoPanel, shows:
//! - Drone count text
//! - DroneSlotRow per owned drone (icon + fuel bar + action button)
//! - BuyDroneSlot button if capacity available
//! - TargetSelectionPanel modal when sending a drone

use bevy::{
    color::palettes::css::BLUE,
    platform::collections::HashMap,
    prelude::*,
    ui::widget::ViewportNode,
};

use alteration::{
    effects::prelude::*,
    modifiers::prelude::*,
};
use almanach::prelude::*;
use buildings::prelude::*;
use game_core::prelude::*;
use grids::placement::{annotate_non_empty, PlacementChannel};
use hud::prelude::*;
use logging::prelude::*;
use map_objects::prelude::*;
use persistence::{
    prelude::{AppGameLoadSaveExtension, CollectSave, GameDbHelpers, LoadContext, SaveWriter},
    rusqlite,
};
use resources::prelude::*;
use states::prelude::*;
use units::{
    expedition_drone::{
        BuilderExpeditionDrone,
        DRONE_COST_ORE,
        DroneFuel,
        DroneState,
        ExpeditionDrone,
        ExpeditionDroneDeploymentRequest,
        RecallDrone,
    },
    prelude::*,
};
use viewport::{BuilderPreviewCamera, OwnedCameras};
use widgets::{
    prelude::Tooltips,
    utils::recolor_background_on,
};

use crate::{
    common::*,
    info_panel::BuildingInfoPanelEnabledTrigger,
};


pub struct ExplorationCenterPlugin;
impl Plugin for ExplorationCenterPlugin {
    fn build(&self, app: &mut App) {
        let almanach_info = BuilderExplorationCenter::almanach_info(app.world().resource::<AssetServer>());
        app
            .add_systems(Update, (
                DroneSlot::update,
                DroneActionButton::update,
                DroneSlotFuelFill::update,
            ).run_if(in_state(UiInteraction::DisplayInfoPanel)))
            .add_observer(BuilderExplorationCenter::on_builder_add_spawn_exploration_center)
            .add_observer(ExplorationCenterInfoPanel::on_building_info_panel_enabled_toggle_subpanel_visibility)
            .add_observer(ExplorationCenterInfoPanel::on_rebuild_drone_slots)
            .add_observer(DroneSlotRow::on_add_construct_drone_slot_row)
            .add_observer(BuilderDroneSlot::on_builder_add_spawn_drone_slot)
            .add_observer(BuilderDroneActionButton::on_builder_add_spawn_drone_action_button)
            .add_observer(BuilderSlotTooltip::on_builder_add_spawn_slot_tooltip)
            .add_observer(SlotTooltip::on_data_changed_update_tooltip_display)
            .add_observer(BuyDroneSlot::on_add_construct_buy_drone_slot)
            .add_observer(TargetSelectionPanel::on_add_construct_target_selection_panel)
            .add_observer(TargetSelectionPanel::on_open_target_selection_panel_do_so)
            .add_observer(TargetSelectionPanel::on_select_target_deploy_drone_to_target)
            .add_observer(TargetSelectionPanel::on_map_object_focused_close_panel)
            .add_observer(TargetSelectionPanel::on_map_object_unfocused_close_panel)
            .add_observer(TargetListItem::on_add_construct_target_list_item)
            .add_observer(TargetListItemCameraPreview::on_add_construct_target_camera_preview)
            .add_systems(CollectSave, collect_exploration_centers)
            .register_loader(MapLoadingStage::SpawnMapElements, "exploration_centers", load_exploration_centers)
            .register_building(BuildingType::ExplorationCenter, almanach_info)
            ;
    }
}



#[derive(Component, SSS)]
pub(crate) struct BuilderExplorationCenter {
    pub grid_position: GridCoords,
    /// Saved integrity points. `None` ⇒ defer to baseline (fresh spawn);
    /// `Some` ⇒ override with saved value (restore).
    pub integrity_points: Option<f32>,
    /// Whether the player disabled this building. False on fresh spawn.
    pub disabled_by_player: bool,
}
impl BuilderExplorationCenter {
    pub fn almanach_info(asset_server: &AssetServer) -> BuildingInfo {
        BuildingInfo {
            name: "Exploration Center".to_string(),
            sprite: asset_server.load("buildings/exploration_center.png"),
            top_sprite: None,
            grid_imprint: GridImprint::Rectangle { width: 4, height: 4 },
            cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 500 }],
            baseline: HashMap::from([(ModifierType::MaxIntegrityPoints, 100.)]),
            validate: building_validator,
            annotate: annotate_non_empty,
            placement: PlacementChannel::of::<Building>(),
        }
    }

    pub fn new(grid_position: GridCoords) -> Self {
        Self { grid_position, integrity_points: None, disabled_by_player: false }
    }
    pub fn with_integrity_points(mut self, integrity_points: f32) -> Self {
        self.integrity_points = Some(integrity_points);
        self
    }
    pub fn with_disabled_by_player(mut self) -> Self {
        self.disabled_by_player = true;
        self
    }

    pub fn on_builder_add_spawn_exploration_center(
        trigger: On<Add, BuilderExplorationCenter>,
        mut commands: Commands,
        almanach: Res<Almanach>,
        builders: Query<&BuilderExplorationCenter>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return; };
        
        let building_info = almanach.get_building_info(BuildingType::ExplorationCenter);
        let grid_imprint = building_info.grid_imprint;
        
        let mut entity_commands = commands.entity(entity);
        if let Some(ip) = builder.integrity_points {
            entity_commands.insert(IntegrityPoints::new(ip));
        }
        if builder.disabled_by_player {
            entity_commands.insert(DisabledByPlayer);
        }

        entity_commands
            .remove::<BuilderExplorationCenter>()
            .insert((
                ExplorationCenter::new(2),
                Sprite {
                    image: building_info.sprite.clone(),
                    custom_size: Some(grid_imprint.world_size()),
                    ..Default::default()
                },
                builder.grid_position,
                grid_imprint,
                NeedsPower::default(),
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

fn collect_exploration_centers(
    exploration_centers: Query<(Entity, &GridCoords, &IntegrityPoints, Has<DisabledByPlayer>), With<ExplorationCenter>>,
    mut save: SaveWriter,
) {
    if exploration_centers.is_empty() { return; }
    let rows: Vec<(i64, i32, i32, f32, bool)> = exploration_centers
        .iter()
        .map(|(entity, coords, integrity_points, disabled_by_player)| {
            (
                entity.index_u32() as i64,
                coords.x,
                coords.y,
                integrity_points.get_current(),
                disabled_by_player,
            )
        })
        .collect();
    Log::debug().dev().tag(Tag::GameSave).message(format!("Saving {} exploration centers", rows.len()));
    save.submit(move |tx| {
        for (id, gx, gy, integrity_points, disabled_by_player) in rows {
            tx.save_marker("exploration_centers", id)?;
            tx.save_grid_coords(id, GridCoords { x: gx, y: gy })?;
            tx.save_integrity_points(id, integrity_points)?;
            if disabled_by_player {
                tx.save_disabled_by_player(id)?;
            }
        }
        Ok(())
    });
}

fn load_exploration_centers(ctx: &mut LoadContext) -> rusqlite::Result<()> {
    let mut stmt = ctx.conn.prepare("SELECT id FROM exploration_centers")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let old_id: i64 = row.get(0)?;
        let grid_position = ctx.conn.get_grid_coords(old_id)?;
        let integrity_points = ctx.conn.get_integrity_points(old_id)?;
        let disabled_by_player = ctx.conn.get_disabled_by_player(old_id)?;

        let Some(entity) = ctx.entity(old_id) else {
            Log::warn().dev().tag(Tag::GameLoad).message(format!("ExplorationCenter with old ID {old_id} has no corresponding new entity"));
            continue;
        };
        let mut builder = BuilderExplorationCenter::new(grid_position)
            .with_integrity_points(integrity_points);
        if disabled_by_player {
            builder = builder.with_disabled_by_player();
        }
        ctx.insert(entity, builder);
    }
    Ok(())
}

////////////////////////////////////////////
////        Display Info Panel          ////
////////////////////////////////////////////

const SLOT_SIZE: f32 = 64.0;
const SLOT_GAP: f32 = 8.0;

/// Trigger to rebuild the drone slots UI
#[derive(Event)]
struct RebuildDroneSlotsUi;

/// Event to open target selection panel for a drone
#[derive(Event)]
struct OpenTargetSelectionPanel {
    drone: Entity,
}

#[derive(Component)]
pub(crate) struct ExplorationCenterInfoPanel;
impl ExplorationCenterInfoPanel {
    fn on_building_info_panel_enabled_toggle_subpanel_visibility(
        trigger: On<BuildingInfoPanelEnabledTrigger>,
        mut commands: Commands,
        exploration_centers: Query<(), With<ExplorationCenter>>,
        exploration_center_panel: Single<&mut Node, With<ExplorationCenterInfoPanel>>,
    ) {
        let focused_entity = trigger.entity;
        let is_exploration_center = exploration_centers.contains(focused_entity);
        if is_exploration_center {
            exploration_center_panel.into_inner().display = Display::Flex;
            commands.trigger(RebuildDroneSlotsUi);
        } else {
            exploration_center_panel.into_inner().display = Display::None;
        }
    }
    
    fn on_rebuild_drone_slots(
        _trigger: On<RebuildDroneSlotsUi>,
        mut commands: Commands,
        focused_center: Single<(&ExplorationCenter, Option<&HomeBaseLinkedObjects>), With<FocusedMapObject>>,
        drone_count_text: Single<&mut Text, With<ExplorationCenterDroneCountText>>,
        slots_container: Single<Entity, With<DroneSlotsContainer>>,
        existing_slots: Query<Entity, Or<(With<DroneSlotRow>, With<BuyDroneSlot>)>>,
        selection_panels: Query<Entity, With<TargetSelectionPanel>>,
    ) {
        let (center, linked_objects) = focused_center.into_inner();

        // Despawn existing slots and any open selection panels
        for slot_entity in existing_slots.iter() {
            commands.entity(slot_entity).despawn();
        }
        for panel in selection_panels.iter() {
            commands.entity(panel).despawn();
        }
        
        // Get drone entities via HomeBaseLinkedObjects (all linked objects are drones)
        let drone_count = linked_objects.map(|lo| lo.len()).unwrap_or_default();
        let max_slots = center.max_drone_slots;
        
        // Update drone count text
        drone_count_text.into_inner().0 = format!("Drones: {}/{}", drone_count, max_slots);
        
        
        // Spawn drone slot rows (slot + action button) for each owned drone
        for drone_entity in linked_objects.map(|lo| lo.iter()).unwrap_or_default() {
            commands.entity(*slots_container).with_child(DroneSlotRow::new(drone_entity));
        }
        
        // Spawn buy button slot if there are free slots
        if drone_count < max_slots {
            commands.entity(*slots_container).with_child(BuyDroneSlot);
        }
    }

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
            ExplorationCenterInfoPanel,
            children![
                // Drone count text
                (
                    Text::new("Drones: ?/?"),
                    TextColor::from(BLUE),
                    TextLayout::no_wrap(),
                    Node {
                        margin: UiRect::top(Val::Px(4.)),
                        ..default()
                    },
                    TextFont::from_font_size(14.),
                    ExplorationCenterDroneCountText,
                ),
                // Drone slots container (horizontal row)
                (
                    Node {
                        flex_direction: FlexDirection::Row,
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Start,
                        column_gap: Val::Px(SLOT_GAP),
                        margin: UiRect::vertical(Val::Px(4.)),
                        ..default()
                    },
                    DroneSlotsContainer,
                ),
            ],
        )
    }
}

#[derive(Component)]
struct ExplorationCenterDroneCountText;

#[derive(Component, Default)]
struct DroneSlotsContainer;

/// Vertical container for drone slot + action button
#[derive(Component)]
struct DroneSlotRow {
    drone_entity: Entity,
}
impl DroneSlotRow {
    fn new(drone_entity: Entity) -> Self {
        Self { drone_entity }
    }
    
    fn on_add_construct_drone_slot_row(
        trigger: On<Add, DroneSlotRow>,
        mut commands: Commands,
        rows: Query<&DroneSlotRow>,
    ) {
        let entity = trigger.entity;
        let Ok(row) = rows.get(entity) else { return };
        let drone_entity = row.drone_entity;
        
        commands.entity(entity).insert((
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(2.),
                ..default()
            },
            children![
                DroneSlot::new(drone_entity),
                DroneActionButton::new(drone_entity),
            ],
        ));
    }
}

const FUEL_BAR_WIDTH: f32 = 6.0;
const FUEL_BAR_COLOR: Color = Color::linear_rgba(0.9, 0.8, 0.2, 1.0); // yellow

/// A square representing an owned drone
#[derive(Component)]
struct DroneSlot {
    drone_entity: Entity,
}
impl DroneSlot {
    fn new(drone_entity: Entity) -> BuilderDroneSlot {
        BuilderDroneSlot { drone_entity }
    }
    
    fn update(
        mut commands: Commands,
        asset_server: Res<AssetServer>,
        slots: Query<(&DroneSlot, &Tooltips, &Children)>,
        drones: Query<&DroneState>,
        tooltip_data: Query<&SlotTooltipData>,
        mut icons: Query<&mut ImageNode, With<DroneSlotIcon>>,
    ) {
        for (slot, tooltips, children) in slots.iter() {
            let Ok(drone_state) = drones.get(slot.drone_entity) else { continue; };
            
            // Update icon if state changed
            for child in children.iter() {
                if let Ok(mut image) = icons.get_mut(child) {
                    // Update tooltip data and background if changed(TODO: there is weird coupling here where slot image is changes based on tooltip data)
                    let Some(tooltip_entity) = tooltips.iter().next() else { continue };
                    let new_data: SlotTooltipData = SlotTooltipData::DroneState { state: *drone_state, drone_entity: slot.drone_entity };
                    let needs_update = tooltip_data.get(tooltip_entity)
                        .map(|current| *current != new_data)
                        .unwrap_or(true);
                    if needs_update {
                        *image = ImageNode::new(asset_server.load(Self::state_icon(*drone_state)));
                        commands.entity(tooltip_entity).insert(new_data);
                    }
                }
            }
        }
    }

    fn state_icon(state: DroneState) -> &'static str {
        match state {
            DroneState::Stationed => "ui/drones/drone_stationed.png",
            DroneState::Refueling => "ui/drones/drone_refueling.png",
            DroneState::Deploying => "ui/drones/drone_deploying.png",
            DroneState::Scanning => "ui/drones/drone_scanning.png",
            DroneState::Returning => "ui/drones/drone_returning.png",
        }
    }
}

#[derive(Component)]
struct BuilderDroneSlot {
    drone_entity: Entity,
}
impl BuilderDroneSlot {
    fn on_builder_add_spawn_drone_slot(
        trigger: On<Add, BuilderDroneSlot>,
        mut commands: Commands,
        asset_server: Res<AssetServer>,
        builders: Query<&BuilderDroneSlot>,
        drones: Query<(&DroneState, &DroneFuel)>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return };
        let Ok((drone_state, drone_fuel)) = drones.get(builder.drone_entity) else { return };
        
        commands.entity(entity)
            .remove::<BuilderDroneSlot>()
            .insert((
                DroneSlot {
                    drone_entity: builder.drone_entity,
                },
                Node {
                    width: Val::Px(SLOT_SIZE + FUEL_BAR_WIDTH + 4.),
                    height: Val::Px(SLOT_SIZE),
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(2.),
                    ..default()
                },
                children![
                    // Drone icon
                    (
                        Node {
                            width: Val::Px(SLOT_SIZE),
                            height: Val::Px(SLOT_SIZE),
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::Center,
                            border_radius: BorderRadius::all(Val::Px(4.)),
                            ..default()
                        },
                        ImageNode::new(asset_server.load(DroneSlot::state_icon(*drone_state))),
                        BorderColor::from(Color::linear_rgba(0.3, 0.6, 0.3, 1.)),
                        DroneSlotIcon,
                    ),
                    // Fuel bar container (vertical, bottom-aligned fill)
                    (
                        Node {
                            width: Val::Px(FUEL_BAR_WIDTH),
                            height: Val::Percent(100.),
                            flex_direction: FlexDirection::Column,
                            justify_content: JustifyContent::End,
                            border: UiRect::all(Val::Px(1.)),
                            border_radius: BorderRadius::all(Val::Px(2.)),
                            ..default()
                        },
                        BackgroundColor::from(Color::linear_rgba(0.1, 0.1, 0.1, 0.8)),
                        BorderColor::from(Color::linear_rgba(0.4, 0.4, 0.2, 1.)),
                        children![(
                            // Fuel bar fill
                            Node {
                                width: Val::Percent(100.),
                                height: Val::Percent(drone_fuel.fraction() * 100.0),
                                ..default()
                            },
                            BackgroundColor::from(FUEL_BAR_COLOR),
                            DroneSlotFuelFill { drone_entity: builder.drone_entity },
                        )],
                    ),
                ],
                related![Tooltips[BuilderSlotTooltip::new_drone(*drone_state, builder.drone_entity)]],
            ));
    }
}

#[derive(Component)]
struct DroneSlotIcon;

#[derive(Component)]
struct DroneSlotFuelFill {
    drone_entity: Entity
}
impl DroneSlotFuelFill {
    fn update(
        drone_fuels: Query<&DroneFuel>,
        mut fuel_fills: Query<(&mut Node, &DroneSlotFuelFill)>,
    ) {
        for (mut fuel_fill_node, fuel_fill) in fuel_fills.iter_mut() {
            // Update fuel bar fill height
            let Ok(drone_fuel) = drone_fuels.get(fuel_fill.drone_entity) else { continue; };
            fuel_fill_node.height = Val::Percent(drone_fuel.fraction() * 100.0);
        }
    }
}

/// Builder for DroneActionButton
#[derive(Component)]
struct BuilderDroneActionButton {
    drone_entity: Entity,
}
impl BuilderDroneActionButton {
    fn new(drone_entity: Entity) -> Self {
        Self { drone_entity }
    }
    
    fn on_builder_add_spawn_drone_action_button(
        trigger: On<Add, BuilderDroneActionButton>,
        mut commands: Commands,
        builders: Query<&BuilderDroneActionButton>,
        drones: Query<(&DroneState, &ExpeditionDrone)>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return };
        let Ok((drone_state, drone)) = drones.get(builder.drone_entity) else { return };
        
        let (text, is_active) = DroneActionButton::button_state(*drone_state, drone.mission_target.is_some());
        
        let text_entity = commands.spawn((
            Text::new(text),
            TextColor::from(if is_active { Color::WHITE } else { Color::linear_rgba(0.6, 0.6, 0.6, 1.) }),
            TextFont::default().with_font_size(10.0),
        )).id();
        
        commands.entity(entity)
            .remove::<BuilderDroneActionButton>()
            .insert((
                DroneActionButton {
                    drone_entity: builder.drone_entity,
                    text_entity,
                },
                Node {
                    padding: UiRect::axes(Val::Px(4.), Val::Px(2.)),
                    border_radius: BorderRadius::all(Val::Px(3.)),
                    ..default()
                },
                BackgroundColor::from(if is_active {
                    Color::linear_rgba(0.2, 0.3, 0.5, 0.9)
                } else {
                    Color::linear_rgba(0.2, 0.2, 0.2, 0.5)
                }),
            ))
            .add_child(text_entity)
            .observe(DroneActionButton::on_click_handle_drone_action)
            .observe(recolor_background_on::<Pointer<Over>>(Color::linear_rgba(0.3, 0.4, 0.6, 0.95)))
            .observe(recolor_background_on::<Pointer<Out>>(Color::linear_rgba(0.2, 0.3, 0.5, 0.9)));
    }
}

/// Button below drone slot: Send/Recall/Returning
#[derive(Component)]
#[require(Button)]
struct DroneActionButton {
    drone_entity: Entity,
    text_entity: Entity,
}
impl DroneActionButton {
    fn new(drone_entity: Entity) -> BuilderDroneActionButton {
        BuilderDroneActionButton::new(drone_entity)
    }
    
    fn update(
        buttons: Query<&DroneActionButton>,
        drones: Query<(&DroneState, &ExpeditionDrone)>,
        mut texts: Query<(&mut Text, &mut TextColor)>,
    ) {
        for button in buttons.iter() {
            let Ok((drone_state, drone)) = drones.get(button.drone_entity) else { continue };
            let (text, is_active) = Self::button_state(*drone_state, drone.mission_target.is_some());
            
            let Ok((mut t, mut color)) = texts.get_mut(button.text_entity) else { continue };
            t.0 = text.to_string();
            *color = TextColor::from(if is_active { Color::WHITE } else { Color::linear_rgba(0.6, 0.6, 0.6, 1.) });
        }
    }
    
    fn button_state(state: DroneState, has_mission: bool) -> (&'static str, bool) {
        match state {
            DroneState::Stationed => ("Send", true),
            DroneState::Deploying | DroneState::Scanning => ("Recall", true),
            DroneState::Returning | DroneState::Refueling => {
                if has_mission {
                    ("Recall", true)
                } else {
                    ("Returning", false)
                }
            }
        }
    }
    
    fn on_click_handle_drone_action(
        trigger: On<Pointer<Click>>,
        mut commands: Commands,
        buttons: Query<&DroneActionButton>,
        drones: Query<(&DroneState, &ExpeditionDrone)>,
    ) {
        let Ok(button) = buttons.get(trigger.entity) else { return };
        let Ok((drone_state, drone)) = drones.get(button.drone_entity) else { return };
        
        match drone_state {
            DroneState::Stationed => {
                commands.trigger(OpenTargetSelectionPanel { drone: button.drone_entity });
            }
            DroneState::Refueling | DroneState::Deploying | DroneState::Scanning => {
                commands.trigger(RecallDrone(button.drone_entity));
            }
            DroneState::Returning => {
                if drone.mission_target.is_some() {
                    commands.trigger(RecallDrone(button.drone_entity))
                }
            }
        }
    }
}

/// Data component for SlotTooltip - insert this to update tooltip text
#[derive(Component, Clone, PartialEq)]
#[component(immutable)]
enum SlotTooltipData {
    DroneState { state: DroneState, drone_entity: Entity },
    BuyCost(u32),
}
impl SlotTooltipData {
    fn text(&self) -> String {
        match self {
            Self::DroneState { state, .. } => state.to_string(),
            Self::BuyCost(cost) => format!("Cost: {} ore", cost),
        }
    }
    
    /// Returns the drone entity if it is outside home base
    fn drone_outside_home_base(&self) -> Option<Entity> {
        match self {
            Self::DroneState { state, drone_entity } => {
                match state {
                    DroneState::Deploying | DroneState::Scanning | DroneState::Returning => Some(*drone_entity),
                    DroneState::Stationed | DroneState::Refueling => None,
                }
            }
            _ => None,
        }
    }
}

const TOOLTIP_CAMERA_SIZE: f32 = 256.0;

/// Builder for SlotTooltip
#[derive(Component)]
struct BuilderSlotTooltip {
    data: SlotTooltipData,
}
impl BuilderSlotTooltip {
    fn new_drone(state: DroneState, drone_entity: Entity) -> impl Bundle {
        Self::new(SlotTooltipData::DroneState { state, drone_entity })
    }
    fn new_buy() -> impl Bundle {
        Self::new(SlotTooltipData::BuyCost(DRONE_COST_ORE))
    }
    fn new(data: SlotTooltipData) -> impl Bundle {
        (
            Self { data },
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                bottom: Val::Px(SLOT_SIZE + 4.),
                padding: UiRect::all(Val::Px(4.)),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(4.),
                border_radius: BorderRadius::all(Val::Px(4.)),
                ..default()
            },
            BackgroundColor::from(Color::linear_rgba(0.1, 0.1, 0.2, 0.95)),
        )
    }
    
    /// Build the tooltip UI structure. Camera preview is handled by on_data_changed_update_tooltip_display.
    fn on_builder_add_spawn_slot_tooltip(
        trigger: On<Add, BuilderSlotTooltip>,
        mut commands: Commands,
        builders: Query<&BuilderSlotTooltip>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return };
        
        // Text showing drone state
        let text_entity = commands.spawn((
            Text::new(builder.data.text()),
            TextColor::from(Color::WHITE),
            TextFont::default().with_font_size(11.0),
        )).id();
        
        // Initialize tooltip without camera - on_data_changed will add it if needed
        commands.entity(entity)
            .remove::<BuilderSlotTooltip>()
            .add_child(text_entity) // Child first, otherwise below inserts may trigger wrong ordering of preview camera
            .insert((
                SlotTooltip::new(text_entity),
                builder.data.clone(),
            ));
    }
}

/// Tooltip shown when hovering over a slot (drone or buy button)
#[derive(Component)]
struct SlotTooltip {
    text_entity: Entity,
    preview_node_entity: Entity,
}
impl SlotTooltip {
    fn new(text_entity: Entity) -> Self {
        Self { text_entity, preview_node_entity: Entity::PLACEHOLDER }
    }    
    /// Handles camera preview creation/removal when drone state changes.
    /// Cameras are created only when needed.
    fn on_data_changed_update_tooltip_display(
        trigger: On<Insert, SlotTooltipData>,
        mut commands: Commands,
        mut tooltips: Query<(&mut SlotTooltip, &SlotTooltipData, Option<&OwnedCameras>)>,
        mut texts: Query<&mut Text>,
        mut nodes: Query<&mut Node>,
        drones: Query<&Transform>,
    ) {
        let entity = trigger.entity;
        let Ok((mut tooltip, data, maybe_owned_cameras)) = tooltips.get_mut(entity) else { return };
        
        // Update text
        if let Ok(mut text) = texts.get_mut(tooltip.text_entity) {
            text.0 = data.text();
        }
        
        // Check if we need camera preview (drone on mission)
        let needs_camera = data.drone_outside_home_base().is_some();
        let has_camera = maybe_owned_cameras.is_some();
        
        if needs_camera && !has_camera {
            // Add camera preview
            let Some(drone_entity) = data.drone_outside_home_base() else { unreachable!() };
            if let Ok(drone_transform) = drones.get(drone_entity) {
                let camera = commands.spawn(
                    BuilderPreviewCamera::new(
                        entity, // tooltip owns the camera
                        drone_transform.translation.xy(),
                        2.,
                    ).with_auto_follow_entity(drone_entity)
                ).id();
                
                let preview_node = commands.spawn((
                    Node {
                        width: Val::Px(TOOLTIP_CAMERA_SIZE),
                        height: Val::Px(TOOLTIP_CAMERA_SIZE),
                        border: UiRect::all(Val::Px(1.)),
                        ..default()
                    },
                    BorderColor::from(Color::linear_rgba(0.3, 0.5, 0.3, 1.)),
                    ViewportNode::new(camera),
                )).id();
                
                commands.entity(entity).add_child(preview_node);
                tooltip.preview_node_entity = preview_node;
            }
        }

        let Ok(mut preview_node) = nodes.get_mut(tooltip.preview_node_entity) else { return; };
        // Show/hide camera preview
        if needs_camera {
            preview_node.display = Display::Flex;
        } else {
            preview_node.display = Display::None;
        }
    }
    
    // Note: on_remove not needed - CameraOf relationship auto-despawns cameras when tooltip despawns
}

/// A square button for buying a new drone
#[derive(Component, Default)]
#[require(Button)]
struct BuyDroneSlot;
impl BuyDroneSlot {
    fn on_add_construct_buy_drone_slot(
        trigger: On<Add, BuyDroneSlot>,
        mut commands: Commands,
    ) {
        commands.entity(trigger.entity).insert((
            Node {
                width: Val::Px(SLOT_SIZE),
                height: Val::Px(SLOT_SIZE),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                border_radius: BorderRadius::all(Val::Px(4.)),
                ..default()
            },
            BackgroundColor::from(Color::linear_rgba(0.1, 0.2, 0.4, 0.8)),
            BorderColor::from(Color::linear_rgba(0.2, 0.4, 0.8, 1.)),
            children![(
                Text::new("+"),
                TextColor::from(BLUE),
                TextFont::default().with_font_size(20.0),
            )],
            related![Tooltips[BuilderSlotTooltip::new_buy()]],
        ))
        .observe(Self::on_click_buy_drone)
        .observe(recolor_background_on::<Pointer<Over>>(Color::linear_rgba(0.15, 0.3, 0.5, 0.9)))
        .observe(recolor_background_on::<Pointer<Out>>(Color::linear_rgba(0.1, 0.2, 0.4, 0.8)));
    }

    fn on_click_buy_drone(
        _trigger: On<Pointer<Click>>,
        mut commands: Commands,
        mut stock: ResMut<Stock>,
        focused_center: Single<(Entity, &ExplorationCenter, Option<&HomeBaseLinkedObjects>), With<FocusedMapObject>>,
    ) {
        let (focused_entity, center, linked_objects) = focused_center.into_inner();
        
        // Check slot availability
        let owned_count = linked_objects.map(|lo| lo.len()).unwrap_or(0);
        if owned_count >= center.max_drone_slots { return; }
        
        // Check cost
        let cost = Cost { resource_type: ResourceType::DarkOre, amount: DRONE_COST_ORE as i32 };
        if !stock.try_pay_cost(cost) { return; }
        
        // Spawn new drone and trigger UI rebuild
        commands.spawn(BuilderExpeditionDrone::new(focused_entity));
        commands.trigger(RebuildDroneSlotsUi);
    }
}

////////////////////////////////////////////
////     Target Selection Panel         ////
////////////////////////////////////////////

/// Event triggered when a target is selected for a drone
#[derive(Event)]
struct OpenTargetSelectionForDrone {
    drone: Entity,
    target: Entity,
}


/// Modal panel for selecting a target for a drone
#[derive(Component)]
struct TargetSelectionPanel {
    drone_entity: Entity,
}
impl TargetSelectionPanel {
    fn new(drone_entity: Entity) -> Self {
        Self { drone_entity }
    }
    
    fn on_add_construct_target_selection_panel(
        trigger: On<Add, TargetSelectionPanel>,
        mut commands: Commands,
        panels: Query<&TargetSelectionPanel>,
        targets: Query<(Entity, &Name, &GridCoords, &GridImprint), With<ExpeditionZone>>,
        drones: Query<(&HomeBase, &DroneFuel)>,
        home_bases: Query<&Transform, With<ExplorationCenter>>,
    ) {
        let entity = trigger.entity;
        let Ok(panel) = panels.get(entity) else { return };
        let drone_entity = panel.drone_entity;
        
        // Get drone's home base position and fuel for distance calculations
        let Ok((home_base, drone_fuel)) = drones.get(drone_entity) else { return };
        let Ok(home_transform) = home_bases.get(home_base.0) else { return };
        let home_pos = home_transform.translation.xy();
        
        // Build list of target items with fuel cost data, sorted by effectiveness
        let mut target_data: Vec<_> = targets.iter()
            .map(|(target_entity, name, coords, imprint)| {
                let target_pos = coords.to_world_position_centered(*imprint);
                let distance = (target_pos - home_pos).length();
                let fuel_percent = drone_fuel.fuel_percent_for_distance(distance);
                (target_entity, name.as_str().to_string(), *coords, fuel_percent)
            })
            .collect();
        
        // Sort by fuel percentage (most effective = lowest fuel cost first)
        target_data.sort_by(|a, b| a.3.partial_cmp(&b.3).unwrap_or(std::cmp::Ordering::Equal));
        
        let target_items: Vec<_> = target_data.iter()
            .map(|(target_entity, name, coords, fuel_percent)| {
                TargetListItem::new(drone_entity, *target_entity, name, *coords, *fuel_percent)
            })
            .collect();
        
        commands.entity(entity).insert((
            Node {
                position_type: PositionType::Absolute,
                // Center on screen: 50% - half width
                left: Val::Percent(50.),
                top: Val::Percent(50.),
                // Use negative margin to offset by half size for true centering
                margin: UiRect {
                    left: Val::Px(-150.), // half of 300px width
                    top: Val::Px(-200.),  // half of 400px max height
                    ..default()
                },
                width: Val::Px(300.),
                max_height: Val::Percent(50.),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(8.)),
                border_radius: BorderRadius::all(Val::Px(6.)),
                ..default()
            },
            BackgroundColor::from(Color::linear_rgba(0.1, 0.1, 0.15, 0.95)),
            BorderColor::from(Color::linear_rgba(0.3, 0.3, 0.5, 1.)),
        ));
        
        // Header
        commands.entity(entity).with_child((
            Text::new("Select Target"),
            TextColor::from(Color::WHITE),
            TextFont::default().with_font_size(14.0),
            Node { margin: UiRect::bottom(Val::Px(8.)), ..default() },
        ));
        
        // Scrollable list container
        let scroll_container = commands.spawn((
            Node {
                flex_direction: FlexDirection::Column,
                overflow: Overflow::scroll_y(),
                flex_grow: 1.0,
                ..default()
            },
            ScrollPosition::default(),
        )).id();
        commands.entity(entity).add_child(scroll_container);
        
        // Add target items or "no targets" message
        if target_items.is_empty() {
            commands.entity(scroll_container).with_child((
                Text::new("No valid targets"),
                TextColor::from(Color::linear_rgba(0.6, 0.6, 0.6, 1.)),
                TextFont::default().with_font_size(12.0),
            ));
        } else {
            for item in target_items {
                commands.entity(scroll_container).with_child(item);
            }
        }
        
        // Cancel button
        let cancel_btn = commands.spawn((
            Button,
            Node {
                margin: UiRect::top(Val::Px(8.)),
                padding: UiRect::axes(Val::Px(8.), Val::Px(4.)),
                justify_content: JustifyContent::Center,
                border_radius: BorderRadius::all(Val::Px(4.)),
                ..default()
            },
            BackgroundColor::from(Color::linear_rgba(0.3, 0.2, 0.2, 0.9)),
            children![(
                Text::new("Cancel"),
                TextColor::from(Color::WHITE),
                TextFont::default().with_font_size(12.0),
            )],
        )).observe(Self::on_cancel_click_close_target_selection_panel).id();
        commands.entity(entity).add_child(cancel_btn);
    }
    
    /// Opens the target selection panel centered on screen
    fn on_open_target_selection_panel_do_so(
        trigger: On<OpenTargetSelectionPanel>,
        mut commands: Commands,
        existing_panels: Query<Entity, With<TargetSelectionPanel>>,
    ) {
        // Close any existing panels
        for panel in existing_panels.iter() {
            commands.entity(panel).despawn();
        }
        
        // Spawn as separate root UI entity (not child of info panel)
        // Position centered on screen using left/top percentages
        commands.spawn((
            TargetSelectionPanel::new(trigger.event().drone),
            GlobalZIndex(100),
            Pickable::default(),
        ));
    }
    
    fn on_cancel_click_close_target_selection_panel(
        _trigger: On<Pointer<Click>>,
        mut commands: Commands,
        panels: Query<Entity, With<TargetSelectionPanel>>,
    ) {
        for panel in panels.iter() {
            commands.entity(panel).despawn();
        }
    }
    
    fn on_select_target_deploy_drone_to_target(
        trigger: On<OpenTargetSelectionForDrone>,
        mut commands: Commands,
        panels: Query<Entity, With<TargetSelectionPanel>>,
    ) {
        let event = trigger.event();
        
        // Send the drone to target
        commands.trigger(ExpeditionDroneDeploymentRequest {
            drone: event.drone,
            target: event.target,
        });
        
        // Close the panel
        for panel in panels.iter() {
            commands.entity(panel).despawn();
        }
    }

    fn on_map_object_focused_close_panel(
        _trigger: On<Insert, FocusedMapObject>,
        mut commands: Commands,
        selection_panels: Single<Entity, With<TargetSelectionPanel>>,
    ) {
        commands.entity(selection_panels.into_inner()).despawn();
    }

    fn on_map_object_unfocused_close_panel(
        _trigger: On<Remove, FocusedMapObject>,
        mut commands: Commands,
        selection_panels: Single<Entity, With<TargetSelectionPanel>>,
    ) {
        commands.entity(selection_panels.into_inner()).despawn();
    }
}

/// A single item in the target selection list
#[derive(Component)]
#[require(Button)]
struct TargetListItem {
    drone_entity: Entity,
    target_entity: Entity,
}
impl TargetListItem {
    /// Color for fuel percentage: <60% green, <80% yellow, <100% orange, >=100% red
    fn fuel_color(fuel_percent: f32) -> Color {
        if fuel_percent < 60.0 {
            Color::linear_rgba(0.3, 0.9, 0.3, 1.0) // green
        } else if fuel_percent < 80.0 {
            Color::linear_rgba(0.9, 0.9, 0.2, 1.0) // yellow
        } else if fuel_percent < 100.0 {
            Color::linear_rgba(0.9, 0.5, 0.2, 1.0) // orange
        } else {
            Color::linear_rgba(0.9, 0.2, 0.2, 1.0) // red
        }
    }
    
    fn new(drone_entity: Entity, target_entity: Entity, name: &str, coords: GridCoords, fuel_percent: f32) -> impl Bundle {
        let fuel_text = format!("{}%", fuel_percent.round() as i32);
        let fuel_color = Self::fuel_color(fuel_percent);
        
        (
            Self { drone_entity, target_entity },
            Node {
                padding: UiRect::all(Val::Px(6.)),
                margin: UiRect::bottom(Val::Px(4.)),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::SpaceBetween,
                column_gap: Val::Px(8.),
                border_radius: BorderRadius::all(Val::Px(3.)),
                ..default()
            },
            BackgroundColor::from(Color::linear_rgba(0.15, 0.15, 0.2, 0.9)),
            children![
                // Left side: Name, coordinates, and fuel cost
                (
                    Node {
                        flex_direction: FlexDirection::Column,
                        ..default()
                    },
                    children![
                        (
                            Text::new(name.to_string()),
                            TextColor::from(Color::WHITE),
                            TextFont::default().with_font_size(12.0),
                        ),
                        (
                            Text::new(format!("at ({}, {})", coords.x, coords.y)),
                            TextColor::from(Color::linear_rgba(0.7, 0.7, 0.7, 1.)),
                            TextFont::default().with_font_size(10.0),
                        ),
                        // Fuel cost indicator
                        (
                            Node {
                                flex_direction: FlexDirection::Row,
                                column_gap: Val::Px(4.),
                                ..default()
                            },
                            children![
                                (
                                    Text::new("Fuel needed for travel: "),
                                    TextColor::from(Color::linear_rgba(0.6, 0.6, 0.6, 1.)),
                                    TextFont::default().with_font_size(10.0),
                                ),
                                (
                                    Text::new(fuel_text),
                                    TextColor::from(fuel_color),
                                    TextFont::default().with_font_size(10.0),
                                ),
                            ],
                        ),
                    ],
                ),
                // Right side: Camera preview placeholder (will be set up in on_add)
                (
                    Node {
                        width: Val::Px(64.),
                        height: Val::Px(64.),
                        border: UiRect::all(Val::Px(1.)),
                        ..default()
                    },
                    BorderColor::from(Color::linear_rgba(0.4, 0.4, 0.6, 1.)),
                    TargetListItemCameraPreview { target_entity },
                ),
            ],
        )
    }
    
    fn on_add_construct_target_list_item(
        trigger: On<Add, TargetListItem>,
        mut commands: Commands,
    ) {
        commands.entity(trigger.entity)
            .observe(Self::on_click_select_expedition_target)
            .observe(recolor_background_on::<Pointer<Over>>(Color::linear_rgba(0.25, 0.3, 0.4, 0.9)))
            .observe(recolor_background_on::<Pointer<Out>>(Color::linear_rgba(0.15, 0.15, 0.2, 0.9)));
    }
    
    fn on_click_select_expedition_target(
        trigger: On<Pointer<Click>>,
        mut commands: Commands,
        items: Query<&TargetListItem>,
    ) {
        let Ok(item) = items.get(trigger.entity) else { return };
        commands.trigger(OpenTargetSelectionForDrone {
            drone: item.drone_entity,
            target: item.target_entity,
        });
    }
}

/// Camera preview for a target in the selection list
#[derive(Component)]
struct TargetListItemCameraPreview {
    target_entity: Entity,
}
impl TargetListItemCameraPreview {
    fn on_add_construct_target_camera_preview(
        trigger: On<Add, TargetListItemCameraPreview>,
        mut commands: Commands,
        previews: Query<&TargetListItemCameraPreview>,
        targets: Query<(&GridCoords, &GridImprint)>,
    ) {
        let entity = trigger.entity;
        let Ok(preview) = previews.get(entity) else { return };
        let Ok((coords, imprint)) = targets.get(preview.target_entity) else { return };
        
        // Add camera preview centered on the target
        let world_pos = coords.to_world_position_centered(*imprint);
        let camera = commands.spawn(BuilderPreviewCamera::new(
            entity,
            world_pos,
            3., // Zoom level
        )).id();
        
        commands.entity(entity).insert(ViewportNode::new(camera));
    }
}
