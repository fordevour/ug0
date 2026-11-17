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

use crate::camera::{
    BLACK, Camera, GRID, MAX_SCALE, MIN_SCALE, PAN_STEP, WHITE, ZOOM_STEP, pan_delta_for_key,
};
use crate::window::{FpsCounter, format_title};
use crate::world::World;
use std::time::{Duration, Instant};
use winit::dpi::PhysicalPosition;
use winit::event::{DeviceId, MouseScrollDelta, TouchPhase, WindowEvent};

fn camera_at(center: (f64, f64), scale: f64) -> Camera {
    Camera { center, scale }
}

/// Цвет единственного пикселя кадра 1×1 — удобно проверять точку мира.
fn pixel_at(camera: &Camera, world: &World) -> u32 {
    camera.render(world, 1, 1).pixels[0]
}

#[test]
fn camera_new_matches_spec() {
    // Ошибка 5: `Camera::new` был мёртвым кодом.
    let camera = Camera::new();
    assert_eq!(camera.center, (0.0, 0.0));
    assert_eq!(camera.scale, 25.0);

    let default = Camera::default();
    assert_eq!(default.center, camera.center);
    assert_eq!(default.scale, camera.scale);
}

#[test]
fn frame_reports_requested_dimensions() {
    // Ошибка 4: размеры кадра лежали в `Frame`, но не читались.
    let frame = Camera::new().render(&World::default(), 37, 11);

    assert_eq!((frame.width, frame.height), (37, 11));
    assert_eq!(frame.pixels.len(), 37 * 11);
}

#[test]
fn camera_origin_looks_at_cell_center() {
    // Ошибка 8: в (0, 0) камера показывала перекрестие рамок четырёх ячеек.
    let frame = Camera::default().render(&World::default(), 50, 50);

    assert_eq!(frame.pixels[24 * 50 + 24], WHITE);
    assert_eq!(frame.pixels[25 * 50 + 25], WHITE);
}

#[test]
fn grid_border_is_exactly_one_pixel() {
    let camera = Camera::default(); // scale 25
    let frame = camera.render(&World::new(1, 1), 25, 25);

    // Левая и верхняя кромки ячейки — ровно 1 px, дальше белое.
    assert_eq!(frame.pixels[5 * 25], GRID);
    assert_eq!(frame.pixels[5 * 25 + 1], WHITE);
    assert_eq!(frame.pixels[5], GRID);
    assert_eq!(frame.pixels[25 + 6], WHITE);
    assert_eq!(frame.pixels[5 * 25 + 5], WHITE);
}

#[test]
fn world_bounds_extend_half_cell() {
    // Ошибка 6: границы мира проверялись как 0..width вместо -0.5..width-0.5.
    let world = World::new(1, 1);
    let scale = 10.0;

    assert_eq!(pixel_at(&camera_at((0.0, 0.0), scale), &world), WHITE);
    assert_eq!(pixel_at(&camera_at((0.3, 0.3), scale), &world), WHITE);
    assert_eq!(pixel_at(&camera_at((-0.3, -0.3), scale), &world), WHITE);

    assert_eq!(pixel_at(&camera_at((0.6, 0.6), scale), &world), BLACK);
    assert_eq!(pixel_at(&camera_at((-0.6, -0.6), scale), &world), BLACK);
}

#[test]
fn outside_world_is_black() {
    // Ошибка 6: предыдущая версия теста ошибочно ждала чёрный цвет там,
    // где кадр ещё попадает в мир. Здесь проверяются и мир, и его окрестности.
    let camera = camera_at((0.0, 0.0), 4.0);
    let frame = camera.render(&World::new(1, 1), 8, 8);

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
    let frame = Camera::new().render(&World::new(0, 0), 8, 8);

    assert!(frame.pixels.iter().all(|&pixel| pixel == BLACK));
}

#[test]
fn wasd_mapping() {
    // Ошибка 7: тест конструировал `KeyEvent` с платформенными полями.
    // Проверяем маппинг клавиш без winit-событий.
    assert_eq!(pan_delta_for_key("w"), Some((0.0, -PAN_STEP)));
    assert_eq!(pan_delta_for_key("W"), Some((0.0, -PAN_STEP)));
    assert_eq!(pan_delta_for_key("s"), Some((0.0, PAN_STEP)));
    assert_eq!(pan_delta_for_key("a"), Some((-PAN_STEP, 0.0)));
    assert_eq!(pan_delta_for_key("d"), Some((PAN_STEP, 0.0)));
    assert_eq!(pan_delta_for_key("D"), Some((PAN_STEP, 0.0)));

    assert!(pan_delta_for_key("q").is_none());
    assert!(pan_delta_for_key("wd").is_none());

    // Сумма всех направлений даёт ноль — центр не уезжает от нажатий.
    let mut camera = Camera::default();
    for key in ["w", "a", "s", "d"] {
        let (dx, dy) = pan_delta_for_key(key).unwrap();
        camera.pan(dx, dy);
    }
    assert_eq!(camera.center, (0.0, 0.0));
}

#[test]
fn wheel_zooms_and_clamps() {
    let mut camera = Camera::default();
    let start = camera.scale;

    assert!(camera.handle_event(&wheel_event(MouseScrollDelta::LineDelta(0.0, 1.0))));
    assert_eq!(camera.scale, start * ZOOM_STEP);

    assert!(!camera.handle_event(&wheel_event(MouseScrollDelta::LineDelta(0.0, 0.0))));
    assert_eq!(camera.scale, start * ZOOM_STEP);

    assert!(camera.handle_event(&wheel_event(MouseScrollDelta::LineDelta(0.0, 1000.0))));
    assert_eq!(camera.scale, MAX_SCALE);

    assert!(camera.handle_event(&wheel_event(MouseScrollDelta::LineDelta(0.0, -1000.0))));
    assert_eq!(camera.scale, MIN_SCALE);
}

#[test]
fn pixel_delta_wheel_zooms() {
    // Touchpad-события приходят в пикселях, а не в «ступеньках».
    let mut camera = Camera::default();
    let start = camera.scale;
    let event = wheel_event(MouseScrollDelta::PixelDelta(PhysicalPosition::new(
        0.0, 50.0,
    )));

    assert!(camera.handle_event(&event));
    assert_eq!(camera.scale, start * ZOOM_STEP.powf(0.5));
}

#[test]
fn title_shows_metrics_separated_by_bars() {
    let camera = camera_at((1.5, -2.25), 25.0);
    let title = format_title(59.6, &camera);

    assert_eq!(
        title,
        "ug0 | 60 fps | zoom: 25.0 px/cell | x: 1.50, y: -2.25"
    );
    assert_eq!(title.matches('|').count(), 3);
    assert!(title.starts_with("ug0 | "));
}

#[test]
fn fps_is_averaged_over_last_second() {
    let base = Instant::now();
    let mut fps = FpsCounter::new(base);

    // Кадры в течение первых 900 мс: окно ещё не закрыто.
    for step in 0..10 {
        fps.tick(base + Duration::from_millis(step * 100));
    }
    assert_eq!(fps.fps(), 0.0);

    // 11-й кадр замыкает окно длиной ровно 1 с.
    fps.tick(base + Duration::from_secs(1));
    assert!((fps.fps() - 11.0).abs() < 1e-9, "fps = {}", fps.fps());

    // Следующая секунда считается заново, а не накопительно.
    for step in 1..=10 {
        fps.tick(base + Duration::from_millis(1000 + step * 100));
    }
    assert!((fps.fps() - 10.0).abs() < 1e-9, "fps = {}", fps.fps());
}

#[test]
fn irrelevant_events_do_not_trigger_redraw() {
    // Ошибка 9: события, не относящиеся к камере, провоцировали лишний кадр.
    let mut camera = Camera::default();
    let before = (camera.center, camera.scale);

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

    assert_eq!((camera.center, camera.scale), before);
}

fn wheel_event(delta: MouseScrollDelta) -> WindowEvent {
    WindowEvent::MouseWheel {
        device_id: DeviceId::dummy(),
        delta,
        phase: TouchPhase::Moved,
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
