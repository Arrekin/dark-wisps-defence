//! Materials for dark-ore UI faces and world-space placement previews.

use bevy::{
    prelude::*,
    reflect::TypePath,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
    sprite_render::{AlphaMode2d, Material2d, Material2dPlugin, MeshMaterial2d},
    ui_render::ui_material::UiMaterial,
};

use game_core::prelude::{FaceSurface, GridImprint, MapObject, ObjectFaceRequest};

pub(crate) struct DarkOreMaterialsPlugin;
impl Plugin for DarkOreMaterialsPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins(UiMaterialPlugin::<DarkOreFaceMaterial>::default())
            .add_plugins(Material2dPlugin::<DarkOreQuadMaterial>::default())
            .add_observer(on_object_face_request_draw_dark_ore);
    }
}

/// Stateless; the face shader derives the deposit from node UVs alone.
#[derive(Asset, AsBindGroup, TypePath, Clone, Copy, Debug)]
pub(crate) struct DarkOreFaceMaterial {}
impl UiMaterial for DarkOreFaceMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/dark_ore/face.wgsl".into()
    }
}

/// Field order and types mirror `DarkOreQuad` in `assets/shaders/dark_ore/quad.wgsl`.
#[derive(ShaderType, Clone, Copy, Debug)]
struct DarkOreQuadUniform {
    alpha: f32,
}

/// World-space material for one isolated deposit.
#[derive(Asset, AsBindGroup, TypePath, Clone, Copy, Debug)]
pub(crate) struct DarkOreQuadMaterial {
    #[uniform(0)]
    quad: DarkOreQuadUniform,
}
impl Material2d for DarkOreQuadMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/dark_ore/quad.wgsl".into()
    }
    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

/// Attaches a dark-ore face for UI or placement preview use.
///
/// Placed deposits are rendered separately by the grid-wide dark-ore canvas.
fn on_object_face_request_draw_dark_ore(
    trigger: On<ObjectFaceRequest>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut face_materials: ResMut<Assets<DarkOreFaceMaterial>>,
    mut quad_materials: ResMut<Assets<DarkOreQuadMaterial>>,
    grid_imprints: Query<&GridImprint>,
) {
    let ObjectFaceRequest { entity, object: MapObject::DarkOre, surface, alpha } = *trigger else { return };

    match surface {
        FaceSurface::Ui => {
            commands.entity(entity).insert(MaterialNode(face_materials.add(DarkOreFaceMaterial {})));
        }
        FaceSurface::World => {
            let Ok(grid_imprint) = grid_imprints.get(entity) else { return; };
            commands.entity(entity).insert((
                Mesh2d(meshes.add(Rectangle::from_size(grid_imprint.world_size()))),
                MeshMaterial2d(quad_materials.add(DarkOreQuadMaterial { quad: DarkOreQuadUniform { alpha } })),
            ));
        }
    }
}
