use bevy::color::palettes::css::BLUE;

use crate::prelude::*;
use crate::ui::indicators::{IndicatorDisplay, IndicatorType, Indicators};
use crate::ui::display_info_panel::DisplayInfoPanel;
use crate::buildings::info_panel::BuildingInfoPanelEnabledTrigger;
use crate::units::expedition_drone::{BuilderExpeditionDrone, ExpeditionDrone, DroneState, DRONE_COST_ORE};

pub struct ExplorationCenterPlugin;
impl Plugin for ExplorationCenterPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Update, DroneSlot::update.run_if(in_state(UiInteraction::DisplayInfoPanel)))
            .add_observer(BuilderExplorationCenter::on_add)
            .add_observer(ExplorationCenterInfoPanel::on_building_info_panel_enabled)
            .add_observer(ExplorationCenterInfoPanel::on_rebuild_drone_slots)
            .add_observer(DroneSlot::on_add)
            .add_observer(BuyDroneSlot::on_add)
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

#[derive(Component)]
pub struct ExplorationCenterInfoPanel;
impl ExplorationCenterInfoPanel {
    fn on_building_info_panel_enabled(
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
        display_info_panel: Single<&DisplayInfoPanel>,
        exploration_centers: Query<(&ExplorationCenter, Option<&HomeBaseLinkedObjects>)>,
        drone_count_text: Single<&mut Text, With<ExplorationCenterDroneCountText>>,
        slots_container: Single<Entity, With<DroneSlotsContainer>>,
        existing_slots: Query<Entity, Or<(With<DroneSlot>, With<BuyDroneSlot>)>>,
    ) {
        let focused_entity = display_info_panel.into_inner().current_focus;
        let Ok((center, linked_objects)) = exploration_centers.get(focused_entity) else { return; };

        // Despawn existing slots
        for slot_entity in existing_slots.iter() {
            commands.entity(slot_entity).despawn();
        }
        
        // Get drone entities via HomeBaseLinkedObjects (all linked objects are drones)
        let drone_count = linked_objects.map(|lo| lo.len()).unwrap_or_default();
        let max_slots = center.max_drone_slots;
        
        // Update drone count text
        drone_count_text.into_inner().0 = format!("Drones: {}/{}", drone_count, max_slots);
        
        
        // Spawn drone slots for each owned drone
        for drone_entity in linked_objects.map(|lo| lo.iter()).unwrap_or_default() {
            commands.entity(*slots_container).with_child(DroneSlot::new(drone_entity));
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
        drones: Query<&ExpeditionDrone>,
    ) {
        let entity = trigger.entity;
        let Ok(slot) = slots.get(entity) else { return; };
        let Ok(drone) = drones.get(slot.drone_entity) else { return; };
        
        commands.entity(entity).insert((
            Node {
                width: Val::Px(SLOT_SIZE),
                height: Val::Px(SLOT_SIZE),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor::from(Self::state_color(drone.state)),
            BorderColor::from(Color::linear_rgba(0.3, 0.6, 0.3, 1.)),
            BorderRadius::all(Val::Px(4.)),
            related![Tooltips[SlotTooltip::new_drone(drone.state)]],
        ));
    }
    
    fn update(
        mut slots: Query<(&DroneSlot, &mut BackgroundColor, &Tooltips)>,
        drones: Query<&ExpeditionDrone>,
        mut tooltip_texts: Query<&mut Text, With<SlotTooltip>>,
    ) {
        for (slot, mut bg, tooltips) in slots.iter_mut() {
            let Ok(drone) = drones.get(slot.drone_entity) else { continue; };
            *bg = BackgroundColor::from(Self::state_color(drone.state));
            
            // Update tooltip text
            let tooltip_entity = tooltips.iter().next().unwrap();
            if let Ok(mut text) = tooltip_texts.get_mut(tooltip_entity) {
                text.0 = drone.state.to_string();
            }
        }
    }
    
    fn state_color(state: DroneState) -> Color {
        match state {
            DroneState::Stationed => Color::linear_rgba(0.2, 0.4, 0.2, 0.9),
            DroneState::Deploying => Color::linear_rgba(0.4, 0.4, 0.1, 0.9),
            DroneState::Scanning => Color::linear_rgba(0.1, 0.4, 0.6, 0.9),
            DroneState::Returning => Color::linear_rgba(0.4, 0.3, 0.1, 0.9),
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
        .observe(Self::on_hover_start)
        .observe(Self::on_hover_end);
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
    
    fn on_hover_start(
        trigger: On<Pointer<Over>>,
        mut buttons: Query<&mut BackgroundColor, With<BuyDroneSlot>>,
    ) {
        let Ok(mut bg) = buttons.get_mut(trigger.entity) else { return };
        *bg = BackgroundColor::from(Color::linear_rgba(0.15, 0.3, 0.5, 0.9));
    }
    
    fn on_hover_end(
        trigger: On<Pointer<Out>>,
        mut buttons: Query<&mut BackgroundColor, With<BuyDroneSlot>>,
    ) {
        let Ok(mut bg) = buttons.get_mut(trigger.entity) else { return };
        *bg = BackgroundColor::from(Color::linear_rgba(0.1, 0.2, 0.4, 0.8));
    }
}