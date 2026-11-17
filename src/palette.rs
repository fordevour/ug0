//! Цвета кадра в формате softbuffer: `0x00RRGGBB`.

/// Фон вне мира.
pub(crate) const BLACK: u32 = 0x0000_0000;
/// Пустая клетка мира.
pub(crate) const WHITE: u32 = 0x00FF_FFFF;
/// Линии сетки по левой и верхней кромке клетки.
pub(crate) const GRID: u32 = 0x0080_8080;
