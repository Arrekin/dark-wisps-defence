#![feature(associated_type_defaults)]

pub mod canvas;
pub mod color_pulsation;
pub mod cut_lane;
pub mod effect_visuals;
pub mod explosion;
pub mod lane;
pub mod map_light;
pub mod paint_lane;
pub mod render_schedule;
pub mod shader;
pub mod shared_gpu;
pub mod wisp_attack;

pub mod prelude {
    pub use super::color_pulsation::ColorPulsation;
    pub use super::explosion::BuilderExplosion;
    pub use super::wisp_attack::BuilderWispAttackEffect;
}
