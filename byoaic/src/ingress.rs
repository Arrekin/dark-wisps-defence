use bevy::{ecs::world::CommandQueue, prelude::*};

#[derive(Resource)]
pub(crate) struct ByoaicIngress {
    pub(crate) receiver: async_channel::Receiver<CommandQueue>,
}

/// Runs in every `GameState` so the game stays controllable while paused, loading or in menus.
pub(crate) fn apply_ingress_queues(mut commands: Commands, ingress: Res<ByoaicIngress>) {
    while let Ok(mut queue) = ingress.receiver.try_recv() {
        commands.append(&mut queue);
    }
}
