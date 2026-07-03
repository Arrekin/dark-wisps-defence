use bevy::prelude::*;

#[derive(Component, Default)]
pub struct ColorPulsation {
    min_brightness: f32,
    max_brightness: f32,
    duration: f32,
    is_increasing: bool,
    delta_change: f32,
}
impl ColorPulsation {
    pub fn new(min_brightness: f32, max_brightness: f32, duration: f32) -> Self {
        let mut color_pulsation = ColorPulsation::default();
        color_pulsation.update_parameters(min_brightness, max_brightness, duration);
        color_pulsation
    }
    pub fn update_parameters(&mut self, min_brightness: f32, max_brightness: f32, duration: f32) {
        assert!(min_brightness < max_brightness, "min_brightness must be less than max_brightness");
        self.min_brightness = min_brightness;
        self.max_brightness = max_brightness;
        self.duration = duration;
        self.delta_change = (max_brightness - min_brightness) / duration;
    }

    pub fn advance(&mut self, lightness: f32, dt: f32) -> f32 {
        if self.is_increasing && lightness > self.max_brightness {
            self.is_increasing = false;
        } else if !self.is_increasing && lightness < self.min_brightness {
            self.is_increasing = true;
        }
        lightness + dt * self.delta_change * if self.is_increasing { 1. } else { -1. }
    }
}
