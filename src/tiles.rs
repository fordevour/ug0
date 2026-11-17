//! Кэш заранее отмасштабированных тайлов.
//!
//! Масштаб ограничен степенями двойки: 1, 2, 4, …, 128. Для каждой картинки
//! и каждого уровня хранится готовый тайл `S×S` (`S = 1 << level`), поэтому
//! рендер клетки сводится к копированию строк без сэмплирования на пиксель.
//! Сетка в тайлы не запекается — она рисуется отдельным проходом только на
//! крупных масштабах.

use std::collections::HashMap;
use std::time::Instant;

use crate::images::{Image, Images};
use crate::logging::debug;
use crate::palette::WHITE;

/// Количество уровней: `0..LEVELS`, то есть масштабы 1..128.
pub(crate) const LEVELS: u8 = 8;
pub(crate) const MAX_LEVEL: u8 = LEVELS - 1;
pub(crate) const MIN_LEVEL: u8 = 0;

/// Размер тайла в пикселях для уровня.
pub(crate) fn level_scale(level: u8) -> u32 {
    1 << level
}

/// Тайлы всех встроенных изображений плюс пустая клетка, по уровням.
pub(crate) struct Tiles {
    by_name: HashMap<&'static str, Vec<Image>>,
    empty: Vec<Image>,
}

impl Tiles {
    /// Строит тайлы для всех уровней. Вызывается один раз при старте.
    pub(crate) fn build(images: &Images) -> Self {
        let started = Instant::now();
        let mut by_name = HashMap::new();

        for (name, image) in images.entries() {
            let levels = (0..LEVELS).map(|level| scaled_tile(image, level)).collect();
            by_name.insert(name, levels);
        }

        let empty = (0..LEVELS)
            .map(|level| {
                let scale = level_scale(level);
                Image {
                    width: scale,
                    height: scale,
                    pixels: vec![WHITE; scale as usize * scale as usize],
                }
            })
            .collect();

        let tiles = Self { by_name, empty };
        debug!(
            "tiles: {} levels (1..{} px), {} KiB, built in {:.2} ms",
            LEVELS,
            level_scale(MAX_LEVEL),
            tiles.memory_bytes() / 1024,
            started.elapsed().as_secs_f64() * 1000.0
        );
        tiles
    }

    pub(crate) fn get(&self, name: &str, level: u8) -> Option<&Image> {
        self.by_name
            .get(name)
            .and_then(|levels| levels.get(level as usize))
    }

    pub(crate) fn empty(&self, level: u8) -> &Image {
        &self.empty[level as usize]
    }

    /// Сколько памяти занимают все тайлы.
    pub(crate) fn memory_bytes(&self) -> usize {
        let sum = |levels: &Vec<Image>| -> usize {
            levels
                .iter()
                .map(|tile| tile.pixels.len() * std::mem::size_of::<u32>())
                .sum()
        };

        self.by_name.values().map(sum).sum::<usize>() + sum(&self.empty)
    }

    #[cfg(test)]
    pub(crate) fn level_count(&self) -> usize {
        LEVELS as usize
    }
}

/// Тайл `S×S`: картинка, растянутая на всю клетку, без сетки.
fn scaled_tile(image: &Image, level: u8) -> Image {
    let scale = level_scale(level);
    let mut pixels = Vec::with_capacity(scale as usize * scale as usize);

    for ty in 0..scale {
        let v = sample_coord(ty, image.height, scale) as usize;
        let row = v * image.width as usize;
        for tx in 0..scale {
            let u = sample_coord(tx, image.width, scale) as usize;
            pixels.push(image.pixels[row + u]);
        }
    }

    Image {
        width: scale,
        height: scale,
        pixels,
    }
}

/// Ближайший пиксель исходной картинки для пикселя тайла `t`.
///
/// Повторяет старый попиксельный рендер: центр пикселя клетки находится
/// в `t + 0.5`, поэтому `u = floor((t + 0.5) * len / S)`.
fn sample_coord(t: u32, len: u32, scale: u32) -> u32 {
    if scale == 1 {
        // При S = 1 старый рендер сэмплировал t = 0; сетка на таком
        // масштабе не рисуется, клетка — один пиксель картинки.
        return 0;
    }

    let numerator = u64::from(2 * t + 1) * u64::from(len);
    let denominator = 2 * u64::from(scale);
    let u = numerator / denominator;
    u.min(u64::from(len - 1)) as u32
}
