//! Wisp visuals for side-menu nodes, live entities and placement ghosts.
//!
//! UI faces use dedicated `UiMaterial` types. World faces use the wisp type's lane component,
//! with the requested opacity stored as `alpha`.

use bevy::{
    prelude::*,
    reflect::TypePath,
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
    ui_render::ui_material::UiMaterial,
};

use game_core::prelude::{FaceSurface, MapObject, ObjectFaceRequest, WispType};

use super::looks::{WispElectricVisual, WispFireVisual, WispLightVisual, WispWaterVisual};

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

fn on_object_face_request_draw_wisp(
    trigger: On<ObjectFaceRequest>,
    mut commands: Commands,
    mut fire_materials: ResMut<Assets<WispFireFaceMaterial>>,
    mut water_materials: ResMut<Assets<WispWaterFaceMaterial>>,
    mut light_materials: ResMut<Assets<WispLightFaceMaterial>>,
    mut electric_materials: ResMut<Assets<WispElectricFaceMaterial>>,
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
        // visuals remain in the default state.
        FaceSurface::World => {
            let mut face_entity = commands.entity(entity);
            match wisp_type {
                WispType::Fire => { face_entity.insert(WispFireVisual::new(alpha)); }
                WispType::Water => { face_entity.insert(WispWaterVisual::new(alpha)); }
                WispType::Light => { face_entity.insert(WispLightVisual::new(alpha)); }
                WispType::Electric => { face_entity.insert(WispElectricVisual::new(alpha)); }
            }
        }
    }
}
