//! Регрессионные тесты по ошибкам, допущенным в этой сессии.
//!
//! Карта «ошибка → тест»:
//!
//! 1. Неотформатированный код (`cargo fmt --check` падал) — `lints_and_format_are_clean`
//!    (запускается вручную, см. `#[ignore]`).
//! 2. `clippy::collapsible_match` в обработке Esc — тот же `lints_and_format_are_clean`.
//! 3. `clippy::collapsible_if` в обработке событий камеры — тот же тест.
//! 4. `dead_code`: поля `Frame::width`/`height` никто не читал —
//!    `frame_reports_requested_dimensions`.
//! 5. `dead_code`: `Camera::new` нигде не использовался — `camera_new_matches_spec`.
//! 6. Неверный тест «вне мира всё чёрное»: не учитывал, что часть кадра попадает
//!    внутрь мира, — `outside_world_is_black` и `world_bounds_extend_half_cell`.
//! 7. Ошибка компиляции теста при конструировании `KeyEvent`
//!    (`physical_key: None`, `platform_specific: Default::default()`) — клавиши
//!    проверяются через `wasd_mapping`, без платформенных событий.
//! 8. Камера в (0, 0) смотрела на стык четырёх ячеек вместо центра ячейки —
//!    `camera_origin_looks_at_cell_center`.
//! 9. Лишние перерисовки от событий (перебор FPS) — `irrelevant_events_do_not_trigger_redraw`.

use crate::camera::{BLACK, Camera, GRID, PAN_STEP, WHITE, pan_delta_for_key};
use crate::images::{EMBEDDED, Image, Images, blend_on_white};
use crate::logging::{Level, format_line, level_from_args};
use crate::tiles::{MAX_LEVEL, MIN_LEVEL, Tiles, level_scale};
use crate::window::{FRAMES_PER_TICK, FpsCounter, TickClock, format_title, grass_milestone};
use crate::world::{EARTH, GRASS, World, earth_becomes_grass};
use std::time::{Duration, Instant};
use winit::dpi::PhysicalPosition;
use winit::event::{DeviceId, MouseScrollDelta, TouchPhase, WindowEvent};

fn camera_at(center: (i64, i64), level: u8) -> Camera {
    Camera { center, level }
}

fn no_images() -> Images {
    Images::from_images(Vec::new())
}

fn no_tiles() -> Tiles {
    Tiles::build(&no_images())
}

#[test]
fn camera_new_matches_spec() {
    let camera = Camera::new();
    assert_eq!(camera.center, (0, 0));
    assert_eq!(camera.level, 5);
    assert_eq!(camera.scale(), 32.0);

    let default = Camera::default();
    assert_eq!(default.center, camera.center);
    assert_eq!(default.level, camera.level);
}

#[test]
fn frame_reports_requested_dimensions() {
    // Ошибка 4: размеры кадра лежали в `Frame`, но не читались.
    let frame = Camera::new().render(&World::default(), &no_tiles(), 37, 11);

    assert_eq!((frame.width, frame.height), (37, 11));
    assert_eq!(frame.pixels.len(), 37 * 11);
}

#[test]
fn camera_origin_looks_at_cell_center() {
    // Ошибка 8: в (0, 0) камера показывала перекрестие рамок четырёх ячеек.
    let frame = Camera::default().render(&World::default(), &no_tiles(), 50, 50);

    assert_eq!(frame.pixels[24 * 50 + 24], WHITE);
    assert_eq!(frame.pixels[25 * 50 + 25], WHITE);
}

#[test]
fn no_grid_is_drawn_at_small_scales() {
    // Уровни 0–3 (1, 2, 4 и 8 px на клетку) рисуются без сетки.
    for level in 0..=3 {
        let camera = camera_at((0, 0), level);
        let frame = camera.render(&World::new(1, 1), &no_tiles(), 16, 16);

        assert!(
            frame.pixels.iter().all(|&pixel| pixel != GRID),
            "level {level} (scale {}) не должен содержать сетку",
            level_scale(level)
        );
    }
}

#[test]
fn grid_is_drawn_at_large_scales() {
    let camera = camera_at((0, 0), 4); // scale 16
    let frame = camera.render(&World::new(1, 1), &no_tiles(), 16, 16);

    // Левая и верхняя кромки ячейки — ровно 1 px, дальше белое.
    for x in 0..16 {
        assert_eq!(frame.pixels[x], GRID, "верхняя кромка, x = {x}");
        if x > 0 {
            assert_eq!(frame.pixels[2 * 16 + x], WHITE, "строка 2, x = {x}");
        }
    }
    for y in 0..16 {
        assert_eq!(frame.pixels[y * 16], GRID, "левая кромка, y = {y}");
        if y > 0 {
            assert_eq!(frame.pixels[y * 16 + 2], WHITE, "столбец 2, y = {y}");
        }
    }
}

#[test]
fn world_bounds_are_one_cell_wide() {
    // Ошибка 6: границы мира проверялись как 0..width вместо -0.5..width-0.5.
    // Мир 1×1 при масштабе 2 занимает 2×2 пикселя вокруг центра клетки (0, 0).
    let world = World::new(1, 1);
    let inside = camera_at((0, 0), 1).render(&world, &no_tiles(), 2, 2);
    assert_eq!(inside.pixels[0], WHITE);
    assert_eq!(inside.pixels[1], WHITE);
    assert_eq!(inside.pixels[2], WHITE);
    assert_eq!(inside.pixels[3], WHITE);

    for center in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        let frame = camera_at(center, 1).render(&world, &no_tiles(), 2, 2);
        assert!(
            frame.pixels.iter().all(|&pixel| pixel == BLACK),
            "центр {center:?} уже вне мира 1×1"
        );
    }
}

#[test]
fn outside_world_is_black() {
    // Ошибка 6: предыдущая версия теста ошибочно ждала чёрный цвет там,
    // где кадр ещё попадает в мир. Здесь проверяются и мир, и его окрестности.
    let camera = camera_at((0, 0), 2); // scale 4
    let frame = camera.render(&World::new(1, 1), &no_tiles(), 8, 8);

    // Ячейка (0, 0) занимает центральные 4×4 пикселя.
    for py in 0..8 {
        for px in 0..8 {
            let inside = (2..6).contains(&px) && (2..6).contains(&py);
            if !inside {
                assert_eq!(
                    frame.pixels[py * 8 + px],
                    BLACK,
                    "пиксель ({px}, {py}) должен быть чёрным"
                );
            }
        }
    }
    assert_eq!(frame.pixels[4 * 8 + 4], WHITE);
}

#[test]
fn empty_world_is_all_black() {
    let frame = Camera::new().render(&World::new(0, 0), &no_tiles(), 8, 8);

    assert!(frame.pixels.iter().all(|&pixel| pixel == BLACK));
}

#[test]
fn wasd_mapping() {
    // Ошибка 7: тест конструировал `KeyEvent` с платформенными полями.
    // Проверяем маппинг клавиш без winit-событий.
    assert_eq!(pan_delta_for_key("w"), Some((0, -PAN_STEP)));
    assert_eq!(pan_delta_for_key("W"), Some((0, -PAN_STEP)));
    assert_eq!(pan_delta_for_key("s"), Some((0, PAN_STEP)));
    assert_eq!(pan_delta_for_key("a"), Some((-PAN_STEP, 0)));
    assert_eq!(pan_delta_for_key("d"), Some((PAN_STEP, 0)));
    assert_eq!(pan_delta_for_key("D"), Some((PAN_STEP, 0)));

    assert!(pan_delta_for_key("q").is_none());
    assert!(pan_delta_for_key("wd").is_none());

    // Сумма всех направлений даёт ноль — центр не уезжает от нажатий.
    let mut camera = Camera::default();
    for key in ["w", "a", "s", "d"] {
        let (dx, dy) = pan_delta_for_key(key).unwrap();
        camera.pan(dx, dy);
    }
    assert_eq!(camera.center, (0, 0));
}

#[test]
fn wheel_zooms_by_powers_of_two_and_clamps() {
    let mut camera = Camera::default();
    let start = camera.level;

    assert!(camera.handle_event(&wheel_event(MouseScrollDelta::LineDelta(0.0, 1.0))));
    assert_eq!(camera.level, start + 1);
    assert_eq!(camera.scale(), 64.0);

    assert!(!camera.handle_event(&wheel_event(MouseScrollDelta::LineDelta(0.0, 0.0))));
    assert_eq!(camera.level, start + 1);

    assert!(camera.handle_event(&wheel_event(MouseScrollDelta::LineDelta(0.0, 1000.0))));
    assert_eq!(camera.level, MAX_LEVEL);
    assert_eq!(camera.scale(), 128.0);

    assert!(camera.handle_event(&wheel_event(MouseScrollDelta::LineDelta(0.0, -1000.0))));
    assert_eq!(camera.level, MIN_LEVEL);
    assert_eq!(camera.scale(), 1.0);
}

#[test]
fn pixel_delta_wheel_zooms() {
    // Touchpad-события приходят в пикселях, а не в «ступеньках».
    let mut camera = Camera::default();
    let start = camera.level;
    let event = wheel_event(MouseScrollDelta::PixelDelta(PhysicalPosition::new(
        0.0, 50.0,
    )));

    assert!(camera.handle_event(&event));
    assert_eq!(camera.level, start + 1);
    assert_eq!(camera.scale(), 64.0);
}

#[test]
fn title_shows_metrics_separated_by_bars() {
    let camera = camera_at((1, -2), 5);
    let title = format_title(59.6, 0.42, &camera, 42);

    assert_eq!(
        title,
        "ug0 | 60 fps | frame: 0.42 ms | zoom: 32 px/cell | x: 1, y: -2 | ticks: 42"
    );
    assert_eq!(title.matches('|').count(), 5);
    assert!(title.starts_with("ug0 | "));
}

#[test]
fn fps_is_averaged_over_last_second() {
    let base = Instant::now();
    let mut fps = FpsCounter::new(base);
    let frame_time = Duration::from_millis(2);

    // Кадры в течение первых 900 мс: окно ещё не закрыто.
    for step in 0..10 {
        fps.tick(base + Duration::from_millis(step * 100), frame_time);
    }
    assert_eq!(fps.fps(), 0.0);
    assert_eq!(fps.frame_ms(), 0.0);

    // 11-й кадр замыкает окно длиной ровно 1 с.
    fps.tick(base + Duration::from_secs(1), frame_time);
    assert!((fps.fps() - 11.0).abs() < 1e-9, "fps = {}", fps.fps());
    assert!(
        (fps.frame_ms() - 2.0).abs() < 1e-9,
        "frame_ms = {}",
        fps.frame_ms()
    );

    // Следующая секунда считается заново, а не накопительно.
    for step in 1..=10 {
        fps.tick(
            base + Duration::from_millis(1000 + step * 100),
            Duration::from_millis(4),
        );
    }
    assert!((fps.fps() - 10.0).abs() < 1e-9, "fps = {}", fps.fps());
    assert!(
        (fps.frame_ms() - 4.0).abs() < 1e-9,
        "frame_ms = {}",
        fps.frame_ms()
    );
}

#[test]
fn irrelevant_events_do_not_trigger_redraw() {
    // Ошибка 9: события, не относящиеся к камере, провоцировали лишний кадр.
    let mut camera = Camera::default();
    let before = (camera.center, camera.level);

    let events = [
        WindowEvent::Focused(true),
        WindowEvent::Focused(false),
        WindowEvent::CursorMoved {
            device_id: DeviceId::dummy(),
            position: PhysicalPosition::new(3.0, 4.0),
        },
        WindowEvent::Resized(winit::dpi::PhysicalSize::new(800, 600)),
    ];

    for event in events {
        assert!(
            !camera.handle_event(&event),
            "событие {event:?} не должно менять камеру"
        );
    }

    assert_eq!((camera.center, camera.level), before);
}

fn wheel_event(delta: MouseScrollDelta) -> WindowEvent {
    WindowEvent::MouseWheel {
        device_id: DeviceId::dummy(),
        delta,
        phase: TouchPhase::Moved,
    }
}

const RED: u32 = 0x00FF_0000;
const GREEN: u32 = 0x0000_FF00;
const BLUE: u32 = 0x0000_00FF;

#[test]
fn embedded_images_decode_and_lookup() {
    // Картинки действительно вшиты в бинарник и декодируются при старте.
    let images = Images::load().expect("встроенные изображения должны декодироваться");

    for &(name, bytes) in EMBEDDED {
        assert!(
            bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
            "{name}: нет PNG-сигнатуры"
        );

        let image = images
            .get(name)
            .unwrap_or_else(|| panic!("{name}: картинка не найдена в Images"));
        assert!(
            image.width > 0 && image.height > 0,
            "{name}: пустая картинка"
        );
        assert_eq!(
            image.pixels.len(),
            image.width as usize * image.height as usize,
            "{name}: неверное число пикселей"
        );
    }

    assert!(images.get("missing").is_none());
}

#[test]
fn every_img_file_is_embedded() {
    // Новый файл в img/ без записи в EMBEDDED не попадёт в бинарник —
    // этот тест ловит такую забывчивость.
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("img");
    let mut files: Vec<String> = std::fs::read_dir(&dir)
        .expect("не удалось прочитать папку img")
        .map(|entry| entry.expect("не удалось прочитать запись папки img").path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("png"))
        .map(|path| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .expect("имя файла img должно быть UTF-8")
                .to_owned()
        })
        .collect();
    files.sort();

    let mut embedded: Vec<String> = EMBEDDED.iter().map(|&(name, _)| name.to_owned()).collect();
    embedded.sort();

    assert_eq!(files, embedded, "список EMBEDDED разошёлся с папкой img/");
}

#[test]
fn world_cells_store_image_names() {
    let mut world = World::new(2, 2);
    assert_eq!(world.cell(0, 0), None);

    world.set_cell(1, 1, Some("grass"));
    assert_eq!(world.cell(1, 1), Some("grass"));
    assert_eq!(world.cell(0, 1), None);

    // За границами мира запись игнорируется.
    world.set_cell(2, 0, Some("stone"));
    assert_eq!(world.cell(2, 0), None);

    world.set_cell(1, 1, None);
    assert_eq!(world.cell(1, 1), None);
}

#[test]
fn initial_world_is_only_earth() {
    let images = Images::load().expect("встроенные изображения должны декодироваться");
    let world = World::starting();

    assert_eq!((world.width, world.height), (100, 100));
    for y in 0..world.height {
        for x in 0..world.width {
            let name = world
                .cell(x, y)
                .unwrap_or_else(|| panic!("ячейка ({x}, {y}) пустая"));
            assert_eq!(name, EARTH, "в начале мира должна быть только земля");
            assert!(
                images.get(name).is_some(),
                "ячейка ({x}, {y}) ссылается на неизвестную картинку {name}"
            );
        }
    }

    assert!(images.get("grass").is_some(), "трава должна быть вшита");
}

#[test]
fn camera_draws_images_into_cells() {
    // Картинка 2×2 растягивается на ячейку 4×4: каждый её пиксель — 2×2 экрана.
    let images = Images::from_images(vec![(
        "test",
        Image::new(2, 2, vec![RED, GREEN, BLUE, WHITE]),
    )]);
    let mut world = World::new(1, 1);
    world.set_cell(0, 0, Some("test"));

    let tiles = Tiles::build(&images);
    let frame = camera_at((0, 0), 2).render(&world, &tiles, 4, 4);

    assert_eq!(frame.pixels[4 + 1], RED);
    assert_eq!(frame.pixels[4 + 2], GREEN);
    assert_eq!(frame.pixels[2 * 4 + 1], BLUE);
    assert_eq!(frame.pixels[2 * 4 + 2], WHITE);
}

#[test]
fn image_stretches_over_whole_cell() {
    let color = 0x0012_3456;
    let images = Images::from_images(vec![("solid", Image::new(1, 1, vec![color]))]);
    let mut world = World::new(1, 1);
    world.set_cell(0, 0, Some("solid"));

    // Ячейка (0, 0) при scale 4 занимает px 2..6; рамка — левая/верхняя кромка.
    let tiles = Tiles::build(&images);
    let frame = camera_at((0, 0), 2).render(&world, &tiles, 8, 8);
    for py in 3..6 {
        for px in 3..6 {
            assert_eq!(
                frame.pixels[py * 8 + px],
                color,
                "пиксель ({px}, {py}) должен быть цветом картинки"
            );
        }
    }
}

#[test]
fn image_is_not_covered_by_grid() {
    let images = Images::from_images(vec![("solid", Image::new(1, 1, vec![RED]))]);
    let mut world = World::new(1, 1);
    world.set_cell(0, 0, Some("solid"));

    let tiles = Tiles::build(&images);
    let frame = camera_at((0, 0), 2).render(&world, &tiles, 4, 4);

    // Вся клетка 4×4 — цвет картинки, без линий сетки.
    assert!(frame.pixels.iter().all(|&pixel| pixel == RED));
}

#[test]
fn unknown_image_name_renders_empty_cell() {
    let mut world = World::new(1, 1);
    world.set_cell(0, 0, Some("missing"));

    let frame = camera_at((0, 0), 2).render(&world, &no_tiles(), 4, 4);

    assert_eq!(frame.pixels[4 + 1], WHITE);
}

#[test]
fn alpha_is_blended_over_white() {
    assert_eq!(blend_on_white(0, 0, 0, 0), WHITE);
    assert_eq!(blend_on_white(0, 0, 0, 255), 0x0000_0000);
    assert_eq!(blend_on_white(0, 0, 0, 128), 0x007F_7F7F);
    assert_eq!(blend_on_white(255, 0, 0, 128), 0x00FF_7F7F);
}

#[test]
fn tick_clock_ticks_every_six_frames() {
    assert_eq!(FRAMES_PER_TICK, 6);

    let mut clock = TickClock::new();
    for _ in 0..5 {
        assert!(!clock.advance(), "первые 5 кадров тика быть не должно");
    }
    assert_eq!(clock.ticks(), 0);
    assert_eq!(clock.frames(), 5);

    assert!(clock.advance(), "шестой кадр даёт тик");
    assert_eq!(clock.ticks(), 1);
    assert_eq!(clock.frames(), 6);

    for _ in 0..60 {
        clock.advance();
    }
    assert_eq!(clock.ticks(), 11, "60 кадров — ещё 10 тиков");
    assert_eq!(clock.frames(), 66);
}

#[test]
fn earth_growth_rule_uses_nested_chances() {
    // Первая проверка 1 к 100 не прошла — клетка не меняется.
    for roll_100 in 1..100 {
        for next_to_grass in [false, true] {
            for roll_10 in 0..10 {
                assert!(!earth_becomes_grass(next_to_grass, roll_100, roll_10));
            }
        }
    }

    // Проверка прошла, рядом трава — заражаемся всегда.
    for roll_10 in 0..10 {
        assert!(earth_becomes_grass(true, 0, roll_10));
    }

    // Проверка прошла, травы рядом нет — только 1 к 10.
    for roll_10 in 1..10 {
        assert!(!earth_becomes_grass(false, 0, roll_10));
    }
    assert!(earth_becomes_grass(false, 0, 0));
}

#[test]
fn simulation_spreads_grass_over_whole_world() {
    let mut world = World::with_seed(7, 7, 7);
    for y in 0..world.height {
        for x in 0..world.width {
            world.set_cell(x, y, Some(EARTH));
        }
    }
    world.set_cell(3, 3, Some(GRASS));

    let mut ticks = 0;
    while (0..world.height).any(|y| (0..world.width).any(|x| world.cell(x, y) != Some(GRASS))) {
        world.tick();
        ticks += 1;
        assert!(
            ticks < 100_000,
            "трава не распространилась за 100 000 тиков"
        );
    }

    for y in 0..world.height {
        for x in 0..world.width {
            assert_eq!(world.cell(x, y), Some(GRASS), "ячейка ({x}, {y})");
        }
    }
}

#[test]
fn bare_earth_eventually_grows_grass_spontaneously() {
    let mut world = World::with_seed(10, 10, 2);
    for y in 0..world.height {
        for x in 0..world.width {
            world.set_cell(x, y, Some(EARTH));
        }
    }

    for _ in 0..1000 {
        world.tick();
    }

    let has_grass =
        (0..world.height).any(|y| (0..world.width).any(|x| world.cell(x, y) == Some(GRASS)));
    assert!(has_grass, "без травы-соседа трава должна появляться сама");
}

#[test]
fn simulation_ignores_empty_cells() {
    let mut world = World::new(4, 4);
    for _ in 0..1000 {
        world.tick();
    }

    for y in 0..world.height {
        for x in 0..world.width {
            assert_eq!(
                world.cell(x, y),
                None,
                "ячейка ({x}, {y}) должна остаться пустой"
            );
        }
    }
}

/// Охватывает ошибки 1–3 (форматирование и линты), которые нельзя проверить
/// юнит-тестом иначе как запуском самих инструментов.
///
/// `cargo test -- --ignored lints_and_format_are_clean`
#[test]
#[ignore = "запускает cargo fmt/clippy; выполняется отдельно"]
fn lints_and_format_are_clean() {
    use std::process::Command;

    let fmt = Command::new("cargo")
        .args(["fmt", "--check"])
        .output()
        .expect("не удалось запустить cargo fmt");
    assert!(
        fmt.status.success(),
        "код не отформатирован:\n{}",
        String::from_utf8_lossy(&fmt.stderr)
    );

    // Отдельный target dir, чтобы не конфликтовать с текущим `cargo test`.
    let target_dir = std::env::temp_dir().join("ug0-lint-check");
    let clippy = Command::new("cargo")
        .args(["clippy", "--all-targets", "--", "-D", "warnings"])
        .env("CARGO_TARGET_DIR", &target_dir)
        .output()
        .expect("не удалось запустить cargo clippy");
    assert!(
        clippy.status.success(),
        "clippy нашёл проблемы:\n{}",
        String::from_utf8_lossy(&clippy.stderr)
    );
}

#[test]
fn log_level_parsing() {
    assert_eq!(Level::parse("off"), Some(Level::Off));
    assert_eq!(Level::parse("error"), Some(Level::Error));
    assert_eq!(Level::parse("warn"), Some(Level::Warn));
    assert_eq!(Level::parse("info"), Some(Level::Info));
    assert_eq!(Level::parse("debug"), Some(Level::Debug));
    assert_eq!(Level::parse("DEBUG"), Some(Level::Debug));
    assert_eq!(Level::parse("verbose"), None);
    assert_eq!(Level::parse(""), None);
}

#[test]
fn log_level_filtering() {
    assert!(Level::Debug.allows(Level::Debug));
    assert!(Level::Debug.allows(Level::Error));
    assert!(Level::Info.allows(Level::Warn));
    assert!(!Level::Info.allows(Level::Debug));
    assert!(Level::Error.allows(Level::Error));
    assert!(!Level::Error.allows(Level::Info));
    assert!(!Level::Off.allows(Level::Error));
}

#[test]
fn log_line_has_timestamp_level_and_message() {
    assert_eq!(
        format_line(Level::Info, Duration::from_millis(12_345), "hello"),
        "[  12.345s INFO ] hello"
    );
    assert_eq!(
        format_line(Level::Error, Duration::ZERO, "boom"),
        "[   0.000s ERROR] boom"
    );
}

fn log_args(args: &[&str]) -> Vec<String> {
    args.iter().map(|arg| (*arg).to_owned()).collect()
}

#[test]
fn log_level_from_args() {
    assert_eq!(level_from_args(log_args(&[])), Ok(Level::Debug));
    assert_eq!(
        level_from_args(log_args(&["--loglevel=debug"])),
        Ok(Level::Debug)
    );
    assert_eq!(
        level_from_args(log_args(&["--loglevel", "warn"])),
        Ok(Level::Warn)
    );
    assert_eq!(
        level_from_args(log_args(&["--LOGLEVEL=off"])),
        Ok(Level::Debug),
        "флаг чувствителен к регистру, опечатка игнорируется"
    );
    assert_eq!(
        level_from_args(log_args(&["--foo", "--loglevel=error"])),
        Ok(Level::Error),
        "лишние аргументы игнорируются"
    );

    let err = level_from_args(log_args(&["--loglevel=verbose"])).unwrap_err();
    assert!(err.contains("invalid log level \"verbose\""), "{err}");

    let err = level_from_args(log_args(&["--loglevel"])).unwrap_err();
    assert!(err.contains("requires a value"), "{err}");
}

#[test]
fn grass_coverage_is_counted_in_percent() {
    let mut world = World::new(2, 2);
    assert_eq!(world.grass_count(), 0);
    assert_eq!(world.grass_percent(), 0);

    world.set_cell(0, 0, Some(GRASS));
    assert_eq!(world.grass_count(), 1);
    assert_eq!(world.grass_percent(), 25);

    world.set_cell(1, 0, Some(GRASS));
    assert_eq!(world.grass_percent(), 50);

    world.set_cell(0, 1, Some(GRASS));
    assert_eq!(world.grass_percent(), 75);

    world.set_cell(1, 1, Some(GRASS));
    assert_eq!(world.grass_percent(), 100);

    assert_eq!(World::new(0, 0).grass_percent(), 0);
}

#[test]
fn grass_milestones_are_reported_once() {
    assert_eq!(grass_milestone(0, 0), None);
    assert_eq!(grass_milestone(0, 49), None);
    assert_eq!(grass_milestone(0, 50), Some(50));
    assert_eq!(grass_milestone(50, 50), None);
    assert_eq!(grass_milestone(50, 99), None);
    assert_eq!(grass_milestone(50, 100), Some(100));
    assert_eq!(grass_milestone(100, 100), None);
    assert_eq!(grass_milestone(0, 100), Some(100));
}

/// Эталон: старый попиксельный рендер с `f64`-координатами.
/// Нужен только тестам, чтобы сравнивать с новым клеточным рендером.
fn reference_render(
    camera: &Camera,
    world: &World,
    images: &Images,
    width: u32,
    height: u32,
) -> Vec<u32> {
    let mut pixels = vec![BLACK; width as usize * height as usize];
    let (cx, cy) = (camera.center.0 as f64, camera.center.1 as f64);
    let scale = camera.scale();
    let inv_scale = 1.0 / scale;
    let half_width = width as f64 / 2.0;
    let half_height = height as f64 / 2.0;
    let min = -0.5;
    let max_x = world.width as f64 - 0.5;
    let max_y = world.height as f64 - 0.5;

    let grid_enabled = camera.level >= crate::camera::GRID_MIN_LEVEL;
    let mut cached_cell = (u32::MAX, u32::MAX);
    let mut cached_image: Option<&Image> = None;

    for py in 0..height {
        let wy = cy + (py as f64 + 0.5 - half_height) * inv_scale;
        if wy < min || wy >= max_y {
            continue;
        }

        let cell_y = (wy + 0.5).floor() as u32;
        let fy = ((wy + 0.5) - cell_y as f64) * scale;
        let grid_row = grid_enabled && fy < 1.0;
        let mut wx = cx + (0.5 - half_width) * inv_scale;

        for px in 0..width {
            if wx >= min && wx < max_x {
                let cell_x = (wx + 0.5).floor() as u32;

                if (cell_x, cell_y) != cached_cell {
                    cached_cell = (cell_x, cell_y);
                    cached_image = world.cell(cell_x, cell_y).and_then(|name| images.get(name));
                }

                let fx = ((wx + 0.5) - cell_x as f64) * scale;
                let color = match cached_image {
                    Some(image) => {
                        let u = ((fx * inv_scale) * image.width as f64) as u32;
                        let v = ((fy * inv_scale) * image.height as f64) as u32;
                        let u = u.min(image.width.saturating_sub(1));
                        let v = v.min(image.height.saturating_sub(1));
                        image.pixels[v as usize * image.width as usize + u as usize]
                    }
                    None => WHITE,
                };

                pixels[(py * width + px) as usize] = if (grid_enabled && fx < 1.0) || grid_row {
                    GRID
                } else {
                    color
                };
            }

            wx += inv_scale;
        }
    }

    pixels
}

fn patterned_images() -> Images {
    Images::from_images(vec![
        (
            "a",
            Image::new(
                3,
                2,
                vec![RED, GREEN, BLUE, WHITE, 0x0011_2233, 0x00AA_BBCC],
            ),
        ),
        ("b", Image::new(2, 2, vec![WHITE, RED, 0x0000_0000, GREEN])),
    ])
}

#[test]
fn tiles_are_built_for_all_levels() {
    let images = Images::load().expect("встроенные изображения должны декодироваться");
    let tiles = Tiles::build(&images);
    assert_eq!(tiles.level_count(), usize::from(MAX_LEVEL) + 1);

    for &(name, _) in EMBEDDED {
        for level in MIN_LEVEL..=MAX_LEVEL {
            let scale = level_scale(level) as usize;
            let tile = tiles
                .get(name, level)
                .unwrap_or_else(|| panic!("{name}: нет тайла уровня {level}"));
            assert_eq!((tile.width as usize, tile.height as usize), (scale, scale));
            assert_eq!(tile.pixels.len(), scale * scale);

            let empty = tiles.empty(level);
            assert_eq!(
                (empty.width as usize, empty.height as usize),
                (scale, scale)
            );
            assert!(empty.pixels.iter().all(|&pixel| pixel == WHITE));
        }
    }

    assert!(tiles.get("missing", 0).is_none());
}

#[test]
fn tile_sampling_is_nearest_neighbor() {
    // Картинка 2×2 на тайле 4×4: каждый исходный пиксель занимает 2×2 тайла.
    let images = Images::from_images(vec![(
        "test",
        Image::new(2, 2, vec![RED, GREEN, BLUE, WHITE]),
    )]);
    let tiles = Tiles::build(&images);
    let tile = tiles.get("test", 2).expect("тайл 4×4");

    let expected = vec![
        RED, RED, GREEN, GREEN, //
        RED, RED, GREEN, GREEN, //
        BLUE, BLUE, WHITE, WHITE, //
        BLUE, BLUE, WHITE, WHITE,
    ];
    assert_eq!(tile.pixels, expected);
}

#[test]
fn tiles_have_no_baked_grid() {
    // Исходная картинка 8×8 на уровне 8×8 должна попасть в тайл один к одному,
    // без серых линий сетки.
    let mut source = Vec::new();
    for y in 0..8u32 {
        for x in 0..8u32 {
            source.push(0x0001_0000 * y + x + 1);
        }
    }
    let images = Images::from_images(vec![("test", Image::new(8, 8, source.clone()))]);
    let tiles = Tiles::build(&images);
    let tile = tiles.get("test", 3).expect("тайл 8×8");

    assert_eq!(tile.pixels, source, "тайл должен совпасть с картинкой");
    assert!(
        !tile.pixels.contains(&GRID),
        "сетка не должна запекаться в тайл"
    );
}

#[test]
fn render_matches_reference_renderer() {
    let images = patterned_images();
    let tiles = Tiles::build(&images);
    let mut world = World::new(3, 3);
    world.set_cell(0, 0, Some("a"));
    world.set_cell(1, 0, Some("b"));
    world.set_cell(0, 2, Some("a"));
    world.set_cell(2, 2, Some("missing"));

    for level in MIN_LEVEL..=MAX_LEVEL {
        for center in [(0, 0), (1, 2), (-2, 1), (3, -1)] {
            let camera = camera_at(center, level);
            for (width, height) in [(16, 12), (8, 6)] {
                let expected = reference_render(&camera, &world, &images, width, height);
                let actual = camera.render(&world, &tiles, width, height).pixels;
                assert_eq!(
                    actual, expected,
                    "level {level}, center {center:?}, {width}×{height}"
                );
            }
        }
    }
}

#[test]
fn odd_window_keeps_center_cell_centered() {
    // Нечётное окно 101: геометрический центр — 50.5, клетка 8 px
    // начинается с 46, то есть её центр ровно на 50 — отклонение 0.5 px.
    let camera = camera_at((0, 0), 3);
    let frame = camera.render(&World::new(1, 1), &no_tiles(), 101, 101);

    assert_eq!(frame.pixels[46 * 101 + 50], WHITE, "верхний край клетки");
    assert_eq!(frame.pixels[45 * 101 + 50], BLACK, "выше клетки — фон");
    assert_eq!(frame.pixels[50 * 101 + 46], WHITE, "левый край клетки");
    assert_eq!(frame.pixels[50 * 101 + 45], BLACK, "левее клетки — фон");
}

#[test]
fn odd_window_has_no_gaps_between_tiles() {
    let mut world = World::new(64, 64);
    for y in 0..world.height {
        for x in 0..world.width {
            world.set_cell(x, y, Some(EARTH));
        }
    }
    let images = Images::load().expect("встроенные изображения должны декодироваться");
    let tiles = Tiles::build(&images);
    let frame = camera_at((32, 32), 3).render(&world, &tiles, 101, 77);

    assert!(
        frame.pixels.iter().all(|&pixel| pixel != BLACK),
        "между тайлами не должно быть чёрных щелей"
    );
}

#[test]
#[ignore = "измеряет скорость рендера; запускать в release"]
fn render_throughput() {
    use std::hint::black_box;

    let images = Images::load().expect("встроенные изображения должны декодироваться");
    let tiles = Tiles::build(&images);
    let world = World::starting();

    for level in [0, 4, 7] {
        let camera = camera_at((4, 4), level);
        let runs = 60;

        // Прогрев.
        black_box(camera.render(&world, &tiles, 800, 600));

        let start = Instant::now();
        for _ in 0..runs {
            black_box(camera.render(&world, &tiles, 800, 600));
        }
        let avg = start.elapsed().as_secs_f64() * 1000.0 / f64::from(runs);

        println!(
            "level {level} (scale {:>3}): render {avg:.3} ms",
            level_scale(level)
        );
    }
}
