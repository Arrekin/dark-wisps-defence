//! Wisp materials used by side-menu faces and world-space previews.
//!
//! UI faces have dedicated `UiMaterial` types. World faces reuse the materials used by live wisps
//! and apply request opacity through their `alpha` uniform.

use bevy::{
    prelude::*,
    reflect::TypePath,
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
    ui_render::ui_material::UiMaterial,
};

use game_core::prelude::{FaceSurface, MapObject, ObjectFaceRequest, WispType};
use wisps::prelude::WISP_GRID_IMPRINT;

use super::materials::{WispElectricMaterial, WispFireMaterial, WispLightMaterial, WispMaterial, WispWaterMaterial};

pub(crate) struct WispFacePlugin;
impl Plugin for WispFacePlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                UiMaterialPlugin::<WispFireFaceMaterial>::default(),
                UiMaterialPlugin::<WispWaterFaceMaterial>::default(),
                UiMaterialPlugin::<WispLightFaceMaterial>::default(),
                UiMaterialPlugin::<WispElectricFaceMaterial>::default(),
            ))
            .add_observer(on_object_face_request_draw_wisp);
    }
}

// Stateless materials; each face shader derives its output from node UVs and global time.
#[derive(Asset, AsBindGroup, TypePath, Clone, Copy, Debug)]
pub(crate) struct WispFireFaceMaterial {}
impl UiMaterial for WispFireFaceMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/wisps/fire_face.wesl".into()
    }
}

#[derive(Asset, AsBindGroup, TypePath, Clone, Copy, Debug)]
pub(crate) struct WispWaterFaceMaterial {}
impl UiMaterial for WispWaterFaceMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/wisps/water_face.wesl".into()
    }
}

#[derive(Asset, AsBindGroup, TypePath, Clone, Copy, Debug)]
pub(crate) struct WispLightFaceMaterial {}
impl UiMaterial for WispLightFaceMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/wisps/light_face.wesl".into()
    }
}

#[derive(Asset, AsBindGroup, TypePath, Clone, Copy, Debug)]
pub(crate) struct WispElectricFaceMaterial {}
impl UiMaterial for WispElectricFaceMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/wisps/electric_face.wesl".into()
    }
}

/// Attaches a wisp mesh and material to a world entity.
///
/// The material's alpha uniform supports both opaque wisps and translucent placement ghosts.
fn attach_wisp_world_face<M: Asset + WispMaterial>(
    commands: &mut Commands,
    entity: Entity,
    alpha: f32,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<M>,
) {
    let mut material = M::make();
    material.set_alpha(alpha);
    let world_size = WISP_GRID_IMPRINT.world_size() * M::mesh_scale();
    let mesh = meshes.add(Rectangle::new(world_size.x, world_size.y));
    commands.entity(entity).insert((
        Mesh2d(mesh),
        MeshMaterial2d(materials.add(material)),
    ));
}

fn on_object_face_request_draw_wisp(
    trigger: On<ObjectFaceRequest>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut fire_materials: ResMut<Assets<WispFireFaceMaterial>>,
    mut water_materials: ResMut<Assets<WispWaterFaceMaterial>>,
    mut light_materials: ResMut<Assets<WispLightFaceMaterial>>,
    mut electric_materials: ResMut<Assets<WispElectricFaceMaterial>>,
    mut fire_world_materials: ResMut<Assets<WispFireMaterial>>,
    mut water_world_materials: ResMut<Assets<WispWaterMaterial>>,
    mut light_world_materials: ResMut<Assets<WispLightMaterial>>,
    mut electric_world_materials: ResMut<Assets<WispElectricMaterial>>,
) {
    let ObjectFaceRequest { entity, object: MapObject::Wisp(wisp_type), surface, alpha } = *trigger else { return };

    match surface {
        FaceSurface::Ui => {
            let mut face_node = commands.entity(entity);
            match wisp_type {
                WispType::Fire => { face_node.insert(MaterialNode(fire_materials.add(WispFireFaceMaterial {}))); }
                WispType::Water => { face_node.insert(MaterialNode(water_materials.add(WispWaterFaceMaterial {}))); }
                WispType::Light => { face_node.insert(MaterialNode(light_materials.add(WispLightFaceMaterial {}))); }
                WispType::Electric => { face_node.insert(MaterialNode(electric_materials.add(WispElectricFaceMaterial {}))); }
            }
        }
        // Preview entities lack the components used by motion and effect systems, so their
        // materials remain in the default state.
        FaceSurface::World => {
            match wisp_type {
                WispType::Fire => attach_wisp_world_face(&mut commands, entity, alpha, &mut meshes, &mut fire_world_materials),
                WispType::Water => attach_wisp_world_face(&mut commands, entity, alpha, &mut meshes, &mut water_world_materials),
                WispType::Light => attach_wisp_world_face(&mut commands, entity, alpha, &mut meshes, &mut light_world_materials),
                WispType::Electric => attach_wisp_world_face(&mut commands, entity, alpha, &mut meshes, &mut electric_world_materials),
            }
        }
    }
}
