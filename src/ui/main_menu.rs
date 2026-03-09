use crate::prelude::*;

pub struct MainMenuPlugin;
impl Plugin for MainMenuPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Startup, |mut commands: Commands| { commands.spawn(MainMenuRoot); })
            .add_systems(OnEnter(UiInteraction::MainMenu), on_menu_enter_system)
            .add_systems(OnExit(UiInteraction::MainMenu), on_menu_exit_system)
            .add_observer(MainMenuRoot::on_add)
            .add_observer(LoadMapButton::on_add)
            .add_observer(MapListContainer::on_add)
            .add_observer(MapEntryButton::on_add)
            ;
    }
}

#[derive(Component)]
struct MainMenuRoot;
impl MainMenuRoot {
    fn on_add(trigger: On<Add, MainMenuRoot>, mut commands: Commands) {
        let entity = trigger.entity;
        commands.entity(entity)
            .insert((
                Node {
                    position_type: PositionType::Absolute,
                    top: Val::Px(0.0),
                    left: Val::Px(0.0),
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor::from(Color::linear_rgba(0.0, 0.0, 0.0, 0.7)),
                Visibility::Hidden,
            ))
            .with_children(|parent| {
                parent.spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(10.0),
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    children![
                        LoadMapButton,
                        MapListContainer::default(),
                    ]
                ));
            });
    }
}

#[derive(Component)]
#[require(Button)]
struct LoadMapButton;
impl LoadMapButton {
    fn on_add(trigger: On<Add, LoadMapButton>, mut commands: Commands) {
        let entity = trigger.entity;
        commands.entity(entity)
            .insert((
                Node {
                    width: Val::Px(220.0),
                    height: Val::Px(40.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor::from(Color::linear_rgba(0.2, 0.2, 0.8, 1.0)),
            ))
            .with_children(|parent| {
                parent.spawn((
                    Text::new("Load Map"),
                    TextLayout::new_with_linebreak(LineBreak::NoWrap),
                ));
            })
            .observe(Self::on_click);
    }

    fn on_click(
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

#[derive(Component, Default)]
#[require(Node)]
struct MapListContainer;
impl MapListContainer {
    fn on_add(trigger: On<Add, MapListContainer>, mut commands: Commands) {
        let entity = trigger.entity;
        commands.entity(entity).insert((
            Node {
                display: Display::None,
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Stretch,
                row_gap: Val::Px(6.0),
                margin: UiRect { top: Val::Px(12.0), ..default() },
                ..default()
            },
        ));
    }
}

#[derive(Component)]
#[require(Button)]
struct MapEntryButton { name: String }
impl MapEntryButton {
    fn on_add(trigger: On<Add, MapEntryButton>, mut commands: Commands, entries: Query<&MapEntryButton>) {
        let entity = trigger.entity;
        let name = &entries.get(entity).unwrap().name;

        commands.entity(entity)
            .insert((
                Node {
                    width: Val::Px(260.0),
                    height: Val::Px(34.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor::from(Color::linear_rgba(0.3, 0.3, 0.3, 1.0)),
            ))
            .with_children(|parent| {
                parent.spawn((
                    Text::new(name.clone()),
                    TextLayout::new_with_linebreak(LineBreak::NoWrap),
                ));
            })
            .observe(Self::on_click);
    }

    fn on_click(trigger: On<Pointer<Click>>, mut commands: Commands, entries: Query<&MapEntryButton>) {
        let entity = trigger.entity;
        let Ok(entry) = entries.get(entity) else { return; };
        Log::debug().dev().tag(Tag::Ui).message(format!("Map selected: {}", entry.name));
        commands.trigger(LoadGameSignal(LoadMapConfig::new(format!("maps/{}.dwd", entry.name))));
    }
}

fn on_menu_enter_system(
    menu: Single<&mut Visibility, With<MainMenuRoot>>,
    mut next_game_state: ResMut<NextState<GameState>>,
) {
    *menu.into_inner() = Visibility::Inherited;
    next_game_state.set(GameState::Paused);
}

fn on_menu_exit_system(
    menu: Single<&mut Visibility, With<MainMenuRoot>>,
    current_game_state: Res<State<GameState>>,
    mut next_game_state: ResMut<NextState<GameState>>,
) {
    *menu.into_inner() = Visibility::Hidden;
    if matches!(current_game_state.get(), GameState::Paused) {
        next_game_state.set(GameState::Running);
    }
}
