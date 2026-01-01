use bevy::color::palettes::css::{AQUA, BLUE, INDIGO};

use lib_grid::grids::obstacles::{ObstacleGrid, ReservedCoords};
use lib_ui::prelude::*;

use crate::prelude::*;
use crate::map_objects::common::ExpeditionZone;
use crate::ui::display_info_panel::{DisplayInfoPanel, DisplayPanelMainContentRoot, UiMapObjectFocusedTrigger};
use crate::ui::grid_object_placer::{GridObjectPlacer, GridObjectPlacerRequest};
use crate::units::expedition_drone::{ExpeditionDrone, DroneState, ExpeditionDroneDeploymentRequest};

use super::common::ExpeditionTargetMarker;

pub struct QuantumFieldPlugin;
impl Plugin for QuantumFieldPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Startup, (
                |mut commands: Commands| { commands.spawn(GridPlacerUiForQuantumField::default()); },
            ))
            .add_systems(PostStartup, (
                initialize_quantum_field_panel_content_system,
            ))
            .add_systems(Update, (
                onclick_spawn_system.run_if(in_state(UiInteraction::PlaceGridObject)),
                operate_arrows_for_grid_placer_ui_for_quantum_field_system,
                process_expeditions_system.run_if(in_state(GameState::Running)),
                (
                    update_quantum_field_info_panel_system,
                    update_quantum_field_action_button_system,
                ).run_if(in_state(UiInteraction::DisplayInfoPanel)),
            ))
            .add_observer(BuilderQuantumField::on_add)
            .add_observer(GridPlacerUiForQuantumField::on_add)
            .add_observer(ArrowButton::on_add)
            .add_observer(QuantumFieldActionButton::on_add)
            .add_observer(on_ui_map_object_focus_changed_trigger)
            .register_db_loader::<BuilderQuantumField>(MapLoadingStage::SpawnMapElements)
            .register_db_saver(BuilderQuantumField::on_game_save)
            ;
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct QuantumFieldImprintSelector(i32);
impl QuantumFieldImprintSelector {
    pub const MIN_IMPRINT_SIZE: i32 = 3;
    pub const MAX_IMPRINT_SIZE: i32 = 6;
    pub fn get_size(&self) -> i32 {
        self.0
    }
    pub fn get(&self) -> GridImprint {
        GridImprint::Rectangle { width: self.0, height: self.0 }
    }
    pub fn set(&mut self, new_size: i32) -> Result<(), String> {
        if new_size >= Self::MIN_IMPRINT_SIZE && new_size <= Self::MAX_IMPRINT_SIZE {
            self.0 = new_size;
            Ok(())
        } else {
            Err(format!("Quantum field imprint size must be between {} and {}", Self::MIN_IMPRINT_SIZE, Self::MAX_IMPRINT_SIZE))
        }
    }
    pub fn increase(&mut self) -> Result<(), String> {
        self.set(self.0 + 1)
    }
    pub fn decrease(&mut self) -> Result<(), String> {
        self.set(self.0 - 1)
    }
}
impl Default for QuantumFieldImprintSelector {
    fn default() -> Self {
        Self(Self::MIN_IMPRINT_SIZE)
    }
}

// Marks QuantumField with all its layers solved
#[derive(Component)]
struct Solved;

#[derive(Component)]
#[require(MapBound, ObstacleGridObject = ObstacleGridObject::QuantumField)]
pub struct QuantumField {
    pub layers: Vec<QuantumFieldLayer>,
    pub current_layer: usize,
    pub current_layer_progress: i32,
}
impl QuantumField {
    pub fn progress_layer(&mut self, amount: i32) {
        if self.is_solved() { return; }
        self.current_layer_progress = std::cmp::min(self.current_layer_progress + amount, self.layers[self.current_layer].value);
    }
    pub fn move_to_next_layer(&mut self) {
        if self.is_solved() { return; }
        self.current_layer += 1;
        self.current_layer_progress = 0;
    }
    /// Returns (current_layer_progress, current_layer_target)
    pub fn get_progress_details(&self) -> (i32, i32) {
        if self.is_solved() { return (0, 0); }
        (self.current_layer_progress, self.layers[self.current_layer].value)
    }
    pub fn is_solved(&self) -> bool {
        self.current_layer == self.layers.len()
    }
    pub fn is_current_layer_solved(&self) -> bool {
        self.current_layer_progress >= self.layers[self.current_layer].value
    }
    pub fn get_current_layer_costs(&self) -> &[Cost] {
        if self.is_solved() { return &[]; }
        &self.layers[self.current_layer].costs
    }
}

// Describes a layer of QuantumField to solve
// `value` - amount of research needed to solve the layer
// `costs` - costs needed to pay after solving the layer to finalize it
pub struct QuantumFieldLayer {
    pub value: i32,
    pub costs: Vec<Cost>,
}

#[derive(Clone, Copy, Debug)]
pub struct QuantumFieldSaveData {
    pub entity: Entity,
    pub current_layer: usize,
    pub current_layer_progress: i32,
    pub is_expedition_target: bool,
}

#[derive(Component, SSS)]
pub struct BuilderQuantumField {
    pub grid_position: GridCoords,
    pub grid_imprint: GridImprint,
    pub save_data: Option<QuantumFieldSaveData>,
}

impl Saveable for BuilderQuantumField {
    fn save(self, tx: &rusqlite::Transaction) -> rusqlite::Result<()> {
        let save_data = self.save_data.expect("BuilderQuantumField for saving purpose must have save_data");
        let entity_index = save_data.entity.index() as i64;

        // 1. Insert into quantum_fields table
        tx.register_entity(entity_index)?;
        tx.execute(
            "INSERT OR REPLACE INTO quantum_fields (id, current_layer, current_layer_progress, is_expedition_target) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![entity_index, save_data.current_layer, save_data.current_layer_progress, save_data.is_expedition_target],
        )?;

        // 2. Insert into grid_positions table
        tx.save_grid_coords(entity_index, self.grid_position)?;
        
        // 3. Insert into grid_imprints table
        tx.save_grid_imprint(entity_index, self.grid_imprint)?;
        
        Ok(())
    }
}
impl Loadable for BuilderQuantumField {
    fn load(ctx: &mut LoadContext) -> rusqlite::Result<LoadResult> {
        let mut stmt = ctx.conn.prepare("SELECT id, current_layer, current_layer_progress, is_expedition_target FROM quantum_fields LIMIT ?1 OFFSET ?2")?;
        let mut rows = stmt.query(ctx.pagination.as_params())?;
        
        let mut count = 0;
        while let Some(row) = rows.next()? {
            let old_id: i64 = row.get(0)?;
            let current_layer: usize = row.get(1)?;
            let current_layer_progress: i32 = row.get(2)?;
            let is_expedition_target: bool = row.get(3)?;
            
            let grid_position = ctx.conn.get_grid_coords(old_id)?;
            let grid_imprint = ctx.conn.get_grid_imprint(old_id)?;
            
            if let Some(new_entity) = ctx.get_new_entity_for_old(old_id) {
                let save_data = QuantumFieldSaveData { 
                    entity: new_entity,
                    current_layer,
                    current_layer_progress,
                    is_expedition_target,
                };
                ctx.commands.entity(new_entity).insert(BuilderQuantumField::new_for_saving(grid_position, grid_imprint, save_data));
            } else {
                eprintln!("Warning: QuantumField with old ID {} has no corresponding new entity", old_id);
            }
            count += 1;
        }

        Ok(count.into())
    }
}

impl BuilderQuantumField {
    pub fn new(grid_position: GridCoords, grid_imprint: GridImprint) -> Self {
        Self { grid_position, grid_imprint, save_data: None }
    }
    pub fn new_for_saving(grid_position: GridCoords, grid_imprint: GridImprint, save_data: QuantumFieldSaveData) -> Self {
        Self { grid_position, grid_imprint, save_data: Some(save_data) }
    }
    
    fn on_game_save(
        mut commands: Commands,
        quantum_fields: Query<(Entity, &GridCoords, &GridImprint, &QuantumField, Has<ExpeditionTargetMarker>)>,
    ) {
        if quantum_fields.is_empty() { return; }
        println!("Creating batch of BuilderQuantumField for saving. {} items", quantum_fields.iter().count());
        let batch = quantum_fields.iter().map(|(entity, coords, imprint, quantum_field, is_expedition_target)| {
            let save_data = QuantumFieldSaveData {
                entity,
                current_layer: quantum_field.current_layer,
                current_layer_progress: quantum_field.current_layer_progress,
                is_expedition_target,
            };
            BuilderQuantumField::new_for_saving(*coords, *imprint, save_data)
        }).collect::<SaveableBatchCommand<_>>();
        commands.queue(batch);
    }
    
    fn on_add(
        trigger: On<Add, BuilderQuantumField>,
        mut commands: Commands,
        builders: Query<&BuilderQuantumField>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return; };
        
        let mut quantum_field = QuantumField {
            current_layer: 0,
            current_layer_progress: 0,
            layers: vec![
                QuantumFieldLayer {
                    value: 15000,
                    costs: vec![Cost{ resource_type: ResourceType::DarkOre, amount: 100}, Cost{ resource_type: ResourceType::DarkOre, amount: 100}, Cost{ resource_type: ResourceType::DarkOre, amount: 100}],
                },
                QuantumFieldLayer {
                    value: 30000,
                    costs: vec![Cost{ resource_type: ResourceType::DarkOre, amount: 200}],
                },
                QuantumFieldLayer {
                    value: 45000,
                    costs: vec![Cost{ resource_type: ResourceType::DarkOre, amount: 300}],
                },
            ],
        };
        
        if let Some(save_data) = &builder.save_data {
            quantum_field.current_layer = save_data.current_layer;
            quantum_field.current_layer_progress = save_data.current_layer_progress;
            
            if save_data.is_expedition_target {
                commands.entity(entity).insert(ExpeditionTargetMarker);
            }
            if quantum_field.is_solved() {
                commands.entity(entity).insert(Solved);
            }
        }

        commands.entity(entity)
            .remove::<BuilderQuantumField>()
            .insert((
                Sprite {
                    custom_size: Some(builder.grid_imprint.world_size()),
                    color: INDIGO.into(),
                    ..Default::default()
                },
                Transform::from_translation(builder.grid_position.to_world_position_centered(builder.grid_imprint).extend(Z_OBSTACLE)),
                builder.grid_position,
                builder.grid_imprint,
                quantum_field,
                ExpeditionZone::default(),
                ExpeditionTargetMarker, // All quantum fields are valid drone targets by default
            ));
    }
}

fn onclick_spawn_system(
    mut commands: Commands,
    mut reserved_coords: ResMut<ReservedCoords>,
    obstacles_grid: Res<ObstacleGrid>,
    mouse: Res<ButtonInput<MouseButton>>,
    mouse_info: Res<MouseInfo>,
    grid_object_placer: Single<&GridObjectPlacer>,
) {
    let GridObjectPlacer::QuantumField(imprint_selector) = grid_object_placer.into_inner() else { return; };
    let grid_imprint = imprint_selector.get();
    let mouse_coords = mouse_info.grid_coords;
    if mouse_info.is_over_ui || !mouse_coords.is_in_bounds(obstacles_grid.bounds()) { return; }
    if mouse.pressed(MouseButton::Left) {
        // Place a quantum_field
        if obstacles_grid.query_imprint_all(mouse_coords, grid_imprint, |field| !field.is_within_quantum_field()) && !reserved_coords.any_reserved(mouse_coords, grid_imprint) {
            commands.spawn(BuilderQuantumField::new(mouse_coords, grid_imprint));
            reserved_coords.reserve(mouse_coords, grid_imprint);
        }
    } else if mouse.pressed(MouseButton::Right) {
        // Remove a quantum_field
        if let Some(entity) = obstacles_grid[mouse_coords].quantum_field {
            commands.entity(entity).despawn();
        }
    }
}

fn process_expeditions_system(
    mut commands: Commands,
    mut quantum_fields: Query<(Entity, &mut QuantumField, &mut ExpeditionZone), (Changed<ExpeditionZone>, Without<Solved>)>,
) {
    for (entity, mut quantum_field, mut expedition_zone) in quantum_fields.iter_mut() {
        while expedition_zone.expeditions_arrived > 0 {
            expedition_zone.expeditions_arrived -= 1;
            quantum_field.progress_layer(1500); // TODO: It should come from Almanach
            if quantum_field.is_solved() {
                commands.entity(entity).insert(Solved);
            }
        }
    }
}

/// Widget for selecting QuantumField grid imprint size during construction
/// The widget consists of one horizontal layer containing left arrow button, text label specifying the imprint size and right arrow button
#[derive(Component, Default)]
pub struct GridPlacerUiForQuantumField {
    pub imprint_selector: QuantumFieldImprintSelector,
}
impl GridPlacerUiForQuantumField {
    pub fn imprint_str(&self) -> String {
        let size = self.imprint_selector.get_size();
        format!("Quantum Field {}x{}", size, size)
    }

    fn on_add(
        trigger: On<Add, GridPlacerUiForQuantumField>,
        mut commands: Commands,
        grid_placer_ui_for_quantum_field: Single<&GridPlacerUiForQuantumField>,
    ) {
        let entity = trigger.entity;
        let ui_text = grid_placer_ui_for_quantum_field.into_inner().imprint_str();
        commands.entity(entity).insert((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(5.0),
                left: Val::Percent(50.0),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(2.5),
                ..default()
            },
            children![
                ArrowButton::Decrease,
                Text::new(ui_text),
                ArrowButton::Increase,
            ],
        ));
    }
}
#[derive(Component)]
#[require(Button, Pickable)]
pub enum ArrowButton {
    Decrease,
    Increase,
}
impl ArrowButton {
    fn text(&self) -> &str {
        match self {
            ArrowButton::Decrease => "<",
            ArrowButton::Increase => ">",
        }
    }
    fn on_add(
        trigger: On<Add, ArrowButton>,
        mut commands: Commands,
        arrows: Query<&ArrowButton>,
    ) {
        let entity = trigger.entity;
        let arrow_button = arrows.get(entity).unwrap();
        commands.entity(entity).insert((
            Node {
                width: Val::Px(16.),
                height: Val::Px(16.),
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::BLACK),
            children![(Text::new(arrow_button.text()), TextFont::default().with_font_size(12.))],
        )).observe(Self::on_click);
    }

    fn on_click(
        trigger: On<Pointer<Click>>,
        ui: Single<(&Children, &mut GridPlacerUiForQuantumField)>,
        arrows: Query<&ArrowButton>,
        mut texts: Query<&mut Text>,
        mut placer_request: ResMut<GridObjectPlacerRequest>,
    ) {
        let entity = trigger.entity;
        let (ui_children, mut grid_placer_ui) = ui.into_inner();
    
        let arrow_button = arrows.get(entity).unwrap();
        match arrow_button {
            ArrowButton::Decrease => { let _ = grid_placer_ui.imprint_selector.decrease();},
            ArrowButton::Increase => { let _ = grid_placer_ui.imprint_selector.increase();},
        }
    
        let ui_text = grid_placer_ui.imprint_str();
        texts.get_mut(ui_children[1]).unwrap().0 = ui_text;
        placer_request.set(GridObjectPlacer::QuantumField(grid_placer_ui.imprint_selector));
    }
}

pub fn operate_arrows_for_grid_placer_ui_for_quantum_field_system(
    ui: Single<&mut Visibility, With<GridPlacerUiForQuantumField>>,
    grid_object_placer: Single<&GridObjectPlacer>,
) {
    let mut visibility = ui.into_inner();
    *visibility = if let GridObjectPlacer::QuantumField(_) = grid_object_placer.into_inner() {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
}

////////////////////////////////////////////
////        Display Info Panel          ////
////////////////////////////////////////////
#[derive(Component)]
struct QuantumFieldPanel;
#[derive(Component)]
struct QuantumFieldLayerHealthbar;
#[derive(Component)]
struct QuantumFieldLayerText;
#[derive(Component)]
struct QuantumFieldLayerCostsContainer;
#[derive(Component)]
struct QuantumFieldLayerCostPanel;
#[derive(Component, Default, PartialEq)]
#[require(Button)]
enum QuantumFieldActionButton {
    #[default]
    Hidden,
    SendExpeditions,
    StopExpeditions,
    PayCost,
}
impl QuantumFieldActionButton {
    fn on_add(
        trigger: On<Add, QuantumFieldActionButton>,
        mut commands: Commands,
    ) {
        commands.entity(trigger.entity).insert((
            Node {
                width: Val::Percent(50.),
                height: Val::Px(20.),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor::from(Color::linear_rgba(0., 0., 0.2, 0.2)),
            BorderColor::from(Color::linear_rgba(0., 0.2, 1., 1.)),
            children![(
                Text::new("Send Expeditions / Stop Expeditions / Pay cost"),
                TextColor::from(BLUE),
                TextFont::default().with_font_size(12.0),
                QuantumFieldActionButtonText,
            )],
        )).observe(Self::on_click);
    }
    fn on_click(
        _trigger: On<Pointer<Click>>,
        mut commands: Commands,
        mut stock: ResMut<Stock>,
        display_info_panel: Single<&DisplayInfoPanel>,
        action_button: Single<&mut QuantumFieldActionButton>,
        mut quantum_fields: Query<&mut QuantumField>,
        drones: Query<(Entity, &ExpeditionDrone, &DroneState)>,
    ) {
        let focused_entity = display_info_panel.into_inner().current_focus;
        let mut action_button = action_button.into_inner();
        match *action_button {
            QuantumFieldActionButton::SendExpeditions => {
                // Send all idle drones
                for (drone_entity, drone, drone_state) in drones.iter() {
                    if matches!(drone_state, DroneState::Stationed) && drone.mission_target.is_none() {
                        commands.trigger(ExpeditionDroneDeploymentRequest {
                            drone: drone_entity,
                            target: focused_entity,
                        });
                    }
                }
            },
            QuantumFieldActionButton::StopExpeditions => {
                commands.entity(focused_entity).remove::<ExpeditionTargetMarker>();
            },
            QuantumFieldActionButton::PayCost => {
                let Ok(mut quantum_field) = quantum_fields.get_mut(focused_entity) else { return; };
                if stock.try_pay_costs(quantum_field.get_current_layer_costs()) {
                    quantum_field.move_to_next_layer();
                    commands.trigger(UiMapObjectFocusedTrigger { entity: focused_entity });
                }
            },
            QuantumFieldActionButton::Hidden => {},
        }
        *action_button = QuantumFieldActionButton::Hidden; // To make sure no multi-trigger occurs
    }
}
#[derive(Component)]
struct QuantumFieldActionButtonText;


fn update_quantum_field_info_panel_system(
    quantum_fields: Query<(&QuantumField, Has<ExpeditionTargetMarker>)>,
    display_info_panel: Single<&DisplayInfoPanel>,
    action_button: Single<&mut QuantumFieldActionButton>,
    healthbar: Single<&mut Healthbar, With<QuantumFieldLayerHealthbar>>,
    text: Single<&mut Text, With<QuantumFieldLayerText>>,
) {
    let focused_entity = display_info_panel.into_inner().current_focus;
    let Ok((quantum_field, has_expedition_target_marker)) = quantum_fields.get(focused_entity) else { return; };
    // Update the layer text
    text.into_inner().0 = if quantum_field.is_solved() {
        "All Quantum Layers Solved".to_string()
    } else {
        format!("Quantum Layer {}/{}", quantum_field.current_layer + 1, quantum_field.layers.len())
    };
    // Update the layer progress
    let (current_layer_progress, current_layer_target) = quantum_field.get_progress_details();
    let mut healthbar = healthbar.into_inner();
    healthbar.value = current_layer_progress as f32;
    healthbar.max_value = current_layer_target as f32;
    // Update the action button
    *action_button.into_inner() = {
        if quantum_field.is_solved() {
            QuantumFieldActionButton::Hidden
        } else if quantum_field.is_current_layer_solved() {
            QuantumFieldActionButton::PayCost
        } else if  has_expedition_target_marker {
            QuantumFieldActionButton::StopExpeditions
        } else {
            QuantumFieldActionButton::SendExpeditions
        }
    };
}

fn on_ui_map_object_focus_changed_trigger(
    trigger: On<UiMapObjectFocusedTrigger>,
    mut commands: Commands,
    quantum_fields: Query<&QuantumField>,
    quantum_field_panel: Single<&mut Node, With<QuantumFieldPanel>>,
    costs_container: Single<Entity, With<QuantumFieldLayerCostsContainer>>,
    costs_panels: Query<Entity, With<QuantumFieldLayerCostPanel>>,
) {
    let focused_entity = trigger.entity;
    let Ok(quantum_field) = quantum_fields.get(focused_entity) else { 
        quantum_field_panel.into_inner().display = Display::None;
        return;
     };
    quantum_field_panel.into_inner().display = Display::Flex;

    // Remove the old panels
    costs_panels.iter().for_each(|entity| commands.entity(entity).despawn());

    // Create the new panels
    let mut costs_container_commands = commands.entity(costs_container.into_inner());
    for cost in quantum_field.get_current_layer_costs() {
        costs_container_commands.with_children(|parent| {
            parent.spawn((
                Node {
                    margin: UiRect{ top: Val::Px(4.), bottom: Val::Px(4.), ..default() },
                    ..default()
                },
                CostIndicator::from(*cost),
                QuantumFieldLayerCostPanel,
            ));
        });
    }
}

fn update_quantum_field_action_button_system(
    action_button: Single<(&QuantumFieldActionButton, &mut Node)>,
    action_button_text: Single<&mut Text, With<QuantumFieldActionButtonText>>,
) {
    let (action_button, mut style) = action_button.into_inner();
    let mut text = action_button_text.into_inner();
    match action_button {
        QuantumFieldActionButton::SendExpeditions => {
            text.0 = "Send Expeditions".to_string();
            style.display = Display::Flex;
        },
        QuantumFieldActionButton::StopExpeditions => {
            text.0 = "Stop Expeditions".to_string();
            style.display = Display::Flex;
        },
        QuantumFieldActionButton::PayCost => {
            text.0 = "Pay Cost".to_string();
            style.display = Display::Flex;
        },
        QuantumFieldActionButton::Hidden => {
            style.display = Display::None;
        },
    }
}


fn initialize_quantum_field_panel_content_system(
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
            QuantumFieldPanel,
            children![
                // Top line of the panel
                (
                    Node {
                        width: Val::Percent(100.),
                        flex_direction: FlexDirection::Row,
                        justify_content: JustifyContent::Start,
                        ..default()
                    },
                    children![(
                        Text::new("Quantum Field"),
                        TextColor::from(BLUE),
                        TextLayout::new_with_linebreak(LineBreak::NoWrap),
                        Node {
                            margin: UiRect{ left: Val::Px(4.), right: Val::Px(4.), ..default() },
                            ..default()
                        },
                    )],
                ),
                // Panel Body
                (
                    Node {
                        width: Val::Percent(100.),
                        height: Val::Percent(100.),
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        border: UiRect::all(Val::Px(2.0)),
                        ..default()
                    },
                    //BackgroundColor::from(Color::linear_rgba(0., 0., 0., 0.)),
                    //BorderColor::from(Color::linear_rgba(0., 0.2, 1., 1.)),
                    children![
                        (
                            Node {
                                width: Val::Percent(100.),
                                justify_content: JustifyContent::Center,
                                ..default()
                            },
                            children![(
                                Text::new("Quantum Layer #/#"),
                                TextColor::from(BLUE),
                                TextFont::default().with_font_size(16.0),
                                QuantumFieldLayerText,
                            )]
                        ),
                        (
                            Node {
                                top: Val::Px(2.0),
                                width: Val::Percent(60.),
                                height: Val::Px(20.),
                                ..default()
                            },
                            Healthbar {
                                color: AQUA.into(),
                                ..default()
                            },
                            QuantumFieldLayerHealthbar,
                        ),
                        // Costs Panel - content is dynamic and managed from a dedicated system
                        (
                            Node {
                                width: Val::Percent(100.),
                                flex_direction: FlexDirection::Row,
                                justify_content: JustifyContent::Center,
                                ..default()
                            },
                            QuantumFieldLayerCostsContainer,
                        ),
                        // [Send Expeditions / Stop Expeditions / Pay Cost] Button.
                        QuantumFieldActionButton::default(),
                    ]
                ),
            ],
        ));
    });
}