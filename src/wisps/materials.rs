use bevy::{
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
    sprite_render::{AlphaMode2d, Material2d},
};

use crate::prelude::*;
use crate::visual_effects::effect_material::{EffectVisualMaterial, EffectVisualUniform};

pub trait WispMaterial: Material2d {
    fn make(asset_server: &AssetServer) -> Self;
}

#[derive(Asset, TypePath, Debug, Clone, AsBindGroup)]
pub struct WispFireMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub wisp_tex1: Handle<Image>,
    #[texture(2)]
    pub wisp_tex2: Handle<Image>,

    #[uniform(4)]
    pub amplitude: f32,
    #[uniform(4)]
    pub frequency: f32,
    #[uniform(4)]
    pub speed: f32,
    #[uniform(4)]
    pub sinus_direction: f32,
    #[uniform(4)]
    pub cosinus_direction: f32,

    #[uniform(5)]
    pub effects: EffectVisualUniform,
}
impl Material2d for WispFireMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/wisps/fire.wgsl".into()
    }
    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}
impl WispMaterial for WispFireMaterial {
    fn make(asset_server: &AssetServer) -> Self {
        let mut rng = nanorand::tls_rng();
        Self {
            amplitude: rng.generate::<f32>() * 0.2 + 0.25, // 0.25 - 0.45
            frequency: rng.generate::<f32>() * 5. + 15., // 15 - 20
            speed: rng.generate::<f32>() * 3. + 4., // 4 - 7
            sinus_direction: [-1., 1.][rng.generate::<usize>() % 2],
            cosinus_direction: [-1., 1.][rng.generate::<usize>() % 2],
            wisp_tex1: asset_server.load("wisps/big_wisp.png"),
            wisp_tex2: asset_server.load("wisps/big_wisp.png"),
            effects: EffectVisualUniform::default(),
        }
    }
}
impl EffectVisualMaterial for WispFireMaterial {
    fn effects_mut(&mut self) -> &mut EffectVisualUniform {
        &mut self.effects
    }
}

#[derive(Asset, TypePath, Debug, Clone, AsBindGroup)]
pub struct WispWaterMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub wisp_tex1: Handle<Image>,
    #[texture(2)]
    pub wisp_tex2: Handle<Image>,

    #[uniform(4)]
    pub amplitude: f32,
    #[uniform(4)]
    pub frequency: f32,
    #[uniform(4)]
    pub speed: f32,
    #[uniform(4)]
    pub sinus_direction: f32,
    #[uniform(4)]
    pub cosinus_direction: f32,

    #[uniform(5)]
    pub effects: EffectVisualUniform,
}
impl Material2d for WispWaterMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/wisps/water.wgsl".into()
    }
    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}
impl WispMaterial for WispWaterMaterial {
    fn make(asset_server: &AssetServer) -> Self {
        let mut rng = nanorand::tls_rng();
        Self {
            amplitude: rng.generate::<f32>() * 0.06 + 0.08, // 0.08 - 0.14
            frequency: rng.generate::<f32>() * 0.4 + 2.02, // 2.2 - 2.6
            speed: rng.generate::<f32>() * 3. + 3., // 3.0 - 6.0
            sinus_direction: [-1., 1.][rng.generate::<usize>() % 2],
            cosinus_direction: [-1., 1.][rng.generate::<usize>() % 2],
            wisp_tex1: asset_server.load("wisps/big_wisp.png"),
            wisp_tex2: asset_server.load("wisps/big_wisp.png"),
            effects: EffectVisualUniform::default(),
        }
    }
}
impl EffectVisualMaterial for WispWaterMaterial {
    fn effects_mut(&mut self) -> &mut EffectVisualUniform {
        &mut self.effects
    }
}


#[derive(Asset, TypePath, Debug, Clone, AsBindGroup)]
pub struct WispLightMaterial {
    #[uniform(4)]
    pub radiance_angle: f32,
    #[uniform(4)]
    pub radiance_radius: f32,

    #[uniform(5)]
    pub effects: EffectVisualUniform,
}
impl Material2d for WispLightMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/wisps/light.wgsl".into()
    }
    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}
impl WispMaterial for WispLightMaterial {
    fn make(_asset_server: &AssetServer) -> Self {
        let mut rng = nanorand::tls_rng();
        Self {
            radiance_angle: rng.generate::<f32>() * 20. + 10., // 10.0 - 30.0
            radiance_radius: rng.generate::<f32>() * 10. + 5., // 5.0 - 15.0
            effects: EffectVisualUniform::default(),
        }
    }
}
impl EffectVisualMaterial for WispLightMaterial {
    fn effects_mut(&mut self) -> &mut EffectVisualUniform {
        &mut self.effects
    }
}

#[derive(Asset, TypePath, Debug, Clone, AsBindGroup)]
pub struct WispElectricMaterial {
    #[uniform(4)]
    pub angle_direction: f32,
    #[uniform(4)]
    pub radius_direction: f32,

    #[uniform(5)]
    pub effects: EffectVisualUniform,
}
impl Material2d for WispElectricMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/wisps/electric.wgsl".into()
    }
    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}
impl WispMaterial for WispElectricMaterial {
    fn make(_asset_server: &AssetServer) -> Self {
        let mut rng = nanorand::tls_rng();
        Self {
            angle_direction: [-1., 1.][rng.generate::<usize>() % 2],
            radius_direction: [-1., 1.][rng.generate::<usize>() % 2],
            effects: EffectVisualUniform::default(),
        }
    }
}
impl EffectVisualMaterial for WispElectricMaterial {
    fn effects_mut(&mut self) -> &mut EffectVisualUniform {
        &mut self.effects
    }
}
