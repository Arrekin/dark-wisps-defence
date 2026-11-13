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
use grids::placement::PlacementSession;
use logging::prelude::*;
use map_objects::{prelude::VolumeShape, wall_style::{WallStyleKey, WallStyles}};
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
    /// [`WallStyleKey::gpu_index`] of the drawn style.
    #[uniform(0)]
    style_index: u32,
}
impl UiMaterial for WallSwatchMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/walls/face.wesl".into()
    }

    specialize_ui_with_shared_gpu!();
}

/// World-space material for one isolated wall cell. Field order and types mirror `WallQuad` in
/// `assets/shaders/walls/quad.wesl`.
#[derive(Asset, AsBindGroup, TypePath, Clone, Copy, Debug)]
pub(crate) struct WallQuadMaterial {
    /// [`WallStyleKey::gpu_index`] of the drawn style.
    #[uniform(0)]
    style_index: u32,
    #[uniform(0)]
    alpha: f32,
    /// Shape encoded by [`VolumeShape::gpu_code`].
    #[uniform(0)]
    shape: u32,
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
/// UI swatches use the target's `WallStyleKey`, falling back to the default style if it is absent.
/// Placement previews use `WallStyleKey` and `VolumeShape` from the placer's session entity.
#[log_tags(Tag::MapObjects)]
fn on_object_face_request_draw_wall(
    trigger: On<ObjectFaceRequest>,
    mut commands: Commands,
    styles: Res<WallStyles>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut swatch_materials: ResMut<Assets<WallSwatchMaterial>>,
    mut quad_materials: ResMut<Assets<WallQuadMaterial>>,
    placement_session: Option<Single<(&WallStyleKey, &VolumeShape), With<PlacementSession>>>,
    grid_imprints: Query<&GridImprint>,
    style_keys: Query<&WallStyleKey>,
) {
    let ObjectFaceRequest { entity, object: MapObject::Wall, surface, alpha } = *trigger else { return; };

    match surface {
        FaceSurface::Ui => {
            let key = style_keys.get(entity).copied().unwrap_or_default();
            #[warn_dev("Wall face requested for {entity} with style {}, which is not in this map's table", key.0)]
            if !styles.contains(key) { return; }
            commands.entity(entity).insert(MaterialNode(swatch_materials.add(WallSwatchMaterial { style_index: key.gpu_index() })));
        }
        FaceSurface::World => {
            #[warn_dev("Wall face requested in the world for {entity} outside a wall placement session")]
            let Some(placement_session) = placement_session else { return; };
            let (&key, &shape) = placement_session.into_inner();
            #[warn_dev("Wall face requested for {entity} with style {}, which is not in this map's table", key.0)]
            if !styles.contains(key) { return; }
            #[warn_dev("Wall face requested in the world for {entity}, which has no GridImprint or no longer exists")]
            let Ok(grid_imprint) = grid_imprints.get(entity) else { return; };

            commands.entity(entity).insert((
                Mesh2d(meshes.add(Rectangle::from_size(grid_imprint.world_size()))),
                MeshMaterial2d(quad_materials.add(WallQuadMaterial { style_index: key.gpu_index(), alpha, shape: shape.gpu_code() })),
            ));
        }
    }
}
