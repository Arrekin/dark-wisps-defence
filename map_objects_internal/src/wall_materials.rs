//! Materials for wall UI faces and world-space placement previews. Both read their style from the
//! shared wall style table by index.

use bevy::{
    prelude::*,
    reflect::TypePath,
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
    sprite_render::{AlphaMode2d, Material2d, Material2dPlugin, MeshMaterial2d},
    ui_render::ui_material::UiMaterial,
};

use game_core::prelude::{FaceSurface, GridImprint, MapObject, ObjectFaceRequest};
use grids::placement::PlacementStyle;
use logging::prelude::*;
use map_objects::wall_style::{WallStyleKey, WallStyles};
use visuals::{specialize_ui_with_shared_gpu, specialize_with_shared_gpu};

pub(crate) struct WallMaterialsPlugin;
impl Plugin for WallMaterialsPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins(UiMaterialPlugin::<WallSwatchMaterial>::default())
            .add_plugins(Material2dPlugin::<WallQuadMaterial>::default())
            .add_observer(on_object_face_request_draw_wall);
    }
}

#[derive(Asset, AsBindGroup, TypePath, Clone, Copy, Debug)]
pub(crate) struct WallSwatchMaterial {
    /// [`WallStyleKey::shader_index`] of the drawn style.
    #[uniform(0)]
    style_index: u32,
}
impl UiMaterial for WallSwatchMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/walls/face.wesl".into()
    }

    specialize_ui_with_shared_gpu!();
}

/// World-space material for one isolated wall cell.
#[derive(Asset, AsBindGroup, TypePath, Clone, Copy, Debug)]
pub(crate) struct WallQuadMaterial {
    /// [`WallStyleKey::shader_index`] of the drawn style.
    #[uniform(0)]
    style_index: u32,
    #[uniform(0)]
    alpha: f32,
}
impl Material2d for WallQuadMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/walls/quad.wesl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }

    specialize_with_shared_gpu!();
}

/// Attaches a wall face for UI or placement preview use.
///
/// Placed walls are rendered separately by the grid-wide wall canvas.
#[log_tags(Tag::MapObjects)]
fn on_object_face_request_draw_wall(
    trigger: On<ObjectFaceRequest>,
    mut commands: Commands,
    styles: Res<WallStyles>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut swatch_materials: ResMut<Assets<WallSwatchMaterial>>,
    mut quad_materials: ResMut<Assets<WallQuadMaterial>>,
    grid_imprints: Query<&GridImprint>,
    placement_styles: Query<&PlacementStyle>,
) {
    let ObjectFaceRequest { entity, object: MapObject::Wall, surface, alpha } = *trigger else { return; };

    // Targets may select a wall style through `PlacementStyle`; otherwise use the default.
    let key = placement_styles.get(entity).map_or_else(|_| WallStyleKey::default(), |style| (*style).into());
    #[warn_dev("Wall face requested for {entity} with style {}, which is not in this map's table", key.0)]
    if !styles.contains(key) { return; }
    let style_index = key.shader_index();

    match surface {
        FaceSurface::Ui => {
            commands.entity(entity).insert(MaterialNode(swatch_materials.add(WallSwatchMaterial { style_index })));
        }
        FaceSurface::World => {
            #[warn_dev("Wall face requested in the world for {entity}, which has no GridImprint or no longer exists")]
            let Ok(grid_imprint) = grid_imprints.get(entity) else { return; };
            commands.entity(entity).insert((
                Mesh2d(meshes.add(Rectangle::from_size(grid_imprint.world_size()))),
                MeshMaterial2d(quad_materials.add(WallQuadMaterial { style_index, alpha })),
            ));
        }
    }
}
