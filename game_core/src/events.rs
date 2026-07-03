use bevy::prelude::*;

// Event that carries string-identified or constant game events
#[derive(Event)]
pub struct DynamicGameEvent(pub String);
impl DynamicGameEvent {
    pub fn game_started() -> Self { DynamicGameEvent("game-started".to_string()) }
}

#[derive(Message, Clone, Copy, Debug)]
pub struct DamageMessage {
    pub target: Entity,
    pub amount: f32,
}
