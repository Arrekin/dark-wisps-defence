use bevy::{picking::hover::Hovered, prelude::*};

/// Owns the node's `BackgroundColor`: `normal` while not hovered, `hover` while hovered.
/// Change the normal color with [`HoverBackground::set_normal`], not by writing `BackgroundColor`.
#[derive(Component, Clone, Copy, Debug)]
#[require(Hovered, BackgroundColor)]
pub struct HoverBackground {
    normal: Color,
    hover: Color,
}
impl HoverBackground {
    pub fn new(normal: Color, hover: Color) -> Self {
        Self { normal, hover }
    }

    pub fn normal(&self) -> Color {
        self.normal
    }

    pub fn set_normal(&mut self, normal: Color) {
        self.normal = normal;
    }

    pub fn color(&self, hovered: bool) -> Color {
        if hovered { self.hover } else { self.normal }
    }
}
