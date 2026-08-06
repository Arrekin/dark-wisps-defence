use bevy::prelude::*;

use states::prelude::UiInteraction;


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

/// Overwrite a `Text` only when the content actually differs.
///
/// Any mutable access to `Text` marks it changed and forces Bevy to re-measure
/// and re-lay-out the node, so a write of an identical string is not free. The
/// comparison reads through `Deref`, which does not mark anything; only the
/// rewrite takes `DerefMut`. Clearing and pushing reuses the existing
/// allocation rather than replacing the `String`.
pub fn set_text_if_changed(text: &mut Mut<Text>, content: &str) {
    if text.0 == content { return }

    text.0.clear();
    text.0.push_str(content);
}

/// Set `UiInteraction` back to `Free` on the given trigger event.
/// Example use: `.observe(set_ui_free_on::<Pointer<Click>>)`
pub fn set_ui_free_on<E: Event>(
    _trigger: On<E>,
    mut next_ui_state: ResMut<NextState<UiInteraction>>,
) {
    next_ui_state.set(UiInteraction::Free);
}
