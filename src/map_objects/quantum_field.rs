//! # Quantum Field
//!
//! Mysterious anomalies whose fluctuations prevent the main base from teleporting away.
//! All quantum fields on a map must be solved for the player to complete the scenario.
//!
//! ## Layer Progression
//!
//! Each QuantumField has multiple layers that must be solved sequentially:
//! 1. **Scan phase** - Expedition drones accumulate progress via ExpeditionZone
//! 2. **Pay phase** - Once scanned, player pays resource costs to finalize the layer
//! 3. **Repeat** - Next layer begins until all layers solved
//!
//! When all layers are solved, the field gains `Solved` marker and loses its ExpeditionZone.
//!
//! ## Integration with Drones
//!
//! QuantumField has an ExpeditionZone component, making it a valid target for expedition drones.
//! The `process_expeditions_system` consumes accumulated scan progress and applies it to
//! the current layer. This decouples drone mechanics from field-specific progression.

use bevy::color::palettes::css::{AQUA, BLUE};

use lib_core::map_objects::QuantumField;
use lib_inventory::almanach::QuantumFieldInfo;
use lib_inventory::placement::{CellHighlight, GridsCollectionParam, PlacementValidity};
use lib_ui::prelude::*;

use crate::map_objects::common::ExpeditionZone;
use crate::prelude::*;
use crate::ui::display_info_panel::{DisplayPanelMainContentRoot, FocusedMapObject};
use crate::ui::grid_object_placer::GridObjectPlacer;
use crate::units::expedition_drone::{
    DroneState, ExpeditionDrone, ExpeditionDroneDeploymentRequest,
};

pub struct QuantumFieldPlugin;
impl Plugin for QuantumFieldPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostStartup, (
            initialize_quantum_field_panel_content_system,
        ))
        .add_systems(Update, (
            process_expeditions_system.run_if(in_state(GameState::Running)),
            (
                update_quantum_field_info_panel_system,
                update_quantum_field_action_button_system,
            ).run_if(in_state(UiInteraction::DisplayInfoPanel)),
        ))
        .add_observer(BuilderQuantumField::on_add)
        .add_observer(GridPlacerUiForQuantumField::on_add)
        .add_observer(GridPlacerUiForQuantumField::on_begin_placing)
        .add_observer(GridPlacerUiForQuantumField::on_stop_placing)
        .add_observer(ArrowButton::on_add)
        .add_observer(QuantumFieldActionButton::on_add)
        .add_observer(on_focused_map_object_insert)
        .add_observer(on_quantum_field_place_request)
        .add_observer(on_quantum_field_remove_request)
        .register_db_loader::<BuilderQuantumField>(MapLoadingStage::SpawnMapElements)
        .register_db_saver(BuilderQuantumField::on_game_save)
        .register_quantum_field(QuantumFieldInfo {
            name: "Quantum Field".to_string(),
            min_size: 3,
            max_size: 6,
            default_size: 3,
            validate: quantum_field_validator,
            annotate: quantum_field_annotator,
        })
        ;
    }
}

/// Marker for fully-solved QuantumFields. Removes ExpeditionZone to prevent further scanning.
#[derive(Component)]
pub struct Solved;

/// Layers are defined at spawn time; current_layer indexes into the layers vec.
#[derive(Component)]
#[require(QuantumField)]
pub struct QuantumFieldLayers {
    pub layers: Vec<QuantumFieldLayer>,
    pub current_layer: usize,        // index into layers; equals layers.len() when solved
    pub current_layer_progress: f32, // scan progress toward current layer's value
}
impl QuantumFieldLayers {
    pub fn progress_layer(&mut self, amount: f32) {
        if self.is_solved() { return; }
        self.current_layer_progress = (self.current_layer_progress + amount).min(self.layers[self.current_layer].value);
    }
    pub fn move_to_next_layer(&mut self) {
        if self.is_solved() { return; }
        self.current_layer += 1;
        self.current_layer_progress = 0.0;
    }
    /// Returns (current_layer_progress, current_layer_target)
    pub fn get_progress_details(&self) -> (f32, f32) {
        if self.is_solved() { return (0.0, 0.0); }
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

/// A single layer requiring scan progress + resource payment to complete.
pub struct QuantumFieldLayer {
    pub value: f32,       // scan progress required to complete this layer
    pub costs: Vec<Cost>, // resources required after scanning to finalize
}

#[derive(Clone, Copy, Debug)]
pub struct QuantumFieldSaveData {
    pub entity: Entity,
    pub current_layer: usize,
    pub current_layer_progress: f32,
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
        let entity_index = save_data.entity.index_u32() as i64;

        // 1. Insert into quantum_fields table
        tx.register_entity(entity_index)?;
        tx.execute(
            "INSERT OR REPLACE INTO quantum_fields (id, current_layer, current_layer_progress) VALUES (?1, ?2, ?3)",
            rusqlite::params![entity_index, save_data.current_layer, save_data.current_layer_progress],
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
        let mut stmt = ctx.conn.prepare("SELECT id, current_layer, current_layer_progress FROM quantum_fields LIMIT ?1 OFFSET ?2")?;
        let mut rows = stmt.query(ctx.pagination.as_params())?;

        let mut count = 0;
        while let Some(row) = rows.next()? {
            let old_id: i64 = row.get(0)?;
            let current_layer: usize = row.get(1)?;
            let current_layer_progress: f32 = row.get(2)?;

            let grid_position = ctx.conn.get_grid_coords(old_id)?;
            let grid_imprint = ctx.conn.get_grid_imprint(old_id)?;

            if let Some(new_entity) = ctx.get_new_entity_for_old(old_id) {
                let save_data = QuantumFieldSaveData {
                    entity: new_entity,
                    current_layer,
                    current_layer_progress,
                };
                ctx.commands.entity(new_entity).insert(BuilderQuantumField::new_for_saving(grid_position, grid_imprint, save_data));
            } else {
                Log::warn().dev().tag(Tag::GameLoad).message(format!("QuantumField with old ID {old_id} has no corresponding new entity"));
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
        quantum_fields: Query<(Entity, &GridCoords, &GridImprint, &QuantumFieldLayers)>,
    ) {
        if quantum_fields.is_empty() { return; }
        Log::debug().dev().tag(Tag::GameSave).message(format!("Saving {} quantum fields", quantum_fields.iter().count()));
        let batch = quantum_fields.iter().map(|(entity, coords, imprint, quantum_field)| {
                let save_data = QuantumFieldSaveData {
                    entity,
                    current_layer: quantum_field.current_layer,
                    current_layer_progress: quantum_field.current_layer_progress,
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

        let mut quantum_field = QuantumFieldLayers {
            current_layer: 0,
            current_layer_progress: 0.0,
            layers: vec![
                QuantumFieldLayer {
                    value: 15000.0,
                    costs: vec![Cost{ resource_type: ResourceType::DarkOre, amount: 100}, Cost{ resource_type: ResourceType::DarkOre, amount: 100}, Cost{ resource_type: ResourceType::DarkOre, amount: 100}],
                },
                QuantumFieldLayer {
                    value: 30000.0,
                    costs: vec![Cost{ resource_type: ResourceType::DarkOre, amount: 200}],
                },
                QuantumFieldLayer {
                    value: 45000.0,
                    costs: vec![Cost{ resource_type: ResourceType::DarkOre, amount: 300}],
                },
            ],
        };

        if let Some(save_data) = &builder.save_data {
            quantum_field.current_layer = save_data.current_layer;
            quantum_field.current_layer_progress = save_data.current_layer_progress;

            if quantum_field.is_solved() {
                commands.entity(entity).insert(Solved);
            }
        }

        commands.entity(entity)
            .remove::<BuilderQuantumField>()
            .insert((
                Name::new("Quantum Field"),
                // No sprite/mesh: the quantum field is drawn entirely by the
                // `quantum_field_post_process` screen-space pass, which locates the field
                // via its Transform + GridImprint. Transform is still required for the
                // effect and for the info-panel preview camera to follow.
                Transform::from_translation(builder.grid_position.to_world_position_centered(builder.grid_imprint).extend(Z_OBSTACLE)),
                builder.grid_position,
                builder.grid_imprint,
                quantum_field,
                ExpeditionZone::default(),
            ));
    }
}

pub fn quantum_field_validator(
    _: MapObject,
    coords: GridCoords,
    imprint: GridImprint,
    grids: &GridsCollectionParam,
) -> PlacementValidity {
    if !coords.is_in_bounds(grids.obstacle_grid.bounds()) {
        return PlacementValidity::Invalid;
    }
    if grids.reserved_coords.any_reserved(coords, imprint) {
        return PlacementValidity::Invalid;
    }
    if !grids.obstacle_grid.query_imprint_all(coords, imprint, |f| !f.is_within_quantum_field()) {
        return PlacementValidity::Invalid;
    }
    PlacementValidity::Valid
}

pub fn quantum_field_annotator(
    _: MapObject,
    coords: GridCoords,
    imprint: GridImprint,
    validity: PlacementValidity,
    grids: &GridsCollectionParam,
) -> Vec<(GridCoords, CellHighlight)> {
    if validity != PlacementValidity::Invalid {
        return vec![];
    }
    imprint.iter(coords)
        .filter(|c| {
            !c.is_in_bounds(grids.obstacle_grid.bounds())
                || grids.obstacle_grid[*c].is_within_quantum_field()
        })
        .map(|c| (c, CellHighlight::Negative))
        .collect()
}

fn on_quantum_field_place_request(
    _trigger: On<lib_core::placement::PlaceRequest<QuantumField>>,
    mut commands: Commands,
    almanach: Res<Almanach>,
    mut grids: GridsCollectionParam,
    placer: Single<(&GridCoords, &GridImprint), With<GridObjectPlacer>>,
) {
    let (coords, grid_imprint) = placer.into_inner();
    let validity = (almanach.quantum_fields.validate)(MapObject::QuantumField, *coords, *grid_imprint, &grids);
    if validity == PlacementValidity::Invalid { return; }
    commands.spawn(BuilderQuantumField::new(*coords, *grid_imprint));
    grids.reserved_coords.reserve(*coords, *grid_imprint);
}

fn on_quantum_field_remove_request(
    _trigger: On<lib_core::placement::RemoveRequest<QuantumField>>,
    mut commands: Commands,
    grids: GridsCollectionParam,
    placer: Single<&GridCoords, With<GridObjectPlacer>>,
) {
    let coords = placer.into_inner();
    if let Some(entity) = grids.obstacle_grid[*coords].quantum_field {
        commands.entity(entity).despawn();
    }
}

fn process_expeditions_system(
    mut commands: Commands,
    mut quantum_fields: Query<(Entity, &mut QuantumFieldLayers, &mut ExpeditionZone), (Changed<ExpeditionZone>, Without<Solved>)>,
) {
    for (entity, mut quantum_field, mut expedition_zone) in quantum_fields.iter_mut() {
        if expedition_zone.accumulated_scan_progress > 0. {
            quantum_field.progress_layer(expedition_zone.take_accumulated_scan_progress());
            if quantum_field.is_solved() {
                commands.entity(entity).insert(Solved).remove::<ExpeditionZone>();
            }
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct QuantumFieldImprintSelector {
    current: i32,
    min: i32,
    max: i32,
}
impl QuantumFieldImprintSelector {
    pub fn new(min: i32, max: i32, default: i32) -> Self {
        Self { current: default, min, max }
    }
    pub fn get_size(&self) -> i32 {
        self.current
    }
    pub fn get(&self) -> GridImprint {
        GridImprint::Rectangle { width: self.current, height: self.current }
    }
    pub fn increase(&mut self) -> Result<(), String> {
        if self.current < self.max {
            self.current += 1;
            Ok(())
        } else {
            Err(format!("Already at max size {}", self.max))
        }
    }
    pub fn decrease(&mut self) -> Result<(), String> {
        if self.current > self.min {
            self.current -= 1;
            Ok(())
        } else {
            Err(format!("Already at min size {}", self.min))
        }
    }
}

/// Editor UI for selecting QuantumField size before placement.
#[derive(Component)]
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

    fn on_begin_placing(
        _trigger: On<lib_core::placement::BeginPlacing<QuantumField>>,
        mut commands: Commands,
        almanach: Res<Almanach>,
    ) {
        let qf_config = &almanach.quantum_fields;
        commands.spawn(GridPlacerUiForQuantumField {
            imprint_selector: QuantumFieldImprintSelector::new(qf_config.min_size, qf_config.max_size, qf_config.default_size),
        });
        // Override placer imprint with the selector's default
        let imprint = GridImprint::Rectangle { width: qf_config.default_size, height: qf_config.default_size };
        commands.trigger(lib_core::placement::GridPlacerOverridePropertyRequest::OverrideImprint(imprint));
    }

    fn on_stop_placing(
        _trigger: On<lib_core::placement::StopPlacing>,
        mut commands: Commands,
        existing_ui: Single<Entity, With<GridPlacerUiForQuantumField>>,
    ) {
        commands.entity(existing_ui.into_inner()).despawn();
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
    fn on_add(trigger: On<Add, ArrowButton>, mut commands: Commands, arrows: Query<&ArrowButton>) {
        let entity = trigger.entity;
        let arrow_button = arrows.get(entity).unwrap();
        commands
            .entity(entity)
            .insert((
                Node {
                    width: Val::Px(16.),
                    height: Val::Px(16.),
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                BackgroundColor(Color::BLACK),
                children![(
                    Text::new(arrow_button.text()),
                    TextFont::default().with_font_size(12.)
                )],
            ))
            .observe(Self::on_click);
    }

    fn on_click(
        trigger: On<Pointer<Click>>,
        mut commands: Commands,
        ui: Single<(&Children, &mut GridPlacerUiForQuantumField)>,
        arrows: Query<&ArrowButton>,
        mut texts: Query<&mut Text>,
    ) {
        let entity = trigger.entity;
        let (ui_children, mut grid_placer_ui) = ui.into_inner();

        let arrow_button = arrows.get(entity).unwrap();
        match arrow_button {
            ArrowButton::Decrease => {
                let _ = grid_placer_ui.imprint_selector.decrease();
            }
            ArrowButton::Increase => {
                let _ = grid_placer_ui.imprint_selector.increase();
            }
        }

        let ui_text = grid_placer_ui.imprint_str();
        texts.get_mut(ui_children[1]).unwrap().0 = ui_text;

        // Request placer to update its imprint
        let imprint = grid_placer_ui.imprint_selector.get();
        commands.trigger(
            lib_core::placement::GridPlacerOverridePropertyRequest::OverrideImprint(imprint),
        );
    }
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
    SendIdleDrones,
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
        focused_quantum_field: Single<Entity, With<FocusedMapObject>>,
        action_button: Single<&mut QuantumFieldActionButton>,
        mut quantum_fields: Query<&mut QuantumFieldLayers>,
        drones: Query<(Entity, &ExpeditionDrone, &DroneState)>,
    ) {
        let focused_entity = focused_quantum_field.into_inner();
        let mut action_button = action_button.into_inner();
        match *action_button {
            QuantumFieldActionButton::SendIdleDrones => {
                // Send all idle drones
                for (drone_entity, drone, drone_state) in drones.iter() {
                    if matches!(drone_state, DroneState::Stationed) && drone.mission_target.is_none() {
                        commands.trigger(ExpeditionDroneDeploymentRequest {
                            drone: drone_entity,
                            target: focused_entity,
                        });
                    }
                }
            }
            QuantumFieldActionButton::PayCost => {
                let Ok(mut quantum_field) = quantum_fields.get_mut(focused_entity) else { return; };
                if stock.try_pay_costs(quantum_field.get_current_layer_costs()) {
                    quantum_field.move_to_next_layer();
                    commands.entity(focused_entity).insert(FocusedMapObject);
                }
            }
            QuantumFieldActionButton::Hidden => {}
        }
        *action_button = QuantumFieldActionButton::Hidden; // To make sure no multi-trigger occurs
    }
}
#[derive(Component)]
struct QuantumFieldActionButtonText;

fn update_quantum_field_info_panel_system(
    focused_quantum_field: Single<&QuantumFieldLayers, With<FocusedMapObject>>,
    action_button: Single<&mut QuantumFieldActionButton>,
    healthbar: Single<&mut Healthbar, With<QuantumFieldLayerHealthbar>>,
    text: Single<&mut Text, With<QuantumFieldLayerText>>,
) {
    let quantum_field = focused_quantum_field.into_inner();
    // Update the layer text
    text.into_inner().0 = if quantum_field.is_solved() {
        "All Quantum Layers Solved".to_string()
    } else {
        format!("Quantum Layer {}/{}", quantum_field.current_layer + 1, quantum_field.layers.len())
    };
    // Update the layer progress
    let (current_layer_progress, current_layer_target) = quantum_field.get_progress_details();
    let mut healthbar = healthbar.into_inner();
    healthbar.value = current_layer_progress;
    healthbar.max_value = current_layer_target;
    // Update the action button
    *action_button.into_inner() = {
        if quantum_field.is_solved() {
            QuantumFieldActionButton::Hidden
        } else if quantum_field.is_current_layer_solved() {
            QuantumFieldActionButton::PayCost
        } else {
            QuantumFieldActionButton::SendIdleDrones
        }
    };
}

fn on_focused_map_object_insert(
    trigger: On<Insert, FocusedMapObject>,
    mut commands: Commands,
    quantum_fields: Query<&QuantumFieldLayers>,
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
        QuantumFieldActionButton::SendIdleDrones => {
            text.0 = "Send Idle Drones".to_string();
            style.display = Display::Flex;
        }
        QuantumFieldActionButton::PayCost => {
            text.0 = "Pay Cost".to_string();
            style.display = Display::Flex;
        }
        QuantumFieldActionButton::Hidden => {
            style.display = Display::None;
        }
    }
}

fn initialize_quantum_field_panel_content_system(
    mut commands: Commands,
    display_info_panel_main_content_root: Single<Entity, With<DisplayPanelMainContentRoot>>,
) {
    commands
        .entity(display_info_panel_main_content_root.into_inner())
        .with_children(|parent| {
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
                            TextLayout::no_wrap(),
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
