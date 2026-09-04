//! Stationary wisp UI materials used by side-menu tiles.
//!
//! They reuse the `dwd::wisps::*` shader libraries. Each variant needs a `UiMaterial` type because
//! map rendering uses incompatible `Material2d` bindings.

use bevy::{
    prelude::*,
    reflect::TypePath,
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
    ui_render::ui_material::UiMaterial,
};

use game_core::prelude::WispType;
use wisps::prelude::BuilderWispFace;

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
            .add_observer(on_builder_add_spawn_wisp_face);
    }
}

// Stateless materials; each face shader derives its output from node UVs and global time.
#[derive(Asset, AsBindGroup, TypePath, Clone, Copy, Debug)]
pub(crate) struct WispFireFaceMaterial {}
impl UiMaterial for WispFireFaceMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/wisps/fire_face.wgsl".into()
    }
}

#[derive(Asset, AsBindGroup, TypePath, Clone, Copy, Debug)]
pub(crate) struct WispWaterFaceMaterial {}
impl UiMaterial for WispWaterFaceMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/wisps/water_face.wgsl".into()
    }
}

#[derive(Asset, AsBindGroup, TypePath, Clone, Copy, Debug)]
pub(crate) struct WispLightFaceMaterial {}
impl UiMaterial for WispLightFaceMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/wisps/light_face.wgsl".into()
    }
}

#[derive(Asset, AsBindGroup, TypePath, Clone, Copy, Debug)]
pub(crate) struct WispElectricFaceMaterial {}
impl UiMaterial for WispElectricFaceMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/wisps/electric_face.wgsl".into()
    }
}

fn on_builder_add_spawn_wisp_face(
    trigger: On<Add, BuilderWispFace>,
    mut commands: Commands,
    mut fire_materials: ResMut<Assets<WispFireFaceMaterial>>,
    mut water_materials: ResMut<Assets<WispWaterFaceMaterial>>,
    mut light_materials: ResMut<Assets<WispLightFaceMaterial>>,
    mut electric_materials: ResMut<Assets<WispElectricFaceMaterial>>,
    faces: Query<&BuilderWispFace>,
) {
    let entity = trigger.entity;
    let Ok(face) = faces.get(entity) else { return; };

    let mut face_node = commands.entity(entity);
    face_node.remove::<BuilderWispFace>();
    match face.0 {
        WispType::Fire => { face_node.insert(MaterialNode(fire_materials.add(WispFireFaceMaterial {}))); }
        WispType::Water => { face_node.insert(MaterialNode(water_materials.add(WispWaterFaceMaterial {}))); }
        WispType::Light => { face_node.insert(MaterialNode(light_materials.add(WispLightFaceMaterial {}))); }
        WispType::Electric => { face_node.insert(MaterialNode(electric_materials.add(WispElectricFaceMaterial {}))); }
    }
}
