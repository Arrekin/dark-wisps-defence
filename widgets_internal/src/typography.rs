use bevy::asset::{AssetServer, Handle};
use bevy::ecs::resource::Resource;
use bevy::prelude::*;
use bevy::text::Font;

pub struct TypographyPlugin;
impl Plugin for TypographyPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Startup, load_fonts)
            .init_resource::<FontHandles>();
    }
}

/// Keeps handles alive for every bundled font.
///
/// Dropping the last `Font` handle unregisters its family from Parley's collection, while
/// [`TextRole`](widgets::prelude::TextRole) resolves fonts by family name during layout.
#[derive(Resource, Deref, Default)]
pub(crate) struct FontHandles(Vec<Handle<Font>>);

fn load_fonts(mut commands: Commands, asset_server: Res<AssetServer>) {
    let handles = vec![
        asset_server.load("fonts/Inter-VariableFont_opsz,wght.ttf"),
        asset_server.load("fonts/Inter-Italic-VariableFont_opsz,wght.ttf"),
        asset_server.load("fonts/JetBrainsMono-VariableFont_wght.ttf"),
        asset_server.load("fonts/JetBrainsMono-Italic-VariableFont_wght.ttf"),
        asset_server.load("fonts/SpaceGrotesk-VariableFont_wght.ttf"),
    ];
    commands.insert_resource(FontHandles(handles));
}
