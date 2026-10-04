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
//! When all layers are solved, the field gains `QuantumFieldSolved` marker and loses its ExpeditionZone.
//!
//! ## Integration with Drones
//!
//! QuantumField has an ExpeditionZone component, making it a valid target for expedition drones.
//! The `process_expeditions_system` consumes accumulated scan progress and applies it to
//! the current layer. This decouples drone mechanics from field-specific progression.

use bevy::{
    color::palettes::css::{AQUA, BLUE},
    prelude::*,
};

use almanach::{Almanach, ObjectPresentation, QuantumFieldInfo, prelude::AlmanachAppExt};
use game_core::prelude::{GridCoords, GridImprint, MapObject, SSS};
use grids::placement::{
    BeginPlacing, CellHighlight, GridObjectPlacer, GridPlacerOverridePropertyRequest, GridsCollectionParam,
    PlacementModes, PlacementValidity, PlaceRequest, RemoveRequest, StopPlacing,
};
use hud::prelude::{BuilderSideMenuItemTooltip, DisplayPanelMainContentRoot, FocusedMapObject};
use logging::prelude::*;
use map_objects::prelude::*;
use persistence::{prelude::*, rusqlite};
use resources::prelude::*;
use states::prelude::{GameState, MapLoadingStage, UiInteraction};
use units::expedition_drone::{DroneState, ExpeditionDrone, ExpeditionDroneDeploymentRequest};
use widgets::prelude::*;

pub(crate) struct QuantumFieldPlugin;
impl Plugin for QuantumFieldPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(PostStartup, initialize_quantum_field_panel_content_system)
            .add_systems(Update, (
                process_expeditions_system.run_if(in_state(GameState::Running)),
                (
                    update_quantum_field_info_panel_system,
                    update_quantum_field_action_button_system,
                ).run_if(in_state(UiInteraction::DisplayInfoPanel)),
            ))
            .add_observer(BuilderQuantumField::on_builder_add_spawn_quantum_field)
            .add_observer(GridPlacerUiForQuantumField::on_add_construct_grid_placer_ui)
            .add_observer(GridPlacerUiForQuantumField::on_begin_placing_spawn_grid_placer_ui)
            .add_observer(GridPlacerUiForQuantumField::on_stop_placing_despawn_grid_placer_ui)
            .add_observer(ArrowButton::on_add_construct_arrow_button)
            .add_observer(QuantumFieldActionButton::on_add_construct_action_button)
            .add_observer(on_focused_map_object_insert_update_quantum_field_panel)
            .add_observer(on_quantum_field_place_request_do_so)
            .add_observer(on_quantum_field_remove_request_do_so)
            .add_observer(on_builder_add_spawn_quantum_field_tooltip)
            .add_systems(CollectSave, collect_quantum_fields)
            .register_loader(MapLoadingStage::SpawnMapElements, "quantum_fields", load_quantum_fields)
            .register_quantum_field(QuantumFieldInfo {
                name: "Quantum Field".to_string(),
                description: "An anomaly preventing Main Base from warping. Every layer must be scanned and solved.".to_string(),
                min_size: 3,
                max_size: 6,
                default_size: 3,
                validate: quantum_field_validator,
                annotate: quantum_field_annotator,
                placement: PlacementModes::default(),
                presentation: ObjectPresentation {
                    tooltip: Some(quantum_field_tooltip),
                },
            });
    }
}

/// Layers are defined at spawn time; current_layer indexes into the layers vec.
#[derive(Component)]
#[require(QuantumField)]
pub(crate) struct QuantumFieldLayers {
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
    pub fn get_current_layer_costs(&self) -> &[ResourceAmount] {
        if self.is_solved() { return &[]; }
        &self.layers[self.current_layer].costs
    }
    /// Single 0→1 "tamed" scalar across all layers. 1.0 once the field is solved.
    pub fn solve_progress(&self) -> f32 {
        let total = self.layers.len().max(1) as f32;
        let partial = if self.is_solved() {
            0.0
        } else {
            let target = self.layers[self.current_layer].value;
            if target > 0.0 { self.current_layer_progress / target } else { 0.0 }
        };
        ((self.current_layer as f32 + partial) / total).clamp(0.0, 1.0)
    }
}

/// A single layer requiring scan progress + resource payment to complete.
pub(crate) struct QuantumFieldLayer {
    /// Scan progress required to complete this layer.
    pub value: f32,
    /// Resources required after scanning to finalize.
    pub costs: Vec<ResourceAmount>,
}

#[derive(Component, SSS)]
pub(crate) struct BuilderQuantumField {
    pub grid_position: GridCoords,
    pub grid_imprint: GridImprint,
    /// Saved layer progress. `None` ⇒ fresh spawn (layer 0, 0.0 progress);
    /// `Some` ⇒ restore from save.
    pub current_layer: Option<usize>,
    pub current_layer_progress: Option<f32>,
}

impl BuilderQuantumField {
    pub fn new(grid_position: GridCoords, grid_imprint: GridImprint) -> Self {
        Self { grid_position, grid_imprint, current_layer: None, current_layer_progress: None }
    }
    pub fn with_current_layer(mut self, current_layer: usize) -> Self {
        self.current_layer = Some(current_layer);
        self
    }
    pub fn with_current_layer_progress(mut self, current_layer_progress: f32) -> Self {
        self.current_layer_progress = Some(current_layer_progress);
        self
    }

    fn on_builder_add_spawn_quantum_field(
        trigger: On<Add, BuilderQuantumField>,
        mut commands: Commands,
        builders: Query<&BuilderQuantumField>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return; };

        let quantum_field = QuantumFieldLayers {
            current_layer: builder.current_layer.unwrap_or(0),
            current_layer_progress: builder.current_layer_progress.unwrap_or(0.0),
            layers: vec![
                QuantumFieldLayer {
                    value: 15000.0,
                    // Temporary placeholder showing three chips. Ignore the fact it's 3x the same resource.
                    costs: vec![ResourceAmount::new(ResourceType::DarkOre, 100), ResourceAmount::new(ResourceType::DarkOre, 100), ResourceAmount::new(ResourceType::DarkOre, 100)],
                },
                QuantumFieldLayer {
                    value: 30000.0,
                    costs: vec![ResourceAmount::new(ResourceType::DarkOre, 200)],
                },
                QuantumFieldLayer {
                    value: 45000.0,
                    costs: vec![ResourceAmount::new(ResourceType::DarkOre, 300)],
                },
            ],
        };

        if (builder.current_layer.is_some() || builder.current_layer_progress.is_some()) && quantum_field.is_solved() {
            commands.entity(entity).insert(QuantumFieldSolved);
        }

        commands.entity(entity)
            .remove::<BuilderQuantumField>()
            .insert((
                Name::new("Quantum Field"),
                // No sprite/mesh: the quantum field is drawn entirely by the
                // `quantum_field_post_process` screen-space pass, which locates the field
                // via its Transform + GridImprint. Transform is still required for the
                // effect and for the info-panel preview camera to follow.
                Transform::from_translation(builder.grid_position.to_world_position_centered(builder.grid_imprint).extend(0.)),
                builder.grid_position,
                builder.grid_imprint,
                quantum_field,
                ExpeditionZone::default(),
            ));
    }
}

#[log_tags(Tag::GameSave)]
fn collect_quantum_fields(
    quantum_fields: Query<(Entity, &GridCoords, &GridImprint, &QuantumFieldLayers)>,
    mut save: SaveWriter,
) {
    if quantum_fields.is_empty() { return; }

    #[debug_dev("Saving {} quantum fields", rows.len())]
    let rows: Vec<(u32, GridCoords, GridImprint, usize, f32)> = quantum_fields
        .iter()
        .map(|(entity, coords, imprint, quantum_field)| {
            (
                entity.index_u32(),
                *coords,
                *imprint,
                quantum_field.current_layer,
                quantum_field.current_layer_progress,
            )
        })
        .collect();
    save.submit(move |ctx| {
        for (id, coords, grid_imprint, current_layer, current_layer_progress) in rows {
            ctx.register_entity(id)?;
            ctx.tx.execute(
                "INSERT OR REPLACE INTO quantum_fields (id, current_layer, current_layer_progress) VALUES (?1, ?2, ?3)",
                rusqlite::params![id, current_layer, current_layer_progress],
            )?;
            ctx.save_grid_coords(id, coords)?;
            ctx.save_grid_imprint(id, grid_imprint)?;
        }
        Ok(())
    });
}

fn load_quantum_fields(ctx: &mut LoadContext) -> LoadResult {
    ctx.for_each_entity("SELECT id, current_layer, current_layer_progress FROM quantum_fields", |ctx, old_id, entity, row| {
        let current_layer: usize = row.get(1)?;
        let current_layer_progress: f32 = row.get(2)?;
        let grid_position = ctx.grid_coords(old_id)?;
        let grid_imprint = ctx.grid_imprint(old_id)?;
        let builder = BuilderQuantumField::new(grid_position, grid_imprint)
            .with_current_layer(current_layer)
            .with_current_layer_progress(current_layer_progress);
        ctx.insert(entity, builder);
        Ok(())
    })
}

fn quantum_field_validator(
    _: MapObject,
    origin: GridCoords,
    imprint: GridImprint,
    grids: &GridsCollectionParam,
) -> PlacementValidity {
    if !origin.are_in_bounds(grids.obstacle_grid.bounds) {
        return PlacementValidity::Invalid;
    }
    if grids.reserved_coords.any_reserved(origin, imprint) {
        return PlacementValidity::Invalid;
    }
    if !grids.obstacle_grid.query_imprint_all(origin, imprint, |field| !field.is_within_quantum_field()) {
        return PlacementValidity::Invalid;
    }
    PlacementValidity::Valid
}

fn quantum_field_annotator(
    _: MapObject,
    origin: GridCoords,
    imprint: GridImprint,
    validity: PlacementValidity,
    grids: &GridsCollectionParam,
) -> Vec<(GridCoords, CellHighlight)> {
    if validity != PlacementValidity::Invalid {
        return vec![];
    }
    imprint.iter(origin)
        .filter(|coords| {
            !coords.are_in_bounds(grids.obstacle_grid.bounds)
                || grids.obstacle_grid[*coords].is_within_quantum_field()
        })
        .map(|coords| (coords, CellHighlight::Negative))
        .collect()
}

#[log_tags(Tag::MapObjects)]
fn on_quantum_field_place_request_do_so(
    trigger: On<PlaceRequest>,
    mut commands: Commands,
    almanach: Res<Almanach>,
    mut grids: GridsCollectionParam,
    placer: Single<(&GridCoords, &GridImprint), With<GridObjectPlacer>>,
) {
    let PlaceRequest(MapObject::QuantumField) = *trigger else { return };
    let (coords, grid_imprint) = placer.into_inner();
    let validity = (almanach.quantum_fields.validate)(MapObject::QuantumField, *coords, *grid_imprint, &grids);
    if validity == PlacementValidity::Invalid { return; }
    #[debug_dev("Quantum field placed at ({}, {})", coords.x, coords.y)]
    commands.spawn(BuilderQuantumField::new(*coords, *grid_imprint));
    grids.reserved_coords.reserve(*coords, *grid_imprint);
}

#[log_tags(Tag::MapObjects)]
fn on_quantum_field_remove_request_do_so(
    trigger: On<RemoveRequest>,
    mut commands: Commands,
    grids: GridsCollectionParam,
    placer: Single<&GridCoords, With<GridObjectPlacer>>,
) {
    let RemoveRequest(MapObject::QuantumField) = *trigger else { return };
    let coords = placer.into_inner();
    #[debug_dev("Quantum field removed at ({}, {})", coords.x, coords.y)]
    if let Some(entity) = grids.obstacle_grid[*coords].quantum_field {
        commands.entity(entity).despawn();
    }
}

#[log_tags(Tag::MapObjects)]
fn process_expeditions_system(
    mut commands: Commands,
    mut quantum_fields: Query<(Entity, &mut QuantumFieldLayers, &mut ExpeditionZone), (Changed<ExpeditionZone>, Without<QuantumFieldSolved>)>,
) {
    for (entity, mut quantum_field, mut expedition_zone) in quantum_fields.iter_mut() {
        if expedition_zone.accumulated_scan_progress > 0. {
            quantum_field.progress_layer(expedition_zone.take_accumulated_scan_progress());
            if quantum_field.is_solved() {
                #[info_player("Quantum field solved")]
                commands.entity(entity).insert(QuantumFieldSolved).remove::<ExpeditionZone>();
            }
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) struct QuantumFieldImprintSelector {
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
pub(crate) struct GridPlacerUiForQuantumField {
    pub imprint_selector: QuantumFieldImprintSelector,
}
impl GridPlacerUiForQuantumField {
    pub fn imprint_label(&self) -> String {
        let size = self.imprint_selector.get_size();
        format!("Quantum Field {}x{}", size, size)
    }

    fn on_add_construct_grid_placer_ui(
        trigger: On<Add, GridPlacerUiForQuantumField>,
        mut commands: Commands,
        grid_placer_ui_for_quantum_field: Single<&GridPlacerUiForQuantumField>,
    ) {
        let entity = trigger.entity;
        let ui_text = grid_placer_ui_for_quantum_field.into_inner().imprint_label();
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

    fn on_begin_placing_spawn_grid_placer_ui(
        trigger: On<BeginPlacing>,
        mut commands: Commands,
        almanach: Res<Almanach>,
    ) {
        let BeginPlacing(MapObject::QuantumField) = *trigger else { return };
        let quantum_field_info = &almanach.quantum_fields;
        let imprint_selector = QuantumFieldImprintSelector::new(
            quantum_field_info.min_size,
            quantum_field_info.max_size,
            quantum_field_info.default_size,
        );
        commands.spawn(GridPlacerUiForQuantumField { imprint_selector });
        commands.trigger(GridPlacerOverridePropertyRequest::OverrideImprint(imprint_selector.get()));
    }

    fn on_stop_placing_despawn_grid_placer_ui(
        _trigger: On<StopPlacing>,
        mut commands: Commands,
        existing_ui: Single<Entity, With<GridPlacerUiForQuantumField>>,
    ) {
        commands.entity(existing_ui.into_inner()).despawn();
    }
}

#[derive(Component)]
#[require(Button, Pickable)]
pub(crate) enum ArrowButton {
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

    fn on_add_construct_arrow_button(
        trigger: On<Add, ArrowButton>,
        mut commands: Commands,
        arrows: Query<&ArrowButton>,
    ) {
        let entity = trigger.entity;
        let Ok(arrow_button) = arrows.get(entity) else { return; };
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
            .observe(Self::on_click_adjust_quantum_field_size);
    }

    #[log_tags(Tag::MapObjects)]
    fn on_click_adjust_quantum_field_size(
        trigger: On<Pointer<Click>>,
        mut commands: Commands,
        ui: Single<(&Children, &mut GridPlacerUiForQuantumField)>,
        arrows: Query<&ArrowButton>,
        mut texts: Query<&mut Text>,
    ) {
        let entity = trigger.entity;
        let (ui_children, mut grid_placer_ui) = ui.into_inner();

        let Ok(arrow_button) = arrows.get(entity) else { return; };
        let _ = match arrow_button {
            ArrowButton::Decrease => grid_placer_ui.imprint_selector.decrease(),
            ArrowButton::Increase => grid_placer_ui.imprint_selector.increase(),
        }.inspect_err(|error| info_player!("Quantum field size unchanged: {error}"));

        if let Ok(mut text) = texts.get_mut(ui_children[1]) {
            text.0 = grid_placer_ui.imprint_label();
        }

        let imprint = grid_placer_ui.imprint_selector.get();
        commands.trigger(GridPlacerOverridePropertyRequest::OverrideImprint(imprint));
    }
}

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
#[derive(Component)]
struct QuantumFieldActionButtonText;

#[derive(Component, Default, PartialEq)]
#[require(Button)]
enum QuantumFieldActionButton {
    #[default]
    Hidden,
    SendIdleDrones,
    PayCost,
}
impl QuantumFieldActionButton {
    fn on_add_construct_action_button(
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
            BackgroundColor::from(Color::srgba(0., 0., 0.2, 0.2)),
            BorderColor::from(Color::srgba(0., 0.2, 1., 1.)),
            children![(
                Text::new("Send Expeditions / Stop Expeditions / Pay cost"),
                TextColor::from(BLUE),
                TextFont::default().with_font_size(12.0),
                QuantumFieldActionButtonText,
            )],
        )).observe(Self::on_click_execute_action);
    }

    #[log_tags(Tag::MapObjects)]
    fn on_click_execute_action(
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
                let sent = drones.iter()
                    .filter(|(_, drone, drone_state)| matches!(drone_state, DroneState::Stationed) && drone.mission_target.is_none())
                    .inspect(|(drone_entity, ..)| commands.trigger(ExpeditionDroneDeploymentRequest { drone: *drone_entity, target: focused_entity }))
                    .count();
                if sent == 0 { info_player!("No idle drones to send"); }
            }
            QuantumFieldActionButton::PayCost => {
                let Ok(mut quantum_field) = quantum_fields.get_mut(focused_entity) else { return; };
                if stock.try_remove_all(quantum_field.get_current_layer_costs()) {
                    // TODO: Paying the last layer solves the field but does not insert `QuantumFieldSolved`.
                    // Only `process_expeditions_system` (on further scan progress) and the load path insert it,
                    // so the field stays unmarked until a drone scans it again or the map reloads. The solved
                    // transition needs a single owner.
                    let solved_layer = quantum_field.current_layer + 1;
                    #[info_player("Quantum layer {solved_layer}/{} solved", quantum_field.layers.len())]
                    quantum_field.move_to_next_layer();
                    commands.entity(focused_entity).insert(FocusedMapObject);
                } else {
                    info_player!("Not enough resources to solve the quantum layer");
                }
            }
            QuantumFieldActionButton::Hidden => {}
        }
        *action_button = QuantumFieldActionButton::Hidden; // To make sure no multi-trigger occurs
    }
}

fn update_quantum_field_info_panel_system(
    focused_quantum_field: Single<&QuantumFieldLayers, With<FocusedMapObject>>,
    action_button: Single<&mut QuantumFieldActionButton>,
    healthbar: Single<&mut Healthbar, With<QuantumFieldLayerHealthbar>>,
    text: Single<&mut Text, With<QuantumFieldLayerText>>,
) {
    let quantum_field = focused_quantum_field.into_inner();
    text.into_inner().0 = if quantum_field.is_solved() {
        "All Quantum Layers Solved".to_string()
    } else {
        format!("Quantum Layer {}/{}", quantum_field.current_layer + 1, quantum_field.layers.len())
    };

    let (current_layer_progress, current_layer_target) = quantum_field.get_progress_details();
    let mut healthbar = healthbar.into_inner();
    healthbar.value = current_layer_progress;
    healthbar.max_value = current_layer_target;

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

fn on_focused_map_object_insert_update_quantum_field_panel(
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
    commands.entity(costs_container.into_inner()).with_children(|parent| {
        for cost in quantum_field.get_current_layer_costs() {
            // The chip owns its own `Node`, so the spacing lives on a wrapper
            // rather than alongside the builder where it would be overwritten.
            parent.spawn((
                Node {
                    margin: UiRect::vertical(Val::Px(4.)),
                    ..default()
                },
                QuantumFieldLayerCostPanel,
                children![(
                    BuilderCostChip(*cost),
                    CostChipVisualFullPrice,
                )],
            ));
        }
    });
}

fn update_quantum_field_action_button_system(
    action_button: Single<(&QuantumFieldActionButton, &mut Node)>,
    action_button_text: Single<&mut Text, With<QuantumFieldActionButtonText>>,
) {
    let (action_button, mut node) = action_button.into_inner();
    let mut text = action_button_text.into_inner();
    match action_button {
        QuantumFieldActionButton::SendIdleDrones => {
            text.0 = "Send Idle Drones".to_string();
            node.display = Display::Flex;
        }
        QuantumFieldActionButton::PayCost => {
            text.0 = "Pay Cost".to_string();
            node.display = Display::Flex;
        }
        QuantumFieldActionButton::Hidden => {
            node.display = Display::None;
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
                                margin: UiRect::horizontal(Val::Px(4.)),
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
                                children![(
                                    BuilderHealthbar::default().with_color(AQUA),
                                    QuantumFieldLayerHealthbar,
                                )],
                            ),
                            // Filled by `on_focused_map_object_insert_update_quantum_field_panel`.
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

/// Queues tooltip construction for a quantum-field placement tile.
pub(crate) fn quantum_field_tooltip(commands: &mut Commands, anchor: Entity, _map_object: MapObject) {
    commands.spawn(BuilderQuantumFieldSideMenuTooltip(anchor));
}

fn on_builder_add_spawn_quantum_field_tooltip(
    trigger: On<Add, BuilderQuantumFieldSideMenuTooltip>,
    mut commands: Commands,
    almanach: Res<Almanach>,
    builders: Query<&BuilderQuantumFieldSideMenuTooltip>,
) {
    let entity = trigger.entity;
    let Ok(builder) = builders.get(entity) else { return; };

    let info = &almanach.quantum_fields;
    // A field is placed at a size the player picks, so its footprint is a range.
    let smallest = GridImprint::Rectangle { width: info.min_size, height: info.min_size };
    let largest = GridImprint::Rectangle { width: info.max_size, height: info.max_size };

    commands.entity(entity)
        .remove::<BuilderQuantumFieldSideMenuTooltip>()
        .insert(
            BuilderSideMenuItemTooltip::new(builder.0)
                .with_name(info.name.clone())
                .with_description(info.description.clone())
                .with_fact(format!("{} to {}", smallest.label(), largest.label())),
        );
}
