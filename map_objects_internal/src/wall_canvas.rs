//! Renders walls and ramps in two passes, each using a quad that covers the map. Wisps render
//! between the passes, allowing wall tops to hide wisps inside tunnels.
//!
//! The wall shaders read each wall's style and shape from the obstacle cells, and the map's wall
//! style table from a shared GPU buffer.
//!
//! The tunnel reveal makes the tops of all hollow walls semi-transparent, showing their
//! interiors beneath.

use bevy::{
    prelude::*,
    reflect::TypePath,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
    sprite_render::{AlphaMode2d, Material2d, Material2dPlugin, MeshMaterial2d},
};
use strum::{EnumIter, IntoEnumIterator};

use game_core::prelude::{DrawLevel, MapInfo, ZDepth};
use map_objects::wall_style::{WallCanvasDebug, WallStyles};
use states::prelude::MapLoadingStage;
use visuals::{canvas::MapCanvasBundle, shared_gpu::{SharedGpuBinding, SharedGpuWriter}, specialize_with_shared_gpu};

use crate::tunnel_reveal::TunnelReveal;

pub(crate) struct WallCanvasPlugin;
impl Plugin for WallCanvasPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins(Material2dPlugin::<WallCanvasMaterial>::default())
            .init_resource::<WallCanvasDebug>()
            .add_systems(OnEnter(MapLoadingStage::Init), |mut commands: Commands| { commands.insert_resource(WallStyles::presets()); })
            .add_systems(OnEnter(MapLoadingStage::LoadResources), WallCanvas::create)
            .add_systems(Update, apply_tunnel_reveal.run_if(resource_changed::<TunnelReveal>))
            .add_systems(PostUpdate, (
                upload_gpu_wall_styles.run_if(resource_changed::<WallStyles>),
                apply_wall_canvas_debug.run_if(resource_changed::<WallCanvasDebug>),
            ));
    }
}

/// Field order and types mirror `WallCanvasSettings` in `assets/shaders/walls/canvas.wesl`.
#[derive(Clone, Copy, Debug, PartialEq, ShaderType, Default)]
struct WallCanvasSettings {
    debug_mode: u32,
    /// Pass selected by [`WallCanvasPass::gpu_index`].
    canvas_pass: u32,
    /// From 0 (hollow wall tops opaque) to 1 (fully revealed).
    tunnel_reveal: f32,
}

#[derive(Asset, AsBindGroup, TypePath, Debug, Clone, Default)]
struct WallCanvasMaterial {
    #[uniform(0)]
    settings: WallCanvasSettings,
}
impl Material2d for WallCanvasMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/walls/canvas.wesl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }

    specialize_with_shared_gpu!();
}

/// Separates wall surfaces into layers below and above wisps.
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter)]
enum WallCanvasPass {
    /// Contact shadows, entrance recesses, hollow wall interiors, and lower ramp halves.
    Bottom,
    /// Flat wall tops and upper ramp halves, starting at half the wall height.
    Top,
}
impl WallCanvasPass {
    /// Mirrors `PASS_*` in `assets/shaders/walls/canvas.wesl`.
    const fn gpu_index(self) -> u32 {
        match self {
            Self::Bottom => 0,
            Self::Top => 1,
        }
    }

    const fn z_depth(self) -> ZDepth {
        match self {
            Self::Bottom => ZDepth::WallBottom,
            Self::Top => ZDepth::WallTop,
        }
    }
}

/// One wall canvas quad.
#[derive(Component)]
#[require(DrawLevel::OnGround)]
struct WallCanvas;
impl WallCanvas {
    fn create(
        mut commands: Commands,
        map_info: Res<MapInfo>,
        debug: Res<WallCanvasDebug>,
        reveal: Res<TunnelReveal>,
        mut materials: ResMut<Assets<WallCanvasMaterial>>,
    ) {
        commands.despawn_all::<With<WallCanvas>>();

        for pass in WallCanvasPass::iter() {
            let material = materials.add(WallCanvasMaterial {
                settings: WallCanvasSettings { debug_mode: debug.gpu_index(), canvas_pass: pass.gpu_index(), tunnel_reveal: reveal.amount },
            });
            commands.spawn((
                MapCanvasBundle::new(material, &map_info),
                WallCanvas,
                pass.z_depth(),
            ));
        }
    }
}

fn upload_gpu_wall_styles(
    mut shared_gpu: SharedGpuWriter,
    styles: Res<WallStyles>,
) {
    shared_gpu.write(SharedGpuBinding::WallStyles, styles.entries.iter().map(|entry| entry.style));
}

fn apply_tunnel_reveal(
    mut materials: ResMut<Assets<WallCanvasMaterial>>,
    reveal: Res<TunnelReveal>,
    wall_canvases: Query<&MeshMaterial2d<WallCanvasMaterial>, With<WallCanvas>>,
) {
    for material in wall_canvases.iter() {
        let Some(mut material) = materials.get_mut(material) else { continue; };
        material.settings.tunnel_reveal = reveal.amount;
    }
}

fn apply_wall_canvas_debug(
    mut materials: ResMut<Assets<WallCanvasMaterial>>,
    debug: Res<WallCanvasDebug>,
    wall_canvases: Query<&MeshMaterial2d<WallCanvasMaterial>, With<WallCanvas>>,
) {
    for material in wall_canvases.iter() {
        let Some(mut material) = materials.get_mut(material) else { continue; };
        material.settings.debug_mode = debug.gpu_index();
    }
}
