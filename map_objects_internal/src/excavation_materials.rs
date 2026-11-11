//! Materials for excavation icons and placement previews. Both use `single_cell_excavation` in
//! `assets/shaders/underground/look.wesl` to render a cell independently of the surrounding map.

use bevy::{
    prelude::*,
    reflect::TypePath,
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
    sprite_render::{AlphaMode2d, Material2d, Material2dPlugin, MeshMaterial2d},
    ui_render::ui_material::UiMaterial,
};

use game_core::prelude::{FaceSurface, GridImprint, MapObject, ObjectFaceRequest};
use indexes::placement::GridPlacerSession;
use logging::prelude::*;
use map_objects::prelude::ExcavationShape;
use visuals::{specialize_ui_with_shared_gpu, specialize_with_shared_gpu};

pub(crate) struct ExcavationMaterialsPlugin;
impl Plugin for ExcavationMaterialsPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins(UiMaterialPlugin::<ExcavationFaceMaterial>::default())
            .add_plugins(Material2dPlugin::<ExcavationQuadMaterial>::default())
            .add_observer(on_object_face_request_draw_excavation);
    }
}

/// Returns the shader's `(pit, shape)` fields: `(1, 0)` for a pit, otherwise `(0, gpu_code)`.
fn shape_codes(shape: ExcavationShape) -> (u32, u32) {
    match shape {
        ExcavationShape::Pit => (1, 0),
        ExcavationShape::Shaped(shape) => (0, shape.gpu_code()),
    }
}

/// Field order and types mirror `ExcavationFace` in `assets/shaders/underground/face.wesl`.
#[derive(Asset, AsBindGroup, TypePath, Clone, Copy, Debug)]
pub(crate) struct ExcavationFaceMaterial {
    #[uniform(0)]
    pit: u32,
    #[uniform(0)]
    shape: u32,
}
impl UiMaterial for ExcavationFaceMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/underground/face.wesl".into()
    }

    specialize_ui_with_shared_gpu!();
}

/// Field order and types mirror `ExcavationQuad` in `assets/shaders/underground/quad.wesl`.
#[derive(Asset, AsBindGroup, TypePath, Clone, Copy, Debug)]
pub(crate) struct ExcavationQuadMaterial {
    #[uniform(0)]
    alpha: f32,
    #[uniform(0)]
    pit: u32,
    #[uniform(0)]
    shape: u32,
}
impl Material2d for ExcavationQuadMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/underground/quad.wesl".into()
    }
    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }

    specialize_with_shared_gpu!();
}

/// Attaches an excavation face for UI or placement preview use.
///
/// UI faces show a pit. Placement previews use the `ExcavationShape` of the placer's session entity.
#[log_tags(Tag::MapObjects)]
fn on_object_face_request_draw_excavation(
    trigger: On<ObjectFaceRequest>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut face_materials: ResMut<Assets<ExcavationFaceMaterial>>,
    mut quad_materials: ResMut<Assets<ExcavationQuadMaterial>>,
    placer_session: Option<Single<&ExcavationShape, With<GridPlacerSession>>>,
    grid_imprints: Query<&GridImprint>,
) {
    let ObjectFaceRequest { entity, object: MapObject::Excavation, surface, alpha } = *trigger else { return; };

    match surface {
        FaceSurface::Ui => {
            let (pit, shape) = shape_codes(ExcavationShape::Pit);
            commands.entity(entity).insert(MaterialNode(face_materials.add(ExcavationFaceMaterial { pit, shape })));
        }
        FaceSurface::World => {
            #[warn_dev("Excavation face requested in the world for {entity} outside an excavation placement session")]
            let Some(placer_session) = placer_session else { return; };
            #[warn_dev("Excavation face requested in the world for {entity}, which has no GridImprint or no longer exists")]
            let Ok(grid_imprint) = grid_imprints.get(entity) else { return; };
            let (pit, shape) = shape_codes(*placer_session.into_inner());
            commands.entity(entity).insert((
                Mesh2d(meshes.add(Rectangle::from_size(grid_imprint.world_size()))),
                MeshMaterial2d(quad_materials.add(ExcavationQuadMaterial { alpha, pit, shape })),
            ));
        }
    }
}
