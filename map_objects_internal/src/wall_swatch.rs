//! Wall-cell UI material shared by the style picker and side-menu face.

use bevy::{
    prelude::*,
    reflect::TypePath,
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
    ui_render::ui_material::UiMaterial,
};

use game_core::prelude::CELL_SIZE;
use map_objects::prelude::BuilderWallFace;
use map_objects::wall_style::{WallStyle, WallStyleKey, WallStyles};

pub(crate) struct WallSwatchPlugin;
impl Plugin for WallSwatchPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins(UiMaterialPlugin::<WallSwatchMaterial>::default())
            .add_observer(WallSwatch::on_add_construct_wall_swatch)
            .add_observer(on_builder_add_spawn_wall_face);
    }
}

#[derive(Asset, AsBindGroup, TypePath, Clone, Copy, Debug)]
pub(crate) struct WallSwatchMaterial {
    #[uniform(0)]
    style: WallStyle,
}
impl UiMaterial for WallSwatchMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/wall_swatch.wgsl".into()
    }
}

/// Draws one style as a single wall cell, at the size that cell has on the map.
#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct WallSwatch(pub WallStyleKey);
impl WallSwatch {
    /// World pixels per cell. Matches `CELL_SIZE` in assets/shaders/wall_swatch.wgsl.
    const SIZE: f32 = CELL_SIZE;

    fn on_add_construct_wall_swatch(
        trigger: On<Add, WallSwatch>,
        mut commands: Commands,
        swatches: Query<&WallSwatch>,
        styles: Res<WallStyles>,
        mut materials: ResMut<Assets<WallSwatchMaterial>>,
    ) {
        let entity = trigger.entity;
        let Ok(swatch) = swatches.get(entity) else { return; };
        let Some(entry) = styles.entries.get(swatch.0.0 as usize) else { return; };

        commands.entity(entity).insert((
            Node {
                width: Val::Px(Self::SIZE),
                height: Val::Px(Self::SIZE),
                ..default()
            },
            MaterialNode(materials.add(WallSwatchMaterial { style: entry.style })),
        ));
    }
}

fn on_builder_add_spawn_wall_face(
    trigger: On<Add, BuilderWallFace>,
    mut commands: Commands,
    styles: Res<WallStyles>,
    mut materials: ResMut<Assets<WallSwatchMaterial>>,
) {
    let Some(entry) = styles.entries.first() else { return; };

    commands.entity(trigger.entity)
        .remove::<BuilderWallFace>()
        .insert(MaterialNode(materials.add(WallSwatchMaterial { style: entry.style })));
}
