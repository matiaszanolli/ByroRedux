//! Common engine types.

/// RGBA color with f32 components in [0, 1].
#[derive(Debug, Clone, Copy)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const BLACK: Self = Self { r: 0.0, g: 0.0, b: 0.0, a: 1.0 };
    pub const WHITE: Self = Self { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };
    pub const CORNFLOWER_BLUE: Self = Self { r: 0.392, g: 0.584, b: 0.929, a: 1.0 };

    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub const fn as_array(&self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }
}
