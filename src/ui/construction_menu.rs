use bevy::color::palettes::css::{TURQUOISE, WHITE};
use bevy::ui::FocusPolicy;

use crate::prelude::*;
use crate::ui::grid_object_placer::GridObjectPlacerRequest;

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
            .add_observer(ConstructMenuListPicker::on_add)
            .add_observer(ResearchMenuButton::on_add);
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

/// Added to the side-menu research icon. Opens the research panel on click (the research icon is a
/// panel toggle, not a placement-list button).
#[derive(Component)]
struct ResearchMenuButton;
impl ResearchMenuButton {
    fn on_add(trigger: On<Add, ResearchMenuButton>, mut commands: Commands) {
        commands.entity(trigger.entity).observe(Self::on_click);
    }

    fn on_click(
        _trigger: On<Pointer<Click>>,
        mut next_ui_state: ResMut<NextState<UiInteraction>>,
    ) {
        next_ui_state.set(UiInteraction::ResearchPanel);
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
    pub object_type: MapObject,
    pub background_color: Color,
}
impl ConstructObjectButton{
    pub fn new(object_type: MapObject) -> Self {
        Self { 
            object_type,
            background_color: TURQUOISE.into(),
        }
    }

    pub fn new_admin(object_type: MapObject) -> Self {
        Self { 
            object_type,
            background_color: Color::srgb(0.8, 0.3, 0.1), // Custom reddish-orange
        }
    }

    fn on_add(
        trigger: On<Add, ConstructObjectButton>,
        mut commands: Commands,
        almanach: Res<Almanach>,
        buttons: Query<&ConstructObjectButton>,
    ) {
        let entity = trigger.entity;
        let button = buttons.get(entity).unwrap();
        let background_color = button.background_color;
        let object_type = button.object_type;

        let image_handle = almanach.get_placement_info_for(object_type).preview_image;
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
                BackgroundColor(background_color),
            ))
            .observe(Self::on_click)
            .with_children(|parent| {
                if let Some(image_handle) = image_handle {
                    parent.spawn((
                        Node {
                            width: Val::Px(46.0),
                            height: Val::Px(46.0),
                            ..default()
                        },
                        ImageNode::new(image_handle),
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
        grid_object_placer_request.set(button.object_type);
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
                            ConstructObjectButton::new(MapObject::Building(BuildingType::Tower(TowerType::Blaster))),
                            ConstructObjectButton::new(MapObject::Building(BuildingType::Tower(TowerType::Cannon))),
                            ConstructObjectButton::new(MapObject::Building(BuildingType::Tower(TowerType::RocketLauncher))),
                            ConstructObjectButton::new(MapObject::Building(BuildingType::Tower(TowerType::Emitter))),
                            ConstructObjectButton::new(MapObject::Building(BuildingType::Tower(TowerType::Field))),
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
                            ConstructObjectButton::new(MapObject::Building(BuildingType::EnergyRelay)),
                            ConstructObjectButton::new(MapObject::Building(BuildingType::MiningComplex)),
                            ConstructObjectButton::new(MapObject::Building(BuildingType::ExplorationCenter)),
                            ConstructObjectButton::new(MapObject::Building(BuildingType::Forge)),
                        ]
                    )]
                ),
                // Research button — opens the research panel on click
                (
                    ButtonConstructMenu::new("ui/side_menu_research.png"),
                    ResearchMenuButton,
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
                            ConstructObjectButton::new_admin(MapObject::Building(BuildingType::MainBase)),
                            ConstructObjectButton::new_admin(MapObject::DarkOre),
                            ConstructObjectButton::new_admin(MapObject::Wall),
                            ConstructObjectButton::new_admin(MapObject::QuantumField),
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
                            ConstructObjectButton::new_admin(MapObject::Wisp(WispType::Fire)),
                            ConstructObjectButton::new_admin(MapObject::Wisp(WispType::Water)),
                            ConstructObjectButton::new_admin(MapObject::Wisp(WispType::Light)),
                            ConstructObjectButton::new_admin(MapObject::Wisp(WispType::Electric)),
                        ]
                    )],
                ),
            ]
        ));
    }
}