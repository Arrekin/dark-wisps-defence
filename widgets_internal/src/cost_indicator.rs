use bevy::color::palettes::css::{BLUE, GREEN, RED};
use bevy::prelude::*;
use resources::prelude::Stock;
use widgets::prelude::CostIndicator;

pub struct CostIndicatorPlugin;
impl Plugin for CostIndicatorPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Update, (
                sync_cost_indicator_display,
                calculate_cost_indicator_has_required_resources_system.run_if(resource_changed::<Stock>),
            ))
            .add_observer(on_add_spawn_cost_indicator);
    }
}

#[derive(Component)]
struct CostIndicatorChildren {
    border_rectangle: Entity,
    icon: Entity,
    value_text: Entity,
}
#[derive(Component)]
struct CostIndicatorIcon;
#[derive(Component)]
struct CostIndicatorValueText;
#[derive(Component)]
struct CostIndicatorBorderRectangle;

fn on_add_spawn_cost_indicator(
    trigger: On<Add, CostIndicator>,
    mut commands: Commands,
    stock: Res<Stock>,
    mut cost_indicators: Query<&mut CostIndicator>,
) {
    let cost_indicator_entity = trigger.entity;
    let Ok(mut cost_indicator) = cost_indicators.get_mut(cost_indicator_entity) else { return; };
    // Update the cost indicator state
    cost_indicator.has_required_resources = stock.can_cover(&cost_indicator.cost);
    // Spawn the full cost indicator structure
    let mut cost_indicator_children = CostIndicatorChildren {
        border_rectangle: Entity::PLACEHOLDER,
        icon: Entity::PLACEHOLDER,
        value_text: Entity::PLACEHOLDER,
    };
    commands.entity(cost_indicator_entity).with_children(|parent| {
        cost_indicator_children.border_rectangle = parent.spawn((
            Node {
                width: Val::Px(32.),
                height: Val::Px(32.),
                border: UiRect::all(Val::Px(2.0)),
                ..default()
            },
            BackgroundColor::from(Color::linear_rgba(0., 0., 0., 0.)),
            BorderColor::from(BLUE),
            CostIndicatorBorderRectangle,
        )).id();
        cost_indicator_children.icon = parent.spawn((
            ImageNode::default(),
            Node {
                width: Val::Px(16.),
                height: Val::Px(16.),
                ..default()
            },
            CostIndicatorIcon,
        )).id();
        parent.spawn((
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(2.0),
                left: Val::Px(2.0),
                ..default()
            },
        )).with_children(|parent| {
            cost_indicator_children.value_text = parent.spawn((
                Text::default(),
                TextFont::default().with_font_size(cost_indicator.font_size),
                TextColor::from(cost_indicator.font_color),
                TextLayout::no_wrap(),
                CostIndicatorValueText,
            )).id();
        });
    }).insert(cost_indicator_children);
}

fn sync_cost_indicator_display(
    cost_indicators: Query<(&CostIndicator, &CostIndicatorChildren), Changed<CostIndicator>>,
    mut texts: Query<&mut Text, With<CostIndicatorValueText>>,
    mut border_rectangles: Query<&mut BorderColor, With<CostIndicatorBorderRectangle>>,
) -> Result<()> {
    for (cost_indicator, children) in cost_indicators.iter() {
        let mut text = texts.get_mut(children.value_text)?;
        text.0 = format!("{}", cost_indicator.cost.amount);

        let mut border_color = border_rectangles.get_mut(children.border_rectangle)?;
        *border_color = BorderColor::all(if cost_indicator.has_required_resources{ GREEN } else { RED });
    }
    Ok(())
}

fn calculate_cost_indicator_has_required_resources_system(
    stock: Res<Stock>,
    mut cost_indicators: Query<&mut CostIndicator>,
) {
    for mut cost_indicator in cost_indicators.iter_mut() {
        cost_indicator.has_required_resources = stock.can_cover(&cost_indicator.cost);
    }
}
