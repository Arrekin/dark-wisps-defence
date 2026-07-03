use bevy::prelude::*;

use logging::prelude::*;
use persistence::{GameMapList, LoadGameSignal, LoadMapConfig};
use states::prelude::*;

pub struct MainMenuPlugin;
impl Plugin for MainMenuPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Startup, |mut commands: Commands| { commands.spawn(MainMenuRoot); })
            .add_systems(OnEnter(UiInteraction::MainMenu), show_main_menu)
            .add_systems(OnExit(UiInteraction::MainMenu), hide_main_menu)
            .add_observer(MainMenuRoot::on_add_build_main_menu)
            .add_observer(LoadMapButton::on_add_build_load_map_button)
            .add_observer(MapListContainer::on_add_build_map_list_container)
            .add_observer(MapEntryButton::on_add_build_map_entry_button)
            ;
    }
}

#[derive(Component)]
struct MainMenuRoot;
impl MainMenuRoot {
    fn on_add_build_main_menu(trigger: On<Add, MainMenuRoot>, mut commands: Commands) {
        commands.entity(trigger.entity).apply_scene(bsn! {
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(0.0),
                left: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
            }
            BackgroundColor(Color::linear_rgba(0.0, 0.0, 0.0, 0.7))
            Visibility::Hidden
            Children [
                (
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(10.0),
                        align_items: AlignItems::Center,
                    }
                    Children [
                        LoadMapButton,
                        MapListContainer,
                    ]
                )
            ]
        });
    }
}

#[derive(Component, Default, Clone)]
#[require(Button)]
struct LoadMapButton;
impl LoadMapButton {
    fn on_add_build_load_map_button(trigger: On<Add, LoadMapButton>, mut commands: Commands) {
        commands.entity(trigger.entity)
            .apply_scene(bsn! {
                Node {
                    width: Val::Px(220.0),
                    height: Val::Px(40.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                }
                BackgroundColor(Color::linear_rgba(0.2, 0.2, 0.8, 1.0))
                Children [
                    ( Text("Load Map") template_value(TextLayout::no_wrap()) )
                ]
                on(Self::on_click_toggle_map_list)
            });
    }

    fn on_click_toggle_map_list(
        _trigger: On<Pointer<Click>>,
        mut commands: Commands,
        map_list: Res<GameMapList>,
        map_list_container: Single<(Entity, &mut Node), With<MapListContainer>>,
    ) {
        let (container_entity, mut node) = map_list_container.into_inner();

        if node.display == Display::Flex {
            node.display = Display::None;
            return;
        }

        node.display = Display::Flex;
        commands.entity(container_entity).despawn_related::<Children>();

        commands.entity(container_entity).with_children(|parent| {
            for name in &map_list.names {
                parent.spawn(MapEntryButton { name: name.clone() });
            }
        });
    }
}

#[derive(Component, Default, Clone)]
#[require(Node)]
struct MapListContainer;
impl MapListContainer {
    fn on_add_build_map_list_container(trigger: On<Add, MapListContainer>, mut commands: Commands) {
        commands.entity(trigger.entity).apply_scene(bsn! {
            Node {
                display: Display::None,
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Stretch,
                row_gap: Val::Px(6.0),
                margin: { UiRect { top: Val::Px(12.0), ..default() } },
            }
        });
    }
}

#[derive(Component)]
#[require(Button)]
struct MapEntryButton { name: String }
impl MapEntryButton {
    fn on_add_build_map_entry_button(trigger: On<Add, MapEntryButton>, mut commands: Commands, entries: Query<&MapEntryButton>) {
        let entity = trigger.entity;
        let name = entries.get(entity).unwrap().name.clone();

        commands.entity(entity)
            .apply_scene(bsn! {
                Node {
                    width: Val::Px(260.0),
                    height: Val::Px(34.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                }
                BackgroundColor(Color::linear_rgba(0.3, 0.3, 0.3, 1.0))
                Children [
                    ( Text(name) template_value(TextLayout::no_wrap()) )
                ]
                on(Self::on_click_load_selected_map)
            });
    }

    fn on_click_load_selected_map(trigger: On<Pointer<Click>>, mut commands: Commands, entries: Query<&MapEntryButton>) {
        let entity = trigger.entity;
        let Ok(entry) = entries.get(entity) else { return; };
        Log::debug().dev().tag(Tag::Ui).message(format!("Map selected: {}", entry.name));
        commands.trigger(LoadGameSignal(LoadMapConfig::new(format!("maps/{}.dwd", entry.name))));
    }
}

fn show_main_menu(
    mut next_game_state: ResMut<NextState<GameState>>,
    menu: Single<&mut Visibility, With<MainMenuRoot>>,
) {
    *menu.into_inner() = Visibility::Inherited;
    next_game_state.set(GameState::Paused);
}

fn hide_main_menu(
    mut next_game_state: ResMut<NextState<GameState>>,
    current_game_state: Res<State<GameState>>,
    menu: Single<&mut Visibility, With<MainMenuRoot>>,
) {
    *menu.into_inner() = Visibility::Hidden;
    if matches!(current_game_state.get(), GameState::Paused) {
        next_game_state.set(GameState::Running);
    }
}
