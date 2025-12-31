use bevy::color::palettes::css::{TURQUOISE, WHITE};
use bevy::ui::FocusPolicy;

use crate::buildings::tower_emitter::TOWER_EMITTER_BASE_IMAGE;
use crate::map_objects::walls::WALL_BASE_IMAGE;
use crate::prelude::*;
use crate::buildings::energy_relay::ENERGY_RELAY_BASE_IMAGE;
use crate::buildings::exploration_center::EXPLORATION_CENTER_BASE_IMAGE;
use crate::buildings::main_base::MAIN_BASE_BASE_IMAGE;
use crate::buildings::mining_complex::MINING_COMPLEX_BASE_IMAGE;
use crate::buildings::tower_blaster::TOWER_BLASTER_BASE_IMAGE;
use crate::buildings::tower_cannon::TOWER_CANNON_BASE_IMAGE;
use crate::buildings::tower_rocket_launcher::TOWER_ROCKET_LAUNCHER_BASE_IMAGE;
use crate::map_objects::dark_ore::DARK_ORE_BASE_IMAGES;
use crate::map_objects::quantum_field::QuantumFieldImprintSelector;
use crate::ui::grid_object_placer::{GridObjectPlacer, GridObjectPlacerRequest};
use crate::wisps::components::WispType;

const NOT_HOVERED_ALPHA: f32 = 0.2;
const CONSTRUCT_MENU_BUTTON_WIDTH: f32 = 65.;
const CONSTRUCT_MENU_BUTTON_HEIGHT: f32 = 64.;

pub struct ConstructionMenuPlugin;
impl Plugin for ConstructionMenuPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Startup, (
                SideMenu::setup,
            ))
            .add_systems(Update, (
                AdminOnly::on_admin_mode_change.run_if(state_changed::<AdminMode>),
            ))
            .add_observer(ConstructObjectButton::on_add)
            .add_observer(ButtonConstructMenu::on_add)
            .add_observer(ConstructMenuListPicker::on_add);
    }
}

#[derive(Component)]
#[require(Button)]
pub struct ButtonConstructMenu {
    icon_path: &'static str,
}
impl ButtonConstructMenu {
    pub fn new(icon_path: &'static str) -> Self {
        Self { icon_path }
    }

    fn on_add(
        trigger: On<Add, ButtonConstructMenu>,
        mut commands: Commands,
        asset_server: Res<AssetServer>,
        buttons: Query<&ButtonConstructMenu>,
    ) {
        let entity = trigger.entity;
        let icon_path = buttons.get(entity).unwrap().icon_path;

        commands.entity(entity).insert((
            Node {
                width: Val::Px(CONSTRUCT_MENU_BUTTON_WIDTH),
                height: Val::Px(CONSTRUCT_MENU_BUTTON_HEIGHT),
                ..default()
            },
            ImageNode::new(asset_server.load(icon_path)).with_color(WHITE.with_alpha(NOT_HOVERED_ALPHA).into()),
        ))
        .observe(Self::on_mouse_over)
        .observe(Self::on_mouse_out);
    }

    fn on_mouse_over(
        trigger: On<Pointer<Over>>,
        mut menu_buttons: Query<(&mut ImageNode, &Children), With<ButtonConstructMenu>>,
        mut list_pickers: Query<&mut Visibility, With<ConstructMenuListPicker>>,
    ) -> Result<()> {
        let entity = trigger.entity;
        let (mut ui_image, children) = menu_buttons.get_mut(entity)?;
        let list_picker_entity = children.get(0).ok_or("List picker not found")?;
        let mut list_picker_visibility = list_pickers.get_mut(*list_picker_entity)?;
        ui_image.color.set_alpha(1.);
        *list_picker_visibility = Visibility::Inherited;
        Ok(())
    }
    
    fn on_mouse_out(
        trigger: On<Pointer<Out>>,
        mut menu_buttons: Query<(&mut ImageNode, &Children), With<ButtonConstructMenu>>,
        mut list_pickers: Query<&mut Visibility, With<ConstructMenuListPicker>>,
    ) -> Result<()> {
        let entity = trigger.entity;
        let (mut ui_image, children) = menu_buttons.get_mut(entity)?;
        let list_picker_entity = children.get(0).ok_or("List picker not found")?;
        let mut list_picker_visibility = list_pickers.get_mut(*list_picker_entity)?;
        ui_image.color.set_alpha(NOT_HOVERED_ALPHA);
        *list_picker_visibility = Visibility::Hidden;
        Ok(())
    }
}

#[derive(Component)]
struct AdminOnly;
impl AdminOnly {
    fn on_admin_mode_change(
        admin_mode: Res<State<AdminMode>>,
        mut menu_buttons: Query<&mut Visibility, With<AdminOnly>>,
    ) {
        let new_visibility = if matches!(admin_mode.get(), AdminMode::Enabled) { Visibility::Inherited } else { Visibility::Hidden };
        for mut visibility in menu_buttons.iter_mut() {
            *visibility = new_visibility;
        }
    }
}

#[derive(Component, Default)]
#[require(Button)]
pub struct ConstructMenuListPicker;
impl ConstructMenuListPicker {
    fn on_add(
        trigger: On<Add, ConstructMenuListPicker>,
        mut commands: Commands,
    ) {
        let entity = trigger.entity;
        commands.entity(entity).insert((
            Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                left: Val::Px(64.),
                padding: UiRect {
                    left: Val::Px(2.5),
                    right: Val::Px(2.5),
                    top: Val::ZERO,
                    bottom: Val::ZERO,
                },
                ..default()
            },
            Visibility::Hidden,
            BackgroundColor(Color::BLACK.into()),
            GlobalZIndex(-1),
        ));
    }
}

#[derive(Component)]
#[require(Button, FocusPolicy)]
pub struct ConstructObjectButton {
    pub object_type: GridObjectPlacer,
    pub background_color: Color,
}
impl ConstructObjectButton{
    pub fn new(object_type: GridObjectPlacer) -> Self {
        Self { 
            object_type,
            background_color: TURQUOISE.into(),
        }
    }

    pub fn new_admin(object_type: GridObjectPlacer) -> Self {
        Self { 
            object_type,
            background_color: Color::srgb(0.8, 0.3, 0.1), // Custom reddish-orange
        }
    }

    fn on_add(
        trigger: On<Add, ConstructObjectButton>,
        mut commands: Commands,
        asset_server: Res<AssetServer>,
        builders: Query<&ConstructObjectButton>,
    ) {
        let entity = trigger.entity;
        let button = builders.get(entity).unwrap();

        commands.entity(entity)
            .insert((
                Node {
                    width: Val::Px(48.),
                    height: Val::Px(48.),
                    margin: UiRect {
                        left: Val::Px(2.5),
                        right: Val::Px(2.5),
                        top: Val::ZERO,
                        bottom: Val::ZERO,
                    },
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                BackgroundColor(button.background_color),
            ))
            .observe(Self::on_click)
            .with_children(|parent| {
                let object_type = &button.object_type;
                let image_handle = match &object_type {
                    GridObjectPlacer::Building(building_type) => match building_type {
                        BuildingType::Tower(tower_type) => {
                            match tower_type {
                                TowerType::Blaster => Some(TOWER_BLASTER_BASE_IMAGE),
                                TowerType::Cannon => Some(TOWER_CANNON_BASE_IMAGE),
                                TowerType::RocketLauncher => Some(TOWER_ROCKET_LAUNCHER_BASE_IMAGE),
                                TowerType::Emitter => Some(TOWER_EMITTER_BASE_IMAGE),
                            }
                        },
                        BuildingType::MainBase => Some(MAIN_BASE_BASE_IMAGE),
                        BuildingType::EnergyRelay => Some(ENERGY_RELAY_BASE_IMAGE),
                        BuildingType::ExplorationCenter => Some(EXPLORATION_CENTER_BASE_IMAGE),
                        BuildingType::MiningComplex => Some(MINING_COMPLEX_BASE_IMAGE),
                    },
                    GridObjectPlacer::DarkOre => Some(DARK_ORE_BASE_IMAGES[0]),
                    GridObjectPlacer::Wall => Some(WALL_BASE_IMAGE),
                    _ => None,
                };
                if let Some(image_handle) = image_handle {
                    parent.spawn((
                        Node {
                            width: Val::Px(46.0),
                            height: Val::Px(46.0),
                            ..default()
                        },
                        
                        ImageNode::new(asset_server.load(image_handle)),
                    ));
                }
            });
    }

    fn on_click(
        trigger: On<Pointer<Click>>, 
        mut grid_object_placer_request: ResMut<GridObjectPlacerRequest>,
        menu_buttons: Query<&ConstructObjectButton>,
        mut list_pickers: Query<(&mut Interaction, &mut Visibility), With<ConstructMenuListPicker>>,
    ) {
        let entity = trigger.entity;
        let Ok(button) = menu_buttons.get(entity) else { return; };
        grid_object_placer_request.set(button.object_type.clone());
        list_pickers.iter_mut().for_each(|(mut interaction, mut visibility)| { *visibility = Visibility::Hidden; *interaction = Interaction::None; });
    }
}

#[derive(Component)]
struct SideMenu;
impl SideMenu {
    pub fn setup(
        mut commands: Commands,
    ) {
        commands.spawn((
            SideMenu,
            Node { // Root node
                position_type: PositionType::Absolute,
                top: Val::Percent(30.),
                left: Val::Px(5.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                ..default()
            },
            children![
                // Construct towers button
                (
                    ButtonConstructMenu::new("ui/side_menu_towers.png"),
                    // Construct towers list picker
                    children![(
                        ConstructMenuListPicker,
                        children![
                            // Specific tower to construct
                            ConstructObjectButton::new(BuildingType::Tower(TowerType::Blaster).into()),
                            ConstructObjectButton::new(BuildingType::Tower(TowerType::Cannon).into()),
                            ConstructObjectButton::new(BuildingType::Tower(TowerType::RocketLauncher).into()),
                            ConstructObjectButton::new(BuildingType::Tower(TowerType::Emitter).into()),
                        ]
                    )]
                ),
                // Construct buildings button
                (
                    ButtonConstructMenu::new("ui/side_menu_buildings.png"),
                    // Construct buildings list picker
                    children![(
                        ConstructMenuListPicker,
                        children![
                            // Specific building to construct
                            ConstructObjectButton::new(BuildingType::EnergyRelay.into()),
                            ConstructObjectButton::new(BuildingType::MiningComplex.into()),
                            ConstructObjectButton::new(BuildingType::ExplorationCenter.into()),
                        ]
                    )]
                ),
                // Construct research button
                (
                    ButtonConstructMenu::new("ui/side_menu_research.png"),
                    // Construct research list picker
                    children![(
                        ConstructMenuListPicker,
                    )],
                ),
                // Construct upgrades button
                (
                    ButtonConstructMenu::new("ui/side_menu_upgrades.png"),
                    // Construct upgrades list picker
                    children![(
                        ConstructMenuListPicker,
                    )],
                ),
                // Construct consumables button
                (
                    ButtonConstructMenu::new("ui/side_menu_consumables.png"),
                    // Construct consumables list picker
                    children![(
                        ConstructMenuListPicker,
                    )],
                ),
                // Construct objects(editor) button
                (
                    ButtonConstructMenu::new("ui/side_menu_admin_objects.png"),
                    AdminOnly,
                    // Construct objects(editor) list picker
                    children![(
                        ConstructMenuListPicker,
                        children![
                            // Specific editor building to construct
                            ConstructObjectButton::new_admin(BuildingType::MainBase.into()),
                            ConstructObjectButton::new_admin(GridObjectPlacer::DarkOre),
                            ConstructObjectButton::new_admin(GridObjectPlacer::Wall),
                            ConstructObjectButton::new_admin(GridObjectPlacer::QuantumField(QuantumFieldImprintSelector::default())),
                        ]
                    )]
                ),
                // Construct wisps button
                (
                    ButtonConstructMenu::new("ui/side_menu_admin_wisps.png"),
                    AdminOnly,
                    // Construct wisps list picker
                    children![(
                        ConstructMenuListPicker,
                        children![
                            // Specific wisp to construct
                            ConstructObjectButton::new_admin(WispType::Fire.into()),
                            ConstructObjectButton::new_admin(WispType::Water.into()),
                            ConstructObjectButton::new_admin(WispType::Light.into()),
                            ConstructObjectButton::new_admin(WispType::Electric.into()),
                        ]
                    )],
                ),
            ]
        ));
    }
}