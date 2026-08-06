use bevy::prelude::*;

use widgets::prelude::{BuilderHealthbar, FillBar, FillBarChildren, Healthbar};

pub struct HealthbarPlugin;
impl Plugin for HealthbarPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_observer(on_builder_add_spawn_healthbar)
            .add_systems(Update, sync_healthbar_display);
    }
}

/// Holds the entities of the nodes `Healthbar` spawns, recorded at spawn
/// time so they can be reached by direct lookup.
#[derive(Component)]
struct HealthbarChildren {
    fill_bar: Entity,
    value_text: Entity,
}

fn on_builder_add_spawn_healthbar(
    trigger: On<Add, BuilderHealthbar>,
    mut commands: Commands,
    builders: Query<&BuilderHealthbar>,
) {
    let entity = trigger.entity;
    let Ok(builder) = builders.get(entity) else { return };

    let mut children_ref = HealthbarChildren {
        fill_bar: Entity::PLACEHOLDER,
        value_text: Entity::PLACEHOLDER,
    };
    commands.entity(entity)
        .remove::<BuilderHealthbar>()
        .insert((
            Node {
                width: Val::Percent(100.),
                height: Val::Percent(100.),
                ..default()
            },
            builder.healthbar,
        ))
        .with_children(|parent| {
            // FillBar child — the track + fill. Seed the fill colour from
            // `healthbar.color` so there is a single colour owner.
            children_ref.fill_bar = parent.spawn(
                builder.builder_fill_bar.with_fill_color(builder.healthbar.color),
            ).id();
            // Centred text overlay — absolute positioning is needed because
            // no combination of flex_direction, justify_content and
            // align_items centres the text reliably.
            parent.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Percent(100.),
                    height: Val::Percent(100.),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
            )).with_children(|overlay| {
                children_ref.value_text = overlay.spawn((
                    Text::default(),
                    TextFont::default().with_font_size(builder.font_size),
                    TextColor::BLACK,
                    TextLayout::no_wrap(),
                )).id();
            });
        })
        .insert(children_ref);
}

fn sync_healthbar_display(
    healthbars: Query<(&Healthbar, &HealthbarChildren), Changed<Healthbar>>,
    mut fill_bars: Query<(&mut FillBar, &FillBarChildren)>,
    mut fill_colors: Query<&mut BackgroundColor>,
    mut texts: Query<&mut Text>,
) {
    for (healthbar, children) in healthbars.iter() {
        // Write fraction and colour into the FillBar child
        let Ok((mut fill_bar, fill_bar_children)) = fill_bars.get_mut(children.fill_bar) else { continue };
        fill_bar.fill_fraction = healthbar.get_fraction();
        let Ok(mut fill_color) = fill_colors.get_mut(fill_bar_children.fill) else { continue };
        fill_color.0 = healthbar.color;
        // Update text
        let Ok(mut text) = texts.get_mut(children.value_text) else { continue };
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
}
