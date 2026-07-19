use bevy::prelude::*;

use visuals::prelude::ColorPulsation;

pub(crate) fn pulsate_sprites_system(
    time: Res<Time>,
    mut sprites: Query<(&mut Sprite, &mut ColorPulsation)>,
) {
    for (mut sprite, mut color_pulsation) in sprites.iter_mut() {
        let delta_time = time.delta_secs();
        if let Color::Hsla(Hsla {lightness, .. }) = &mut sprite.color {
            *lightness = color_pulsation.advance(*lightness, delta_time);
        }
    }
}

pub(crate) fn on_remove_color_pulsation_reset_sprite_lightness(
    trigger: On<Remove, ColorPulsation>,
    mut sprites: Query<&mut Sprite>,
) {
    let entity = trigger.entity;
    let Ok(mut sprite) = sprites.get_mut(entity) else { return; };
    if let Color::Hsla(Hsla {lightness, .. }) = &mut sprite.color {
        *lightness = 1.0;
    }
}
