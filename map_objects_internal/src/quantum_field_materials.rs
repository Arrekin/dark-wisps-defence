//! Materials for quantum-field UI faces and world-space placement previews.
//!
//! Both reuse the boundary and moiré glow from `dwd::quantum_field`. Frame distortion remains in
//! the map post-process shader.

use bevy::{
    prelude::*,
    reflect::TypePath,
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
    sprite_render::{AlphaMode2d, Material2d, Material2dPlugin, MeshMaterial2d},
    ui_render::ui_material::UiMaterial,
};

use game_core::prelude::{FaceSurface, GridImprint, MapObject, ObjectFaceRequest};

pub(crate) struct QuantumFieldMaterialsPlugin;
impl Plugin for QuantumFieldMaterialsPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins(UiMaterialPlugin::<QuantumFieldFaceMaterial>::default())
            .add_plugins(Material2dPlugin::<QuantumFieldQuadMaterial>::default())
            .add_observer(on_object_face_request_draw_quantum_field);
    }
}

/// Stateless material; the shader derives its output from node UVs and global time.
#[derive(Asset, AsBindGroup, TypePath, Clone, Copy, Debug)]
pub(crate) struct QuantumFieldFaceMaterial {}
impl UiMaterial for QuantumFieldFaceMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/quantum_field/face.wgsl".into()
    }
}

/// The same glow in world space, spanning the imprint the field covers.
#[derive(Asset, AsBindGroup, TypePath, Clone, Copy, Debug)]
pub(crate) struct QuantumFieldQuadMaterial {
    #[uniform(0)]
    quad_size: Vec2,
    #[uniform(0)]
    alpha: f32,
}
impl Material2d for QuantumFieldQuadMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/quantum_field/quad.wgsl".into()
    }
    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

fn on_object_face_request_draw_quantum_field(
    trigger: On<ObjectFaceRequest>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut face_materials: ResMut<Assets<QuantumFieldFaceMaterial>>,
    mut quad_materials: ResMut<Assets<QuantumFieldQuadMaterial>>,
    grid_imprints: Query<&GridImprint>,
) {
    let ObjectFaceRequest { entity, object: MapObject::QuantumField, surface, alpha } = *trigger else { return };

    match surface {
        FaceSurface::Ui => {
            commands.entity(entity).insert(MaterialNode(face_materials.add(QuantumFieldFaceMaterial {})));
        }
        FaceSurface::World => {
            let Ok(grid_imprint) = grid_imprints.get(entity) else { return; };
            let quad_size = grid_imprint.world_size();
            commands.entity(entity).insert((
                Mesh2d(meshes.add(Rectangle::from_size(quad_size))),
                MeshMaterial2d(quad_materials.add(QuantumFieldQuadMaterial { quad_size, alpha })),
            ));
        }
    }
}
