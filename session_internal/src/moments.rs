use bevy::prelude::*;

use game_core::prelude::{Moment, MomentOf};
use persistence::{creating_new_map, prelude::*};
use session::MomentGameStart;
use states::prelude::{GameState, MapLoadingStage};

pub(crate) struct SessionMomentsPlugin;
impl Plugin for SessionMomentsPlugin {
    fn build(&self, app: &mut App) {
        app
            .register_moment_persistence::<MomentGameStart>()
            .add_systems(OnEnter(GameState::Running), fire_moment_game_start)
            .add_systems(OnEnter(MapLoadingStage::SpawnMapElements), seed_moment_game_start.run_if(creating_new_map));
    }
}

/// Fires the game-start moment once, when the game enters `Running`. A reloaded map restores its
/// `fired_count`, so it does not fire again.
fn fire_moment_game_start(
    mut commands: Commands,
    moment: Single<(Entity, &mut Moment), With<MomentGameStart>>,
) {
    let (entity, mut moment) = moment.into_inner();
    moment.fire_if_not_yet_fired(&mut commands.entity(entity));
}

/// Spawns the self-parented `MomentGameStart` on a new map. It takes two steps because the entity
/// references itself in `MomentOf`.
fn seed_moment_game_start(mut commands: Commands) {
    let entity = commands.spawn_empty().id();
    commands.entity(entity).insert((MomentOf(entity), MomentGameStart));
}
