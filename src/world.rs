/// Мир — прямоугольник ячеек: x ∈ [0, width), y ∈ [0, height).
///
/// Каждая ячейка хранит имя встроенного изображения (или `None` — пустая
/// белая ячейка). Имя совпадает с именем файла в `img/` без расширения.
#[derive(Debug, Clone)]
pub struct World {
    pub width: u32,
    pub height: u32,
    cells: Vec<Option<&'static str>>,
    rng: Rng,
}

/// Почва: единственное, что есть в мире в начале симуляции.
pub(crate) const EARTH: &str = "earth";
/// Трава, которая постепенно распространяется по почве.
pub(crate) const GRASS: &str = "grass";

const RNG_SEED: u64 = 0x9E37_79B9_7F4A_7C15;

impl World {
    pub fn new(width: u32, height: u32) -> Self {
        let len = width as usize * height as usize;
        Self {
            width,
            height,
            cells: vec![None; len],
            rng: Rng::new(RNG_SEED),
        }
    }

    /// Начальный мир: все ячейки покрыты землёй.
    pub fn starting() -> Self {
        let mut world = Self::new(100, 100);
        for y in 0..world.height {
            for x in 0..world.width {
                world.set_cell(x, y, Some(EARTH));
            }
        }
        world
    }

    pub fn cell(&self, x: u32, y: u32) -> Option<&'static str> {
        self.index(x, y).and_then(|index| self.cells[index])
    }

    /// Сколько клеток мира уже покрыто травой.
    pub fn grass_count(&self) -> usize {
        self.cells
            .iter()
            .filter(|cell| **cell == Some(GRASS))
            .count()
    }

    /// Доля травы в мире, в процентах (0 для пустого мира).
    pub fn grass_percent(&self) -> u64 {
        let grass = self.grass_count() as u64 * 100;
        grass.checked_div(self.cells.len() as u64).unwrap_or(0)
    }

    /// Устанавливает изображение ячейки. Координаты за границами мира
    /// игнорируются.
    pub fn set_cell(&mut self, x: u32, y: u32, image: Option<&'static str>) {
        if let Some(index) = self.index(x, y) {
            self.cells[index] = image;
        }
    }

    /// Один тик симуляции (10 раз в секунду).
    ///
    /// Проверяются все клетки, но изменения применяются к снимку состояния:
    /// трава, появившаяся на этом тике, начнёт заражать соседей только
    /// на следующем.
    pub fn tick(&mut self) {
        let previous = self.cells.clone();

        for y in 0..self.height {
            for x in 0..self.width {
                let index = y as usize * self.width as usize + x as usize;
                if previous[index] != Some(EARTH) {
                    continue;
                }

                let next_to_grass = has_grass_neighbor(&previous, self.width, self.height, x, y);
                let roll_100 = self.rng.roll(100);
                let roll_10 = self.rng.roll(10);

                if earth_becomes_grass(next_to_grass, roll_100, roll_10) {
                    self.cells[index] = Some(GRASS);
                }
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn with_seed(width: u32, height: u32, seed: u64) -> Self {
        let mut world = Self::new(width, height);
        world.rng = Rng::new(seed);
        world
    }

    fn index(&self, x: u32, y: u32) -> Option<usize> {
        if x >= self.width || y >= self.height {
            return None;
        }
        Some(y as usize * self.width as usize + x as usize)
    }
}

impl Default for World {
    fn default() -> Self {
        Self::new(100, 100)
    }
}

/// Правило роста: земля сначала должна пройти проверку 1 к 100. Рядом с травой
/// она становится травой сразу, иначе — только с шансом 1 к 10.
pub(crate) fn earth_becomes_grass(next_to_grass: bool, roll_100: u32, roll_10: u32) -> bool {
    roll_100 == 0 && (next_to_grass || roll_10 == 0)
}

/// Есть ли трава в восьми соседних клетках.
fn has_grass_neighbor(
    cells: &[Option<&'static str>],
    width: u32,
    height: u32,
    x: u32,
    y: u32,
) -> bool {
    let x_from = x.saturating_sub(1);
    let x_to = (x + 1).min(width - 1);
    let y_from = y.saturating_sub(1);
    let y_to = (y + 1).min(height - 1);

    for ny in y_from..=y_to {
        for nx in x_from..=x_to {
            if (nx, ny) == (x, y) {
                continue;
            }
            if cells[ny as usize * width as usize + nx as usize] == Some(GRASS) {
                return true;
            }
        }
    }

    false
}

/// Детерминированный xorshift64*, чтобы симуляция не зависела от внешних крейтов.
#[derive(Debug, Clone)]
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(if seed == 0 { 1 } else { seed })
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Равномерное число из `0..sides`.
    fn roll(&mut self, sides: u32) -> u32 {
        (self.next_u64() % sides as u64) as u32
    }
}
