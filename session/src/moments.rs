use bevy::prelude::*;

use game_core::prelude::{Moment, MomentKind, MomentOf};
use persistence::{creating_new_map, prelude::*};
use states::prelude::{GameState, MapLoadingStage};

pub struct SessionMomentsPlugin;
impl Plugin for SessionMomentsPlugin {
    fn build(&self, app: &mut App) {
        app
            .register_moment_persistence::<MomentGameStart>()
            .add_systems(OnEnter(GameState::Running), fire_start_game_once)
            .add_systems(OnEnter(MapLoadingStage::SpawnMapElements), seed_moment_game_start.run_if(creating_new_map))
            ;
    }
}

/// The moment that starts a play session when the game enters `Running`.
/// Its `MomentOf` parent is itself, so it is a standalone moment.
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

/// Spawn the self-parented `MomentGameStart` on a new map. Two-step because the
/// entity needs to reference itself in `MomentOf`.
fn seed_moment_game_start(mut commands: Commands) {
    let entity = commands.spawn_empty().id();
    commands.entity(entity).insert((MomentOf(entity), MomentGameStart));
}
