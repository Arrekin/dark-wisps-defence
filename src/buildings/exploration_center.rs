use bevy::color::palettes::css::BLUE;

use crate::prelude::*;
use crate::ui::indicators::{IndicatorDisplay, IndicatorType, Indicators};
use crate::ui::display_info_panel::DisplayInfoPanel;
use crate::buildings::info_panel::{BuildingInfoPanelEnabledTrigger};
use crate::units::expedition_drone::{BuilderExpeditionDrone, ExpeditionDrone, DroneFuel, DRONE_COST_ORE};

pub struct ExplorationCenterPlugin;
impl Plugin for ExplorationCenterPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Update, ExplorationCenterInfoPanel::update.run_if(in_state(UiInteraction::DisplayInfoPanel)))
            .add_observer(BuilderExplorationCenter::on_add)
            .add_observer(ExplorationCenterInfoPanel::on_building_info_panel_enabled)
            .add_observer(ExplorationCenterBuyDroneButton::on_add)
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
        // Even if we don't strictly need pagination for ExplorationCenter (few items), respecting it is safer for generic loader logic
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

#[derive(Component)]
pub struct ExplorationCenterInfoPanel;
impl ExplorationCenterInfoPanel {
    fn on_building_info_panel_enabled(
        trigger: On<BuildingInfoPanelEnabledTrigger>,
        exploration_centers: Query<(), With<ExplorationCenter>>,
        exploration_center_panel: Single<&mut Node, With<ExplorationCenterInfoPanel>>,
    ) {
        let focused_entity = trigger.entity;
        exploration_center_panel.into_inner().display = if exploration_centers.contains(focused_entity) { Display::Flex } else { Display::None };
    }

    // TODO: make it event based
    fn update(
        display_info_panel: Single<&DisplayInfoPanel>,
        exploration_centers: Query<&ExplorationCenter>,
        drones: Query<(&ExpeditionDrone, &HomeBase, &DroneFuel)>,
        drone_count_text: Single<&mut Text, With<ExplorationCenterDroneCountText>>,
        buy_button: Single<&mut Node, With<ExplorationCenterBuyDroneButton>>,
    ) {
        let focused_entity = display_info_panel.into_inner().current_focus;
        let Ok(center) = exploration_centers.get(focused_entity) else { return; };
        
        // Count drones owned by this center
        let owned_drones: Vec<_> = drones.iter()
            .filter(|(_, home_base, _)| home_base.0 == focused_entity)
            .collect();
        let drone_count = owned_drones.len();
        let max_slots = center.max_drone_slots;
        
        // Update drone count text
        drone_count_text.into_inner().0 = format!("Drones: {}/{}", drone_count, max_slots);
        
        // Show/hide buy button
        buy_button.into_inner().display = if drone_count < max_slots { Display::Flex} else { Display::None };
    }

    pub fn subpanel_content_bundle() -> impl Bundle {
        (
            Node {
                display: Display::None,
                width: Val::Percent(100.),
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::Start,
                    align_items: AlignItems::Start,
                    ..default()
            },
            ExplorationCenterInfoPanel,
            children![
                // Drone count
                (
                    Text::new("Drones: 0/2"),
                    TextColor::from(BLUE),
                    TextLayout::new_with_linebreak(LineBreak::NoWrap),
                    Node {
                        margin: UiRect{ left: Val::Px(4.), right: Val::Px(4.), ..default() },
                        ..default()
                    },
                    ExplorationCenterDroneCountText,
                ),
                // Buy drone button
                ExplorationCenterBuyDroneButton::default(),
            ],
        )
    }

}

#[derive(Component)]
struct ExplorationCenterDroneCountText;

#[derive(Component, Default)]
#[require(Button)]
struct ExplorationCenterBuyDroneButton;
impl ExplorationCenterBuyDroneButton {
    fn on_add(
        trigger: On<Add, ExplorationCenterBuyDroneButton>,
        mut commands: Commands,
    ) {
        commands.entity(trigger.entity).insert((
            Node {
                width: Val::Percent(80.),
                height: Val::Px(24.),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                margin: UiRect::top(Val::Px(4.)),
                ..default()
            },
            BackgroundColor::from(Color::linear_rgba(0., 0.1, 0.3, 0.8)),
            BorderColor::from(Color::linear_rgba(0., 0.2, 1., 1.)),
            BorderRadius::all(Val::Px(4.)),
            children![(
                Text::new(format!("Buy Drone ({} ore)", DRONE_COST_ORE)),
                TextColor::from(BLUE),
                TextFont::default().with_font_size(12.0),
            )],
        )).observe(Self::on_click);
    }

    fn on_click(
        _trigger: On<Pointer<Click>>,
        mut commands: Commands,
        mut stock: ResMut<Stock>,
        display_info_panel: Single<&DisplayInfoPanel>,
        exploration_centers: Query<&ExplorationCenter>,
        drones: Query<(&ExpeditionDrone, &HomeBase)>,
    ) {
        let focused_entity = display_info_panel.into_inner().current_focus;
        let Ok(center) = exploration_centers.get(focused_entity) else { return; };
        
        // Check slot availability
        let owned_count = drones.iter().filter(|(_, home_base)| home_base.0 == focused_entity).count();
        if owned_count >= center.max_drone_slots { return; }
        
        // Check cost
        let cost = Cost { resource_type: ResourceType::DarkOre, amount: DRONE_COST_ORE as i32 };
        if !stock.try_pay_cost(cost) { return; }
        
        // Spawn new drone
        commands.spawn(BuilderExpeditionDrone::new(focused_entity));
    }
}