use bevy::prelude::*;


/// Recolor BackgroundColor with the given color on the specifed trigger.
/// Example use: `.observe(recolor_background_on::<Pointer<Out>>(Color::NONE))`
pub fn recolor_background_on<E: EntityEvent>(color: Color) -> impl Fn(On<E>, Query<&mut BackgroundColor>) {
    move |event, mut background_colors| {
        let Ok(mut background_color) = background_colors.get_mut(event.event_target()) else {
            return;
        };
        background_color.0 = color;
    }
}
