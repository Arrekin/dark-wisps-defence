use crate::prelude::*;


// Determines which part of the sprite sheet to use
#[derive(Component)]
pub struct AnimationController {
    pub atlas_first_frame: usize,
    pub atlas_last_frame: usize,
    pub timer: Timer,
    pub repeating: bool,
    pub has_finished: bool, // Only if repeating is false
}
impl AnimationController {
    pub fn new(atlas_first_frame: usize, atlas_last_frame: usize, duration: f32, repeating: bool) -> Self {
        Self {
            atlas_first_frame,
            atlas_last_frame,
            timer: Timer::from_seconds(duration, TimerMode::Repeating),
            repeating,
            has_finished: false,
        }
    }
}

pub fn animate_sprite_system(
    time: Res<Time>,
    mut animations: Query<(&mut AnimationController, &mut Sprite)>,
) {
    for (mut controller, mut sprite) in &mut animations {
        let Some(atlas) = &mut sprite.texture_atlas else { continue; };
        controller.timer.tick(time.delta());
        if controller.timer.just_finished() {
            atlas.index = if atlas.index == controller.atlas_last_frame {
                if controller.repeating {
                    controller.atlas_first_frame
                } else {
                    controller.has_finished = true;
                    controller.atlas_last_frame
                }
            } else {
                atlas.index + 1
            };
        }
    }
}