use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use strum::IntoEnumIterator;

use resources::prelude::*;
use states::prelude::GameState;
use widgets::common::utils::recolor_background_on;

pub struct BadgesPlugin;
impl Plugin for BadgesPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Startup, initialize_badges_system)
            .add_systems(Update, (
                ResourceBadgeText::sync_text_with_stock.run_if(resource_changed::<Stock>),
                EssencesContainer::manage_essence_badges_visibility.run_if(on_message::<StockChangedEvent>),
            ))
            .add_systems(OnEnter(GameState::Loading), EssencesContainer::hide_all_essence_badges)
            ;
    }
}

#[derive(Component)]
pub(crate) struct ResourceBadgeText(ResourceType);
impl ResourceBadgeText {
    fn sync_text_with_stock(    
        stock: Res<Stock>,
        mut resource_text: Query<(&mut Text, &ResourceBadgeText)>,
    ) {
        for (mut text, badge) in resource_text.iter_mut() {
            text.0 = stock.get(badge.0).to_string();
        }
    }
}

#[derive(Component, Default)]
pub(crate) struct EssencesContainer {
    badges: HashMap<EssenceType, Entity>,
}
impl EssencesContainer {
    fn manage_essence_badges_visibility(
        mut event_reader: MessageReader<StockChangedEvent>,
        mut nodes: Query<&mut Node, With<EssenceBadge>>,
        essences_container: Single<&EssencesContainer>,
    ) {
        let essences_container = essences_container.into_inner();
        for event in event_reader.read() {
            let ResourceType::Essence(essence_type) = event.resource_type else { continue; };
            let essence_badge_entity = *essences_container.badges.get(&essence_type).expect("Essence badge entity not found");    
            let Ok(mut node) = nodes.get_mut(essence_badge_entity) else { return; };
            if event.new_amount > 0 {
                node.display = Display::Flex;
            } else {
                node.display = Display::None;
            }
        }
    }
    fn hide_all_essence_badges(
        mut nodes: Query<&mut Node, With<EssenceBadge>>,
        essences_container: Single<&EssencesContainer>,
    ) {
        let essences_container = essences_container.into_inner();
        for essence_badge_entity in essences_container.badges.values() {
            let Ok(mut node) = nodes.get_mut(*essence_badge_entity) else { continue; };
            node.display = Display::None;
        }

    }
}
#[derive(Component)]
pub(crate) struct EssenceBadge;

fn initialize_badges_system(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
) {
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(5.0),
            left: Val::Px(5.0),
            flex_direction: FlexDirection::Column,
            ..default()
        },
    )).with_children(|parent| {
        let _dare_ore_badge = parent.spawn((
            Node {
                width: Val::Px(101.),
                height: Val::Px(115.),
                ..default()
            },
            ImageNode::new(asset_server.load("ui/dark_ore_badge.png")),
        )).with_children(|parent| {
            parent.spawn((
                Text::new("####"),
                TextFont::default().with_font_size(12.),
                Node {
                    top: Val::Px(83.),
                    left: Val::Px(39.),
                    ..default()
                },
                Label,
                ResourceBadgeText(ResourceType::DarkOre),
            ));
        });
        let mut essence_badges_map = EssencesContainer::default();
        let _essences_container = parent.spawn((
            Node {
                width: Val::Px(101.),
                flex_direction: FlexDirection::Column,
                border: UiRect::all(Val::Px(1.)),
                border_radius: BorderRadius::all(Val::Px(2.)),
                ..default()
            },
            BackgroundColor::from(Color::linear_rgba(0., 0.3, 0.7, 0.1)),
            BorderColor::from(Color::linear_rgba(0., 0.3, 0.9, 0.3)),
        )).with_children(|parent| {
            for essence_type in EssenceType::iter() {
                let essence_badge = parent
                    .spawn((
                        Node {
                            display: Display::None,
                            width: Val::Percent(100.),
                            height: Val::Px(20.),
                            left: Val::Px(2.),
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        EssenceBadge,
                    ))
                    .observe(recolor_background_on::<Pointer<Over>>(Color::linear_rgba(0., 0.3, 0.9, 0.3)))
                    .observe(recolor_background_on::<Pointer<Out>>(Color::NONE))
                    .with_children(|parent| {
                        // Essence circle
                        let (background_color, border_color) = essence_type_to_badge_colors(essence_type);
                        parent.spawn((
                            Node {
                                width: Val::Px(10.),
                                height: Val::Px(10.),
                                border_radius: BorderRadius::all(Val::Px(25.)),
                                ..default()
                            },
                            background_color,
                            border_color,
                        ));
                        // Essence amount
                        parent.spawn((
                            Text::new("###"),
                            TextFont::default().with_font_size(12.),
                            Node {
                                left: Val::Px(2.),
                                ..default()
                            },
                            ResourceBadgeText(ResourceType::Essence(essence_type)),
                        ));
                    }).id();
                essence_badges_map.badges.insert(essence_type, essence_badge);
            }
        }).insert(essence_badges_map);
    });
}

fn essence_type_to_badge_colors(essence_type: EssenceType) -> (BackgroundColor, BorderColor) {
    match essence_type {
        EssenceType::Fire => (BackgroundColor::from(Color::linear_rgba(1., 0.05, 0.05, 0.8)), BorderColor::from(Color::linear_rgba(1., 0., 0., 0.9))),
        EssenceType::Water => (BackgroundColor::from(Color::linear_rgba(0.05, 0.05, 1., 0.8)), BorderColor::from(Color::linear_rgba(0., 0., 1., 0.9))),
        EssenceType::Light => (BackgroundColor::from(Color::linear_rgba(0.95, 0.95, 0.95, 0.8)), BorderColor::from(Color::linear_rgba(1., 1., 1., 0.9))),
        EssenceType::Electric => (BackgroundColor::from(Color::linear_rgba(1., 0.95, 0.05, 0.8)), BorderColor::from(Color::linear_rgba(1., 1., 0., 0.9))),
    }
}

