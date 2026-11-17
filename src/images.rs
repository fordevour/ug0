//! Встроенные в бинарник изображения.
//!
//! Файлы лежат в `img/` в корне проекта и попадают в бинарник через
//! `include_bytes!`. Имя картинки — это имя файла без расширения.

use std::collections::HashMap;
use std::io::Cursor;
use std::time::Instant;

use crate::logging::debug;

/// Список вшиваемых изображений. Новый файл `img/<name>.png` нужно добавить сюда;
/// тест `every_img_file_is_embedded` следит за тем, чтобы список не разошёлся с папкой.
macro_rules! embedded {
    ($($name:literal),* $(,)?) => {
        &[$(($name, include_bytes!(concat!("../img/", $name, ".png")).as_slice())),*]
    };
}

pub(crate) const EMBEDDED: &[(&str, &[u8])] = embedded!["earth", "grass"];

/// Декодированное изображение. Пиксели уже непрозрачные (`0x00RRGGBB`):
/// альфа-канал наложен на белый фон при загрузке.
pub(crate) struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u32>,
}

impl Image {
    #[cfg(test)]
    pub(crate) fn new(width: u32, height: u32, pixels: Vec<u32>) -> Self {
        debug_assert_eq!(pixels.len(), width as usize * height as usize);
        Self {
            width,
            height,
            pixels,
        }
    }
}

/// Набор встроенных изображений с поиском по имени.
pub(crate) struct Images {
    by_name: HashMap<&'static str, usize>,
    images: Vec<Image>,
}

impl Images {
    /// Декодирует все встроенные изображения. Вызывается один раз при старте.
    pub(crate) fn load() -> Result<Self, String> {
        let mut by_name = HashMap::with_capacity(EMBEDDED.len());
        let mut images = Vec::with_capacity(EMBEDDED.len());

        for &(name, bytes) in EMBEDDED {
            let started = Instant::now();
            let image = decode_png(name, bytes)?;
            debug!(
                "img/{name}.png: {}x{} decoded in {:.2} ms",
                image.width,
                image.height,
                started.elapsed().as_secs_f64() * 1000.0
            );
            by_name.insert(name, images.len());
            images.push(image);
        }

        Ok(Self { by_name, images })
    }

    /// Строка для стартового лога: `earth 32x32, grass 32x32`.
    pub(crate) fn summary(&self) -> String {
        EMBEDDED
            .iter()
            .filter_map(|&(name, _)| {
                self.get(name)
                    .map(|image| format!("{name} {}x{}", image.width, image.height))
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub(crate) fn get(&self, name: &str) -> Option<&Image> {
        self.by_name.get(name).map(|&index| &self.images[index])
    }

    /// Все картинки с их именами.
    pub(crate) fn entries(&self) -> impl Iterator<Item = (&'static str, &Image)> {
        self.by_name
            .iter()
            .map(move |(&name, &index)| (name, &self.images[index]))
    }

    #[cfg(test)]
    pub(crate) fn from_images(items: Vec<(&'static str, Image)>) -> Self {
        let mut by_name = HashMap::with_capacity(items.len());
        let mut images = Vec::with_capacity(items.len());
        for (name, image) in items {
            by_name.insert(name, images.len());
            images.push(image);
        }
        Self { by_name, images }
    }
}

/// PNG → RGBA8 → непрозрачные пиксели `0x00RRGGBB` поверх белого.
fn decode_png(name: &str, bytes: &[u8]) -> Result<Image, String> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());

    let mut reader = decoder
        .read_info()
        .map_err(|err| format!("img/{name}.png: {err}"))?;

    let mut buffer = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buffer)
        .map_err(|err| format!("img/{name}.png: {err}"))?;

    if info.width == 0 || info.height == 0 {
        return Err(format!("img/{name}.png: пустое изображение"));
    }

    let width = info.width as usize;
    let height = info.height as usize;
    let mut pixels = Vec::with_capacity(width * height);
    let data = &buffer[..info.buffer_size()];

    for y in 0..height {
        let start = y * info.line_size;
        let row = &data[start..start + width * bytes_per_pixel(info.color_type)];

        match info.color_type {
            png::ColorType::Rgba => {
                for px in row.as_chunks::<4>().0 {
                    pixels.push(blend_on_white(px[0], px[1], px[2], px[3]));
                }
            }
            png::ColorType::Rgb => {
                for px in row.as_chunks::<3>().0 {
                    pixels.push(rgb(px[0], px[1], px[2]));
                }
            }
            png::ColorType::GrayscaleAlpha => {
                for px in row.as_chunks::<2>().0 {
                    pixels.push(blend_on_white(px[0], px[0], px[0], px[1]));
                }
            }
            png::ColorType::Grayscale => {
                for &value in row {
                    pixels.push(rgb(value, value, value));
                }
            }
            png::ColorType::Indexed => {
                return Err(format!(
                    "img/{name}.png: не удалось раскрыть палитровое изображение"
                ));
            }
        }
    }

    Ok(Image {
        width: width as u32,
        height: height as u32,
        pixels,
    })
}

fn bytes_per_pixel(color_type: png::ColorType) -> usize {
    match color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Grayscale | png::ColorType::Indexed => 1,
    }
}

fn rgb(r: u8, g: u8, b: u8) -> u32 {
    ((r as u32) << 16) | ((g as u32) << 8) | b as u32
}

/// Накладывает цвет с альфой `a` на белый фон.
pub(crate) fn blend_on_white(r: u8, g: u8, b: u8, a: u8) -> u32 {
    let alpha = a as u32;
    let blend = |channel: u8| -> u32 { (channel as u32 * alpha + 255 * (255 - alpha) + 127) / 255 };
    (blend(r) << 16) | (blend(g) << 8) | blend(b)
}
