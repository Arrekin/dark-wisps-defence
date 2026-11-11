//! # Underground canvas
//!
//! Two quads cover the map and render terrain from the shared obstacle grid.
//!
//! The bottom pass draws pit floors, ramps, cliffs and tunnel interiors below underground wisps
//! and floor covers. The top pass draws ground and overhanging rims above them, hiding tunnel
//! interiors and their occupants.
//!
//! Both passes read [`TunnelReveal`]: the bottom pass draws tunnel interiors while reveal is active,
//! and the top pass makes the ground above them translucent.

use bevy::{
    prelude::*,
    reflect::TypePath,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
    sprite_render::{AlphaMode2d, Material2d, Material2dPlugin, MeshMaterial2d},
};
use strum::{EnumIter, IntoEnumIterator};

use game_core::prelude::{DrawLevel, MapInfo, ZDepth};
use states::prelude::MapLoadingStage;
use visuals::{canvas::MapCanvasBundle, specialize_with_shared_gpu};

use crate::tunnel_reveal::TunnelReveal;

pub(crate) struct UndergroundCanvasPlugin;
impl Plugin for UndergroundCanvasPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins(Material2dPlugin::<UndergroundCanvasMaterial>::default())
            .add_systems(OnEnter(MapLoadingStage::LoadResources), UndergroundCanvas::create)
            .add_systems(Update, apply_tunnel_reveal.run_if(resource_changed::<TunnelReveal>));
    }
}

/// Field order and types mirror `UndergroundCanvasSettings` in
/// `assets/shaders/underground/canvas.wesl`.
#[derive(Clone, Copy, Debug, PartialEq, ShaderType, Default)]
struct UndergroundCanvasSettings {
    /// Pass selected by [`UndergroundCanvasPass::gpu_index`].
    canvas_pass: u32,
    /// Reveal amount: 0 keeps ground opaque; 1 uses the shader's minimum cover opacity.
    tunnel_reveal: f32,
}

#[derive(Asset, AsBindGroup, TypePath, Debug, Clone, Default)]
struct UndergroundCanvasMaterial {
    #[uniform(0)]
    settings: UndergroundCanvasSettings,
}
impl Material2d for UndergroundCanvasMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/underground/canvas.wesl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }

    specialize_with_shared_gpu!();
}

/// Places terrain below underground occupants and ground cover above them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter)]
enum UndergroundCanvasPass {
    /// Pit floors, ramps, cliff faces, tunnel mouths and tunnel interiors.
    Bottom,
    /// The ground and the rims of pits and ramps.
    Top,
}
impl UndergroundCanvasPass {
    /// Mirrors `PASS_*` in `assets/shaders/underground/canvas.wesl`.
    const fn gpu_index(self) -> u32 {
        match self {
            Self::Bottom => 0,
            Self::Top => 1,
        }
    }

    const fn z_depth(self) -> ZDepth {
        match self {
            Self::Bottom => ZDepth::UndergroundBottom,
            Self::Top => ZDepth::UndergroundTop,
        }
    }

    const fn draw_level(self) -> DrawLevel {
        match self {
            Self::Bottom => DrawLevel::UndergroundFloor,
            Self::Top => DrawLevel::GroundFloor,
        }
    }
}

/// One underground canvas quad.
#[derive(Component)]
struct UndergroundCanvas;
impl UndergroundCanvas {
    fn create(
        mut commands: Commands,
        map_info: Res<MapInfo>,
        reveal: Res<TunnelReveal>,
        mut materials: ResMut<Assets<UndergroundCanvasMaterial>>,
    ) {
        commands.despawn_all::<With<UndergroundCanvas>>();

        for pass in UndergroundCanvasPass::iter() {
            let material = materials.add(UndergroundCanvasMaterial {
                settings: UndergroundCanvasSettings { canvas_pass: pass.gpu_index(), tunnel_reveal: reveal.amount },
            });
            commands.spawn((
                MapCanvasBundle::new(material, &map_info),
                UndergroundCanvas,
                pass.z_depth(),
                pass.draw_level(),
            ));
        }
    }
}

fn apply_tunnel_reveal(
    mut materials: ResMut<Assets<UndergroundCanvasMaterial>>,
    reveal: Res<TunnelReveal>,
    underground_canvases: Query<&MeshMaterial2d<UndergroundCanvasMaterial>, With<UndergroundCanvas>>,
) {
    for material in underground_canvases.iter() {
        let Some(mut material) = materials.get_mut(material) else { continue; };
        material.settings.tunnel_reveal = reveal.amount;
    }
}
