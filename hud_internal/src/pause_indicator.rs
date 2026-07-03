use bevy::prelude::*;
use bevy::ui::FocusPolicy;

use states::prelude::*;

pub struct PauseIndicatorPlugin;
impl Plugin for PauseIndicatorPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Startup, PauseIndicator::setup)
            .add_systems(Update, PauseIndicator::update_visibility.run_if(state_changed::<GameState>));
    }
}

#[derive(Component, Default, Clone)]
struct PauseIndicator;

impl PauseIndicator {
    fn setup(mut commands: Commands) {
        commands.spawn_scene(bsn! {
            PauseIndicator
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(0.0),
                left: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                border: UiRect::all(Val::Px(4.0)),
            }
            BackgroundColor(Color::NONE)
            template_value(BorderColor::all(Color::srgb(1.0, 0.8, 0.0))) // Yellow border
            template_value(FocusPolicy::Pass)
            template_value(Pickable::IGNORE) // Don't block mouse clicks/events
            Visibility::Hidden // Initially hidden
            ZIndex(-1) // Render behind other UI elements
        });
    }

    fn update_visibility(
        game_state: Res<State<GameState>>,
        mut pause_indicator: Single<&mut Visibility, With<PauseIndicator>>,
    ) {
        let is_paused = matches!(game_state.get(), GameState::Paused);
        **pause_indicator = if is_paused {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}
