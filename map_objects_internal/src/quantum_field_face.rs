//! Quantum-field UI material used by side-menu tiles.
//!
//! It reuses the boundary and moiré glow from `dwd::quantum_field`. Frame-sampling distortion
//! remains exclusive to the map post-process shader.

use bevy::{
    prelude::*,
    reflect::TypePath,
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
    ui_render::ui_material::UiMaterial,
};

use map_objects::prelude::BuilderQuantumFieldFace;

pub(crate) struct QuantumFieldFacePlugin;
impl Plugin for QuantumFieldFacePlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins(UiMaterialPlugin::<QuantumFieldFaceMaterial>::default())
            .add_observer(on_builder_add_spawn_quantum_field_face);
    }
}

/// Stateless material; the shader derives its output from node UVs and global time.
#[derive(Asset, AsBindGroup, TypePath, Clone, Copy, Debug)]
pub(crate) struct QuantumFieldFaceMaterial {}
impl UiMaterial for QuantumFieldFaceMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/quantum_field_face.wgsl".into()
    }
}

fn on_builder_add_spawn_quantum_field_face(
    trigger: On<Add, BuilderQuantumFieldFace>,
    mut commands: Commands,
    mut materials: ResMut<Assets<QuantumFieldFaceMaterial>>,
) {
    commands.entity(trigger.entity)
        .remove::<BuilderQuantumFieldFace>()
        .insert(MaterialNode(materials.add(QuantumFieldFaceMaterial {})));
}
