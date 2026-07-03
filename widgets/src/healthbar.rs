use bevy::color::palettes::css::GREEN;
use bevy::prelude::*;

#[derive(Component)]
#[require(Node)]
pub struct Healthbar {
    pub value: f32,
    pub max_value: f32,
    pub font_size: f32,
    pub color: Color,
}
impl Default for Healthbar {
    fn default() -> Self {
        Self { value: 0., max_value: 0., font_size: 16., color: GREEN.into() }
    }
}
impl Healthbar {
    pub fn get_percent(&self) -> f32 {
        if self.max_value == 0. { 100. }
        else { self.value / self.max_value * 100. }
    }
}
