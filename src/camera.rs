use winit::event::{ElementState, MouseScrollDelta, WindowEvent};
use winit::keyboard::Key;

use crate::world::World;

pub(crate) const BLACK: u32 = 0x0000_0000;
pub(crate) const WHITE: u32 = 0x00FF_FFFF;
pub(crate) const GRID: u32 = 0x0080_8080;

/// Сдвиг центра обзора за одно нажатие WASD, в ячейках.
pub(crate) const PAN_STEP: f64 = 1.0;
/// Во сколько раз меняется масштаб за одну «ступеньку» колеса.
pub(crate) const ZOOM_STEP: f64 = 1.1;
pub(crate) const MIN_SCALE: f64 = 1.0;
pub(crate) const MAX_SCALE: f64 = 200.0;

/// Кадр в формате softbuffer: 0x00RRGGBB.
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u32>,
}

pub struct Camera {
    /// Центр обзора в координатах мира (в ячейках).
    pub center: (f64, f64),
    /// Масштаб: пикселей на ячейку.
    pub scale: f64,
}

impl Camera {
    pub fn new() -> Self {
        Self {
            center: (0.0, 0.0),
            scale: 25.0,
        }
    }

    /// Интерпретирует сырое событие окна как управление камерой.
    /// WASD двигают центр обзора, колесо мыши меняет масштаб.
    /// Возвращает `true`, если состояние камеры изменилось и нужен кадр.
    pub fn handle_event(&mut self, event: &WindowEvent) -> bool {
        match event {
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                let Key::Character(ch) = &event.logical_key else {
                    return false;
                };
                let Some((dx, dy)) = pan_delta_for_key(ch) else {
                    return false;
                };
                self.pan(dx, dy)
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => *y as f64,
                    MouseScrollDelta::PixelDelta(pos) => pos.y / 100.0,
                };
                self.zoom(ZOOM_STEP.powf(lines))
            }
            _ => false,
        }
    }

    pub(crate) fn pan(&mut self, dx: f64, dy: f64) -> bool {
        self.center.0 += dx;
        self.center.1 += dy;
        true
    }

    pub(crate) fn zoom(&mut self, factor: f64) -> bool {
        let scale = (self.scale * factor).clamp(MIN_SCALE, MAX_SCALE);
        if scale == self.scale {
            return false;
        }
        self.scale = scale;
        true
    }

    /// Рендерит вид мира заданного размера в пикселях.
    /// Ячейки мира — белые с серой рамкой 1 px, всё остальное чёрное.
    ///
    /// Координаты мира — это центры ячеек: ячейка (i, j) занимает
    /// `[i - 0.5, i + 0.5) × [j - 0.5, j + 0.5)`, поэтому камера в (0, 0)
    /// смотрит ровно в центр ячейки (0, 0), а не на стык четырёх ячеек.
    pub fn render(&self, world: &World, width: u32, height: u32) -> Frame {
        let mut frame = Frame {
            width,
            height,
            pixels: vec![BLACK; width as usize * height as usize],
        };
        let (cx, cy) = self.center;

        // Границы мира: -0.5 — левый/верхний край ячейки (0, 0).
        let min = -0.5;
        let max_x = world.width as f64 - 0.5;
        let max_y = world.height as f64 - 0.5;

        for py in 0..height {
            for px in 0..width {
                // Центр пикселя → мировые координаты (в ячейках).
                let wx = cx + (px as f64 + 0.5 - width as f64 / 2.0) / self.scale;
                let wy = cy + (py as f64 + 0.5 - height as f64 / 2.0) / self.scale;

                if wx < min || wy < min || wx >= max_x || wy >= max_y {
                    continue; // за границами мира — чёрный (уже залит)
                }

                // Положение внутри ячейки в пикселях экрана.
                let fx = ((wx + 0.5).fract() * self.scale).min(self.scale);
                let fy = ((wy + 0.5).fract() * self.scale).min(self.scale);

                frame.pixels[(py * width + px) as usize] =
                    if fx < 1.0 || fy < 1.0 { GRID } else { WHITE };
            }
        }

        frame
    }
}

impl Default for Camera {
    fn default() -> Self {
        Self::new()
    }
}

/// WASD → сдвиг центра обзора в ячейках.
pub(crate) fn pan_delta_for_key(key: &str) -> Option<(f64, f64)> {
    Some(match key.to_ascii_lowercase().as_str() {
        "w" => (0.0, -PAN_STEP),
        "s" => (0.0, PAN_STEP),
        "a" => (-PAN_STEP, 0.0),
        "d" => (PAN_STEP, 0.0),
        _ => return None,
    })
}
