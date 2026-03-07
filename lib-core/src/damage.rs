use crate::lib_prelude::*;

pub mod damage_prelude {
    pub use super::DamageMessage;
}

pub struct DamagePlugin;
impl Plugin for DamagePlugin {
    fn build(&self, app: &mut App) {
        app
            .add_message::<DamageMessage>()
            .add_systems(PostUpdate, (
                DamageMessage::process.run_if(on_message::<DamageMessage>),
            ))
            ;
    }
}

#[derive(Message, Clone, Copy, Debug)]
pub struct DamageMessage {
    pub target: Entity,
    pub amount: f32,
}
impl DamageMessage {
    fn process(
        mut reader: MessageReader<Self>,
        mut targets: Query<(&mut IntegrityPoints, Option<&IncomingDamageMultiplier>)>,
    ) {
        for message in reader.read() {
            if message.amount <= 0.0 { continue; }
            let Ok((mut integrity_points, incoming_damage_multiplier)) = targets.get_mut(message.target) else { continue; };
            let incoming_damage_multiplier = incoming_damage_multiplier.map_or(1.0, |multiplier| multiplier.get());
            integrity_points.decrease(message.amount * incoming_damage_multiplier);
        }
    }
}
