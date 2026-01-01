use bevy::color::palettes::css::BLUE;

use crate::prelude::*;
use crate::ui::indicators::{IndicatorDisplay, IndicatorType, Indicators};
use crate::ui::display_info_panel::DisplayInfoPanel;
use crate::buildings::info_panel::BuildingInfoPanelEnabledTrigger;
use crate::units::expedition_drone::{BuilderExpeditionDrone, DroneState, DRONE_COST_ORE, ExpeditionDroneDeploymentRequest, RecallDrone};
use crate::map_objects::common::ExpeditionTargetMarker;
use lib_ui::utils::recolor_background_on;

pub struct ExplorationCenterPlugin;
impl Plugin for ExplorationCenterPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Update, (
                DroneSlot::update,
                DroneActionButton::update,
            ).run_if(in_state(UiInteraction::DisplayInfoPanel)))
            .add_observer(BuilderExplorationCenter::on_add)
            .add_observer(ExplorationCenterInfoPanel::on_building_info_panel_enabled)
            .add_observer(ExplorationCenterInfoPanel::on_rebuild_drone_slots)
            .add_observer(DroneSlotRow::on_add)
            .add_observer(DroneSlot::on_add)
            .add_observer(DroneActionButton::on_add)
            .add_observer(BuyDroneSlot::on_add)
            .add_observer(TargetSelectionPanel::on_add)
            .add_observer(TargetSelectionPanel::on_open)
            .add_observer(TargetSelectionPanel::on_select_target)
            .add_observer(TargetListItem::on_add)
            .register_db_loader::<BuilderExplorationCenter>(MapLoadingStage::SpawnMapElements)
            .register_db_saver(BuilderExplorationCenter::on_game_save);
    }
}

pub const EXPLORATION_CENTER_BASE_IMAGE: &str = "buildings/exploration_center.png";



#[derive(Clone, Copy, Debug)]
pub struct ExplorationCenterSaveData {
    pub entity: Entity,
    pub health: f32,
    pub disabled_by_player: bool,
}

#[derive(Component, SSS)]
pub struct BuilderExplorationCenter {
    pub grid_position: GridCoords,
    pub save_data: Option<ExplorationCenterSaveData>,
}
impl Saveable for BuilderExplorationCenter {
    fn save(self, tx: &rusqlite::Transaction) -> rusqlite::Result<()> {
        let save_data = self.save_data.expect("BuilderExplorationCenter for saving purpose must have save_data");
        let entity_index = save_data.entity.index() as i64;

        tx.save_marker("exploration_centers", entity_index)?;
        tx.save_grid_coords(entity_index, self.grid_position)?;
        tx.save_health(entity_index, save_data.health)?;
        if save_data.disabled_by_player {
            tx.save_disabled_by_player(entity_index)?;
        }
        Ok(())
    }
}
impl Loadable for BuilderExplorationCenter {
    fn load(ctx: &mut LoadContext) -> rusqlite::Result<LoadResult> {
        let mut stmt = ctx.conn.prepare("SELECT id FROM exploration_centers LIMIT ?1 OFFSET ?2")?;
        let mut rows = stmt.query(ctx.pagination.as_params())?;
        
        let mut count = 0;
        while let Some(row) = rows.next()? {
            let old_id: i64 = row.get(0)?;
            let grid_position = ctx.conn.get_grid_coords(old_id)?;
            let health = ctx.conn.get_health(old_id)?;
            let disabled_by_player = ctx.conn.get_disabled_by_player(old_id)?;
            
            if let Some(new_entity) = ctx.get_new_entity_for_old(old_id) {
                let save_data = ExplorationCenterSaveData { entity: new_entity, health, disabled_by_player };
                ctx.commands.entity(new_entity).insert(BuilderExplorationCenter::new_for_saving(grid_position, save_data));
            } else {
                eprintln!("Warning: ExplorationCenter with old ID {} has no corresponding new entity", old_id);
            }
            count += 1;
        }

        Ok(count.into())
    }
}
impl BuilderExplorationCenter {
    pub fn new(grid_position: GridCoords) -> Self {
        Self { grid_position, save_data: None }
    }
    pub fn new_for_saving(grid_position: GridCoords, save_data: ExplorationCenterSaveData) -> Self {
        Self { grid_position, save_data: Some(save_data) }
    }

    fn on_game_save(
        mut commands: Commands,
        exploration_centers: Query<(Entity, &GridCoords, &Health, Has<DisabledByPlayer>), With<ExplorationCenter>>,
    ) {
        if exploration_centers.is_empty() { return; }
        println!("Creating batch of BuilderExplorationCenter for saving. {} items", exploration_centers.iter().count());
        let batch = exploration_centers.iter().map(|(entity, coords, health, disabled_by_player)| {
            let save_data = ExplorationCenterSaveData {
                entity,
                health: health.get_current(),
                disabled_by_player,
            };
            BuilderExplorationCenter::new_for_saving(*coords, save_data)
        }).collect::<SaveableBatchCommand<_>>();
        commands.queue(batch);
    }

    pub fn on_add(
        trigger: On<Add, BuilderExplorationCenter>,
        mut commands: Commands,
        builders: Query<&BuilderExplorationCenter>,
        asset_server: Res<AssetServer>,
        almanach: Res<Almanach>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return; };
        
        let building_info = almanach.get_building_info(BuildingType::ExplorationCenter);
        let grid_imprint = building_info.grid_imprint;
        
        let mut entity_commands = commands.entity(entity);
        if let Some(save_data) = &builder.save_data {
            // Save data
            entity_commands.insert(Health::new(save_data.health));
            if save_data.disabled_by_player {
                entity_commands.insert(DisabledByPlayer);
            }
        }

        entity_commands
            .remove::<BuilderExplorationCenter>()
            .insert((
                ExplorationCenter::new(2),
                Sprite {
                    image: asset_server.load(EXPLORATION_CENTER_BASE_IMAGE),
                    custom_size: Some(grid_imprint.world_size()),
                    ..Default::default()
                },
                builder.grid_position,
                grid_imprint,
                NeedsPower::default(),
                ModifiersBank::from_baseline(&building_info.baseline),
                related![Indicators[
                    IndicatorType::NoPower,
                    IndicatorType::DisabledByPlayer,
                ]],
                children![
                    IndicatorDisplay::default(),
                ],
            ));
    }
}

////////////////////////////////////////////
////        Display Info Panel          ////
////////////////////////////////////////////

use lib_ui::prelude::Tooltips;

const SLOT_SIZE: f32 = 32.0;
const SLOT_GAP: f32 = 4.0;

/// Trigger to rebuild the drone slots UI
#[derive(Event)]
struct RebuildDroneSlotsUi;

/// Event to open target selection panel for a drone
#[derive(Event)]
struct OpenTargetSelectionPanel {
    drone: Entity,
}

#[derive(Component)]
pub struct ExplorationCenterInfoPanel;
impl ExplorationCenterInfoPanel {
    fn on_building_info_panel_enabled(
        trigger: On<BuildingInfoPanelEnabledTrigger>,
        mut commands: Commands,
        exploration_centers: Query<(), With<ExplorationCenter>>,
        exploration_center_panel: Single<&mut Node, With<ExplorationCenterInfoPanel>>,
        selection_panels: Query<Entity, With<TargetSelectionPanel>>,
    ) {
        // Always close any open selection panels on focus change
        for panel in selection_panels.iter() {
            commands.entity(panel).despawn();
        }
        
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
        display_info_panel: Single<&DisplayInfoPanel>,
        exploration_centers: Query<(&ExplorationCenter, Option<&HomeBaseLinkedObjects>)>,
        drone_count_text: Single<&mut Text, With<ExplorationCenterDroneCountText>>,
        slots_container: Single<Entity, With<DroneSlotsContainer>>,
        existing_slots: Query<Entity, Or<(With<DroneSlotRow>, With<BuyDroneSlot>)>>,
        selection_panels: Query<Entity, With<TargetSelectionPanel>>,
    ) {
        let focused_entity = display_info_panel.into_inner().current_focus;
        let Ok((center, linked_objects)) = exploration_centers.get(focused_entity) else { return; };

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
                    TextLayout::new_with_linebreak(LineBreak::NoWrap),
                    Node {
                        margin: UiRect::all(Val::Px(4.)),
                        ..default()
                    },
                    ExplorationCenterDroneCountText,
                ),
                // Drone slots container (horizontal row)
                (
                    Node {
                        flex_direction: FlexDirection::Row,
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
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
    
    fn on_add(
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

/// A square representing an owned drone
#[derive(Component)]
struct DroneSlot {
    drone_entity: Entity,
}
impl DroneSlot {
    fn new(drone_entity: Entity) -> Self {
        Self { drone_entity }
    }
    
    fn on_add(
        trigger: On<Add, DroneSlot>,
        mut commands: Commands,
        slots: Query<&DroneSlot>,
        drones: Query<&DroneState>,
    ) {
        let entity = trigger.entity;
        let Ok(slot) = slots.get(entity) else { return };
        let Ok(drone_state) = drones.get(slot.drone_entity) else { return };
        
        commands.entity(entity).insert((
            Node {
                width: Val::Px(SLOT_SIZE),
                height: Val::Px(SLOT_SIZE),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor::from(Self::state_color(*drone_state)),
            BorderColor::from(Color::linear_rgba(0.3, 0.6, 0.3, 1.)),
            BorderRadius::all(Val::Px(4.)),
            related![Tooltips[SlotTooltip::new_drone(*drone_state)]],
        ));
    }
    
    fn update(
        mut slots: Query<(&DroneSlot, &mut BackgroundColor, &Tooltips)>,
        drones: Query<&DroneState>,
        mut tooltip_texts: Query<&mut Text, With<SlotTooltip>>,
    ) {
        for (slot, mut bg, tooltips) in slots.iter_mut() {
            let Ok(drone_state) = drones.get(slot.drone_entity) else { continue; };
            *bg = BackgroundColor::from(Self::state_color(*drone_state));
            
            // Update tooltip text
            if let Some(tooltip_entity) = tooltips.iter().next() {
                if let Ok(mut text) = tooltip_texts.get_mut(tooltip_entity) {
                    text.0 = drone_state.to_string();
                }
            }
        }
    }
    
    fn state_color(state: DroneState) -> Color {
        match state {
            DroneState::Stationed => Color::linear_rgba(0.2, 0.4, 0.2, 0.9),
            DroneState::Refueling => Color::linear_rgba(0.3, 0.3, 0.4, 0.9),
            DroneState::Deploying => Color::linear_rgba(0.4, 0.4, 0.1, 0.9),
            DroneState::Scanning => Color::linear_rgba(0.1, 0.4, 0.6, 0.9),
            DroneState::Returning => Color::linear_rgba(0.4, 0.3, 0.1, 0.9),
        }
    }
}

/// Button below drone slot: Send/Recall/Returning
#[derive(Component)]
#[require(Button)]
struct DroneActionButton {
    drone_entity: Entity,
}
impl DroneActionButton {
    fn new(drone_entity: Entity) -> Self {
        Self { drone_entity }
    }
    
    fn on_add(
        trigger: On<Add, DroneActionButton>,
        mut commands: Commands,
        buttons: Query<&DroneActionButton>,
        drones: Query<&DroneState>,
    ) {
        let entity = trigger.entity;
        let Ok(button) = buttons.get(entity) else { return };
        let Ok(drone_state) = drones.get(button.drone_entity) else { return };
        
        let (text, is_active) = Self::button_state(*drone_state);
        
        commands.entity(entity).insert((
            Node {
                padding: UiRect::axes(Val::Px(4.), Val::Px(2.)),
                ..default()
            },
            BackgroundColor::from(if is_active {
                Color::linear_rgba(0.2, 0.3, 0.5, 0.9)
            } else {
                Color::linear_rgba(0.2, 0.2, 0.2, 0.5)
            }),
            BorderRadius::all(Val::Px(3.)),
            children![(
                Text::new(text),
                TextColor::from(if is_active { Color::WHITE } else { Color::linear_rgba(0.6, 0.6, 0.6, 1.) }),
                TextFont::default().with_font_size(10.0),
            )],
        ))
        .observe(Self::on_click)
        .observe(recolor_background_on::<Pointer<Over>>(Color::linear_rgba(0.3, 0.4, 0.6, 0.95)))
        .observe(recolor_background_on::<Pointer<Out>>(Color::linear_rgba(0.2, 0.3, 0.5, 0.9)));
    }
    
    fn update(
        buttons: Query<(&DroneActionButton, &Children)>,
        drones: Query<&DroneState>,
        mut texts: Query<(&mut Text, &mut TextColor)>,
    ) {
        for (button, children) in buttons.iter() {
            let Ok(drone_state) = drones.get(button.drone_entity) else { continue };
            let (text, is_active) = Self::button_state(*drone_state);
            
            // Only update text - background is handled by hover observers
            for child in children.iter() {
                if let Ok((mut t, mut color)) = texts.get_mut(child) {
                    t.0 = text.to_string();
                    *color = TextColor::from(if is_active { Color::WHITE } else { Color::linear_rgba(0.6, 0.6, 0.6, 1.) });
                }
            }
        }
    }
    
    fn button_state(state: DroneState) -> (&'static str, bool) {
        match state {
            DroneState::Stationed => ("Send", true),
            DroneState::Refueling | DroneState::Deploying | DroneState::Scanning => ("Recall", true),
            DroneState::Returning => ("Returning", false),
        }
    }
    
    fn on_click(
        trigger: On<Pointer<Click>>,
        mut commands: Commands,
        buttons: Query<&DroneActionButton>,
        drones: Query<&DroneState>,
    ) {
        let Ok(button) = buttons.get(trigger.entity) else { return };
        let Ok(drone_state) = drones.get(button.drone_entity) else { return };
        
        match drone_state {
            DroneState::Stationed => {
                // Open target selection panel as child of the info panel
                commands.trigger(OpenTargetSelectionPanel { drone: button.drone_entity });
            }
            DroneState::Refueling | DroneState::Deploying | DroneState::Scanning => {
                // Recall drone (or cancel mission if refueling)
                commands.trigger(RecallDrone(button.drone_entity));
            }
            DroneState::Returning => {
                // Already returning, do nothing
            }
        }
    }
}

/// Tooltip shown when hovering over a slot (drone or buy button)
#[derive(Component)]
struct SlotTooltip;
impl SlotTooltip {
    fn new_drone(state: DroneState) -> impl Bundle {
        Self::bundle(state.to_string())
    }
    
    fn new_buy() -> impl Bundle {
        Self::bundle(format!("Cost: {} ore", DRONE_COST_ORE))
    }
    
    fn bundle(text: String) -> impl Bundle {
        (
            Self,
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                bottom: Val::Px(SLOT_SIZE + 4.),
                padding: UiRect::all(Val::Px(4.)),
                ..default()
            },
            BackgroundColor::from(Color::linear_rgba(0.1, 0.1, 0.2, 0.95)),
            BorderRadius::all(Val::Px(4.)),
            children![(
                Text::new(text),
                TextColor::from(Color::WHITE),
                TextFont::default().with_font_size(11.0),
            )],
        )
    }
}

/// A square button for buying a new drone
#[derive(Component, Default)]
#[require(Button)]
struct BuyDroneSlot;
impl BuyDroneSlot {
    fn on_add(
        trigger: On<Add, BuyDroneSlot>,
        mut commands: Commands,
    ) {
        commands.entity(trigger.entity).insert((
            Node {
                width: Val::Px(SLOT_SIZE),
                height: Val::Px(SLOT_SIZE),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor::from(Color::linear_rgba(0.1, 0.2, 0.4, 0.8)),
            BorderColor::from(Color::linear_rgba(0.2, 0.4, 0.8, 1.)),
            BorderRadius::all(Val::Px(4.)),
            children![(
                Text::new("+"),
                TextColor::from(BLUE),
                TextFont::default().with_font_size(20.0),
            )],
            related![Tooltips[SlotTooltip::new_buy()]],
        ))
        .observe(Self::on_click)
        .observe(recolor_background_on::<Pointer<Over>>(Color::linear_rgba(0.15, 0.3, 0.5, 0.9)))
        .observe(recolor_background_on::<Pointer<Out>>(Color::linear_rgba(0.1, 0.2, 0.4, 0.8)));
    }

    fn on_click(
        _trigger: On<Pointer<Click>>,
        mut commands: Commands,
        mut stock: ResMut<Stock>,
        display_info_panel: Single<&DisplayInfoPanel>,
        exploration_centers: Query<(&ExplorationCenter, Option<&HomeBaseLinkedObjects>)>,
    ) {
        let focused_entity = display_info_panel.into_inner().current_focus;
        let Ok((center, linked_objects)) = exploration_centers.get(focused_entity) else { return; };
        
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
struct SelectTargetForDrone {
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
    
    fn on_add(
        trigger: On<Add, TargetSelectionPanel>,
        mut commands: Commands,
        panels: Query<&TargetSelectionPanel>,
        targets: Query<(Entity, &Transform), With<ExpeditionTargetMarker>>,
    ) {
        let entity = trigger.entity;
        let Ok(panel) = panels.get(entity) else { return };
        let drone_entity = panel.drone_entity;
        
        // Build list of target items
        let target_items: Vec<_> = targets.iter()
            .enumerate()
            .map(|(i, (target_entity, _))| TargetListItem::new(drone_entity, target_entity, i))
            .collect();
        
        commands.entity(entity).insert((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(220.),  // Offset from the info panel
                bottom: Val::Px(200.),
                width: Val::Px(200.),
                max_height: Val::Px(300.),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(8.)),
                ..default()
            },
            BackgroundColor::from(Color::linear_rgba(0.1, 0.1, 0.15, 0.95)),
            BorderRadius::all(Val::Px(6.)),
            BorderColor::from(Color::linear_rgba(0.3, 0.3, 0.5, 1.)),
            GlobalZIndex(100),
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
                max_height: Val::Px(200.),
                ..default()
            },
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
                ..default()
            },
            BackgroundColor::from(Color::linear_rgba(0.3, 0.2, 0.2, 0.9)),
            BorderRadius::all(Val::Px(4.)),
            children![(
                Text::new("Cancel"),
                TextColor::from(Color::WHITE),
                TextFont::default().with_font_size(12.0),
            )],
        )).observe(Self::on_cancel_click).id();
        commands.entity(entity).add_child(cancel_btn);
    }
    
    /// Opens the target selection panel as a child of the info panel
    fn on_open(
        trigger: On<OpenTargetSelectionPanel>,
        mut commands: Commands,
        info_panel: Single<Entity, With<ExplorationCenterInfoPanel>>,
        existing_panels: Query<Entity, With<TargetSelectionPanel>>,
    ) {
        // Close any existing panels first
        for panel in existing_panels.iter() {
            commands.entity(panel).despawn();
        }
        
        // Spawn as child of the info panel - will auto-hide when panel hides
        let panel = commands.spawn(TargetSelectionPanel::new(trigger.event().drone)).id();
        commands.entity(*info_panel).add_child(panel);
    }
    
    fn on_cancel_click(
        _trigger: On<Pointer<Click>>,
        mut commands: Commands,
        panels: Query<Entity, With<TargetSelectionPanel>>,
    ) {
        for panel in panels.iter() {
            commands.entity(panel).despawn();
        }
    }
    
    fn on_select_target(
        trigger: On<SelectTargetForDrone>,
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
}

/// A single item in the target selection list
#[derive(Component)]
#[require(Button)]
struct TargetListItem {
    drone_entity: Entity,
    target_entity: Entity,
}
impl TargetListItem {
    fn new(drone_entity: Entity, target_entity: Entity, index: usize) -> impl Bundle {
        (
            Self { drone_entity, target_entity },
            Node {
                padding: UiRect::all(Val::Px(6.)),
                margin: UiRect::bottom(Val::Px(2.)),
                ..default()
            },
            BackgroundColor::from(Color::linear_rgba(0.15, 0.15, 0.2, 0.9)),
            BorderRadius::all(Val::Px(3.)),
            children![(
                Text::new(format!("Target {}", index + 1)),
                TextColor::from(Color::WHITE),
                TextFont::default().with_font_size(12.0),
            )],
        )
    }
    
    fn on_add(
        trigger: On<Add, TargetListItem>,
        mut commands: Commands,
    ) {
        commands.entity(trigger.entity)
            .observe(Self::on_click)
            .observe(recolor_background_on::<Pointer<Over>>(Color::linear_rgba(0.25, 0.3, 0.4, 0.9)))
            .observe(recolor_background_on::<Pointer<Out>>(Color::linear_rgba(0.15, 0.15, 0.2, 0.9)));
    }
    
    fn on_click(
        trigger: On<Pointer<Click>>,
        mut commands: Commands,
        items: Query<&TargetListItem>,
    ) {
        let Ok(item) = items.get(trigger.entity) else { return };
        commands.trigger(SelectTargetForDrone {
            drone: item.drone_entity,
            target: item.target_entity,
        });
    }
}