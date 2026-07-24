use bevy::prelude::*;

use game_core::prelude::{Moment, MomentKind};
use persistence::prelude::*;
use states::prelude::GameState;

pub struct SessionMomentsPlugin;
impl Plugin for SessionMomentsPlugin {
    fn build(&self, app: &mut App) {
        app
            .register_moment_persistence::<MomentGameStart>()
            .add_systems(OnEnter(GameState::Running), fire_start_game_once)
            ;
    }
}

/// The moment the game enters `Running` — the start of a play session.
/// Self-parented standalone moment.
#[derive(Component, Default, MomentKind)]
#[require(Moment, Name = Name::new("Game Start"))]
pub struct MomentGameStart;

/// Fire the game-start moment exactly once, when the game enters `Running`.
/// On mid-game reload, `fired_count >= 1` (restored from save) → no re-fire.
fn fire_start_game_once(
    mut commands: Commands,
    moment: Single<(Entity, &mut Moment), With<MomentGameStart>>,
) {
    let (entity, mut moment) = moment.into_inner();
    moment.fire_if_not_yet_fired(&mut commands.entity(entity));
}
