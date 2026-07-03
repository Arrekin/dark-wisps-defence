use bevy::prelude::*;
use widgets::prelude::Healthbar;

pub struct HealthbarPlugin;
impl Plugin for HealthbarPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Update, (
                sync_healthbar_display,
            ))
            .add_observer(on_add_spawn_healthbar);

    }
}

#[derive(Component)]
struct HealthbarChildren {
    value_rectangle: Entity,
    value_text: Entity,
}
#[derive(Component)]
struct HealthbarValueText;
#[derive(Component)]
struct HealthbarValueRectangle;

fn on_add_spawn_healthbar(
    trigger: On<Add, Healthbar>,
    mut commands: Commands,
    healthbars: Query<&Healthbar>,
) {
    let healthbar_entity = trigger.entity;
    let Ok(healthbar) = healthbars.get(healthbar_entity) else { return; };
    let mut healthbar_children = HealthbarChildren {
        value_rectangle: Entity::PLACEHOLDER,
        value_text: Entity::PLACEHOLDER,
    };
    commands.entity(healthbar_entity).with_children(|parent| {
        parent.spawn((
            // Bottom rectangle(background)
            Node {
                width: Val::Percent(100.),
                height: Val::Percent(100.),
                border: UiRect::all(Val::Px(2.0)),
                ..default()
            },
            BackgroundColor::from(Color::linear_rgba(0., 0., 0., 0.)),
            BorderColor::from(Color::linear_rgba(0., 0.2, 1., 1.)),
        )).with_children(|parent| {
            // Top rectangle(health)
            healthbar_children.value_rectangle = parent.spawn((
                Node {
                    width: Val::Percent(healthbar.get_percent()),
                    height: Val::Percent(100.),
                    ..default()
                },
                BackgroundColor::from(healthbar.color),
                HealthbarValueRectangle,
            )).id();
            // Current hp text
            parent.spawn((
                // This additional container is needed to center the text as no combination of flex_direction, justify_content and align_items work
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Percent(100.),
                    height: Val::Percent(100.),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
            )).with_children(|parent| {
                    healthbar_children.value_text = parent.spawn((
                        Text::default(),
                        TextFont::default().with_font_size(healthbar.font_size),
                        TextColor::BLACK,
                        TextLayout::no_wrap(),
                        HealthbarValueText,
                    )).id();
                });
            });
    }).insert(healthbar_children);
}

fn sync_healthbar_display(
    healthbars: Query<(&Healthbar, &HealthbarChildren), Changed<Healthbar>>,
    mut value_rectangles: Query<(&mut Node, &mut BackgroundColor), With<HealthbarValueRectangle>>,
    mut texts: Query<&mut Text, With<HealthbarValueText>>,
) -> Result<()> {
    for (healthbar, children) in healthbars.iter() {
        let (mut style, mut background_color) = value_rectangles.get_mut(children.value_rectangle)?;
        style.width = Val::Percent(healthbar.get_percent());
        background_color.0 = healthbar.color;
        let mut text = texts.get_mut(children.value_text)?;
        let format_value = |v: f32| {
            if v.fract() == 0.0 {
                format!("{:.0}", v)
            } else if (v * 10.0).fract() == 0.0 {
                format!("{:.1}", v)
            } else {
                format!("{:.2}", v)
            }
        };
        text.0 = format!("{} / {}", format_value(healthbar.value), format_value(healthbar.max_value));
    }
    Ok(())
}
