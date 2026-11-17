/// Мир — прямоугольник ячеек: x ∈ [0, width), y ∈ [0, height).
#[derive(Debug, Clone, Copy)]
pub struct World {
    pub width: u32,
    pub height: u32,
}

impl World {
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }
}

impl Default for World {
    fn default() -> Self {
        Self::new(10, 10)
    }
}
