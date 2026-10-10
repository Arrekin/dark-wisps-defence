use bevy::prelude::*;

use widgets::prelude::{BuilderHealthbar, FillBar, FillBarChildren, Healthbar};

pub(crate) struct HealthbarPlugin;
impl Plugin for HealthbarPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_observer(on_builder_add_spawn_healthbar)
            .add_systems(Update, sync_healthbar_display);
    }
}

/// Holds the entities of the nodes `Healthbar` spawns, recorded at spawn
/// time so they can be reached by direct lookup.
#[derive(Component, FromTemplate)]
struct HealthbarChildren {
    fill_bar: Entity,
    value_text: Entity,
}

fn on_builder_add_spawn_healthbar(
    trigger: On<Add<BuilderHealthbar>>,
    mut commands: Commands,
    builders: Query<&BuilderHealthbar>,
) {
    let entity = trigger.entity;
    let Ok(builder) = builders.get(entity) else { return };

    let BuilderHealthbar { healthbar, builder_fill_bar, font_size } = builder.clone();
    // Seed the fill colour from `healthbar.color` so there is a single colour owner.
    let fill_bar = builder_fill_bar.with_fill_color(healthbar.color);
    commands.entity(entity)
        .remove::<BuilderHealthbar>()
        .apply_scene(bsn! {
            Node { width: Val::Percent(100.), height: Val::Percent(100.) }
            ~{healthbar}
            HealthbarChildren { fill_bar: #FillBar, value_text: #ValueText }
            Children [
                #FillBar
                ~{fill_bar}
                --
                // Centred text overlay — absolute positioning is needed because
                // no combination of flex_direction, justify_content and
                // align_items centres the text reliably.
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Percent(100.),
                    height: Val::Percent(100.),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                }
                Children [
                    #ValueText
                    Text
                    TextFont { font_size: FontSize::Px(font_size) }
                    TextColor(Color::BLACK)
                    TextLayout::no_wrap()
                ]
            ]
        });
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
        let format_value = |value: f32| {
            if value.fract() == 0.0 {
                format!("{value:.0}")
            } else if (value * 10.0).fract() == 0.0 {
                format!("{value:.1}")
            } else {
                format!("{value:.2}")
            }
        };
        text.0 = format!("{} / {}", format_value(healthbar.value), format_value(healthbar.max_value));
    }
}
