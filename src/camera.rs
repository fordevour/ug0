use winit::event::{ElementState, MouseScrollDelta, WindowEvent};
use winit::keyboard::Key;

use crate::images::Image;
#[cfg(test)]
pub(crate) use crate::palette::WHITE;
pub(crate) use crate::palette::{BLACK, GRID};
use crate::tiles::{MAX_LEVEL, MIN_LEVEL, Tiles, level_scale};
use crate::world::World;

/// Сдвиг центра обзора за одно нажатие WASD, в ячейках.
pub(crate) const PAN_STEP: i64 = 1;

/// Уровень масштаба по умолчанию: 32 пикселя на клетку.
const DEFAULT_LEVEL: u8 = 5;

/// С этого уровня (16 px на клетку) и крупнее рисуется серая сетка.
/// На масштабах 1, 2, 4 и 8 px сетки нет.
pub(crate) const GRID_MIN_LEVEL: u8 = 4;

/// Кадр в формате softbuffer: 0x00RRGGBB.
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u32>,
}

pub struct Camera {
    /// Центр обзора в координатах мира (в ячейках). Целый, чтобы тайлы
    /// ложились ровно друг к другу.
    pub center: (i64, i64),
    /// Уровень масштаба: `scale = 1 << level`, от 1 до 128 пикселей на клетку.
    pub level: u8,
}

impl Camera {
    pub fn new() -> Self {
        Self {
            center: (0, 0),
            level: DEFAULT_LEVEL,
        }
    }

    /// Масштаб в пикселях на клетку.
    pub fn scale(&self) -> f64 {
        f64::from(self.scale_px())
    }

    fn scale_px(&self) -> u32 {
        level_scale(self.level.min(MAX_LEVEL))
    }

    /// Интерпретирует сырое событие окна как управление камерой.
    /// WASD двигают центр обзора, колесо мыши меняет масштаб вдвое.
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
                    MouseScrollDelta::LineDelta(_, y) => f64::from(*y),
                    MouseScrollDelta::PixelDelta(pos) => pos.y / 100.0,
                };
                // Половина «ступеньки» тачпада тоже считается шагом,
                // иначе мелкие движения колеса терялись бы.
                self.zoom_steps(lines.round() as i64)
            }
            _ => false,
        }
    }

    pub(crate) fn pan(&mut self, dx: i64, dy: i64) -> bool {
        self.center.0 += dx;
        self.center.1 += dy;
        true
    }

    /// Меняет уровень масштаба на `steps` ступеней (каждая ступень — ×2).
    pub(crate) fn zoom_steps(&mut self, steps: i64) -> bool {
        let level =
            (i64::from(self.level) + steps).clamp(i64::from(MIN_LEVEL), i64::from(MAX_LEVEL)) as u8;

        if level == self.level {
            return false;
        }
        self.level = level;
        true
    }

    /// Рендерит вид мира заданного размера в пикселях.
    /// Ячейки с изображением заполняются готовым тайлом, пустые — белым.
    /// На масштабах 1–8 px сетки нет; начиная с 16 px сверху отдельным
    /// проходом рисуется серая сетка 1 px. Всё вне мира чёрное.
    ///
    /// Координаты мира — это центры ячеек: ячейка (i, j) занимает
    /// `[i - 0.5, i + 0.5) × [j - 0.5, j + 0.5)`, поэтому камера в (0, 0)
    /// смотрит ровно в центр ячейки (0, 0), а не на стык четырёх ячеек.
    pub fn render(&self, world: &World, tiles: &Tiles, width: u32, height: u32) -> Frame {
        let mut frame = Frame {
            width,
            height,
            pixels: vec![BLACK; width as usize * height as usize],
        };

        let scale = self.scale_px();
        let offset = scale.div_ceil(2);
        let half_width = width / 2;
        let half_height = height / 2;
        let (cx, cy) = self.center;

        let (x_start, x_end) = visible_range(cx, half_width, width, scale, offset);
        let (y_start, y_end) = visible_range(cy, half_height, height, scale, offset);
        let x_start = x_start.max(0);
        let x_end = x_end.min(i64::from(world.width));
        let y_start = y_start.max(0);
        let y_end = y_end.min(i64::from(world.height));

        // Проход 1: тайлы.
        for y in y_start..y_end {
            let base_y = (y - cy) * i64::from(scale) + i64::from(half_height) - i64::from(offset);
            for x in x_start..x_end {
                let base_x =
                    (x - cx) * i64::from(scale) + i64::from(half_width) - i64::from(offset);
                let tile = world
                    .cell(x as u32, y as u32)
                    .and_then(|name| tiles.get(name, self.level))
                    .unwrap_or_else(|| tiles.empty(self.level));

                blit(&mut frame.pixels, width, height, base_x, base_y, tile);
            }
        }

        // Проход 2: сетка по левой и верхней кромке каждой видимой клетки.
        // На масштабах 1–8 px (уровни 0–3) сетка не рисуется.
        if self.level >= GRID_MIN_LEVEL {
            for y in y_start..y_end {
                let base_y =
                    (y - cy) * i64::from(scale) + i64::from(half_height) - i64::from(offset);
                for x in x_start..x_end {
                    let base_x =
                        (x - cx) * i64::from(scale) + i64::from(half_width) - i64::from(offset);
                    draw_grid(&mut frame.pixels, width, height, base_x, base_y, scale);
                }
            }
        }

        frame
    }
}

/// Возвращает полуинтервал индексов клеток, которые попадают на экран.
///
/// Клетка `i` занимает пиксели `[base, base + scale)`, где
/// `base = (i - center) * scale + half_extent - offset`. Здесь `offset` —
/// `(scale + 1) / 2`, что в точности повторяет старую попиксельную раскладку
/// (включая `scale = 1`) для чётного размера окна.
fn visible_range(
    center: i64,
    half_extent: u32,
    extent: u32,
    scale: u32,
    offset: u32,
) -> (i64, i64) {
    let scale = i64::from(scale);
    let half = i64::from(half_extent);
    let offset = i64::from(offset);

    let start = center + (offset - half - scale).div_euclid(scale) + 1;
    let numerator = i64::from(extent) - half + offset;
    let end = center + (numerator + scale - 1).div_euclid(scale);
    (start, end)
}

/// Копирует тайл в кадр с обрезкой по краям окна.
fn blit(frame: &mut [u32], width: u32, height: u32, base_x: i64, base_y: i64, tile: &Image) {
    let tile_size = i64::from(tile.width);
    let x0 = base_x.max(0);
    let y0 = base_y.max(0);
    let x1 = (base_x + tile_size).min(i64::from(width));
    let y1 = (base_y + tile_size).min(i64::from(height));
    if x0 >= x1 || y0 >= y1 {
        return;
    }

    let run = (x1 - x0) as usize;
    let src_x = (x0 - base_x) as usize;
    let src_y = (y0 - base_y) as usize;
    let tile_width = tile.width as usize;
    let frame_width = width as usize;

    for row in 0..(y1 - y0) as usize {
        let src = (src_y + row) * tile_width + src_x;
        let dst = (y0 as usize + row) * frame_width + x0 as usize;
        frame[dst..dst + run].copy_from_slice(&tile.pixels[src..src + run]);
    }
}

/// Рисует левую и верхнюю кромку клетки шириной 1 px.
fn draw_grid(frame: &mut [u32], width: u32, height: u32, base_x: i64, base_y: i64, scale: u32) {
    let scale = i64::from(scale);
    let width_i = i64::from(width);
    let height_i = i64::from(height);

    // Верхняя кромка — непрерывный отрезок строки.
    if base_y >= 0 && base_y < height_i {
        let x0 = base_x.max(0);
        let x1 = (base_x + scale).min(width_i);
        if x0 < x1 {
            let row = base_y as usize * width as usize;
            frame[row + x0 as usize..row + x1 as usize].fill(GRID);
        }
    }

    // Левая кромка — по одному пикселю на строку.
    if base_x >= 0 && base_x < width_i {
        let y0 = base_y.max(0);
        let y1 = (base_y + scale).min(height_i);
        let column = base_x as usize;
        let frame_width = width as usize;
        for y in y0..y1 {
            frame[y as usize * frame_width + column] = GRID;
        }
    }
}

impl Default for Camera {
    fn default() -> Self {
        Self::new()
    }
}

/// WASD → сдвиг центра обзора в ячейках.
pub(crate) fn pan_delta_for_key(key: &str) -> Option<(i64, i64)> {
    Some(match key.to_ascii_lowercase().as_str() {
        "w" => (0, -PAN_STEP),
        "s" => (0, PAN_STEP),
        "a" => (-PAN_STEP, 0),
        "d" => (PAN_STEP, 0),
        _ => return None,
    })
}
