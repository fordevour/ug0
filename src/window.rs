use std::collections::HashSet;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{Duration, Instant};

use softbuffer::{Context, Surface};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

use crate::camera::Camera;
use crate::images::{EMBEDDED, Images};
use crate::logging::{debug, info, warning};
use crate::tiles::Tiles;
use crate::world::World;

/// 60 кадров в секунду.
const FRAME_TIME: Duration = Duration::from_nanos(1_000_000_000 / 60);
/// Один тик симуляции каждые 6 кадров — 10 тиков в секунду при 60 fps.
pub(crate) const FRAMES_PER_TICK: u32 = 6;

/// Счётчик кадров, который раз в `FRAMES_PER_TICK` кадров выдаёт тик.
pub(crate) struct TickClock {
    frames: u32,
    total_frames: u64,
    ticks: u64,
}

impl TickClock {
    pub(crate) fn new() -> Self {
        Self {
            frames: 0,
            total_frames: 0,
            ticks: 0,
        }
    }

    /// Учитывает кадр. Возвращает `true`, когда пора выполнить тик мира.
    pub(crate) fn advance(&mut self) -> bool {
        self.frames += 1;
        self.total_frames += 1;
        if self.frames < FRAMES_PER_TICK {
            return false;
        }

        self.frames = 0;
        self.ticks += 1;
        true
    }

    pub(crate) fn ticks(&self) -> u64 {
        self.ticks
    }

    pub(crate) fn frames(&self) -> u64 {
        self.total_frames
    }
}

/// Новая веха зарастания: 50% или 100%. Возвращает `None`, если веха уже была.
pub(crate) fn grass_milestone(previous: u64, percent: u64) -> Option<u64> {
    let milestone = if percent >= 100 {
        100
    } else if percent >= 50 {
        50
    } else {
        return None;
    };

    (milestone > previous).then_some(milestone)
}

impl Default for TickClock {
    fn default() -> Self {
        Self::new()
    }
}

/// Средний FPS и среднее время генерации кадра за последнюю секунду.
pub(crate) struct FpsCounter {
    window_start: Instant,
    frames: u32,
    fps: f64,
    frame_time_total: Duration,
    frame_ms: f64,
}

impl FpsCounter {
    pub(crate) fn new(now: Instant) -> Self {
        Self {
            window_start: now,
            frames: 0,
            fps: 0.0,
            frame_time_total: Duration::ZERO,
            frame_ms: 0.0,
        }
    }

    /// Учитывает отрисованный кадр; раз в секунду пересчитывает средние.
    pub(crate) fn tick(&mut self, now: Instant, frame_time: Duration) {
        self.frames += 1;
        self.frame_time_total += frame_time;

        let elapsed = now.duration_since(self.window_start);
        if elapsed >= Duration::from_secs(1) {
            self.fps = self.frames as f64 / elapsed.as_secs_f64();
            self.frame_ms = self.frame_time_total.as_secs_f64() * 1000.0 / self.frames as f64;
            self.frames = 0;
            self.frame_time_total = Duration::ZERO;
            self.window_start = now;
        }
    }

    pub(crate) fn fps(&self) -> f64 {
        self.fps
    }

    /// Среднее время генерации кадра за последнюю секунду, в миллисекундах.
    pub(crate) fn frame_ms(&self) -> f64 {
        self.frame_ms
    }
}

impl Default for FpsCounter {
    fn default() -> Self {
        Self::new(Instant::now())
    }
}

/// Заголовок окна:
/// `ug0 | 60 fps | frame: 0.42 ms | zoom: 32 px/cell | x: 0, y: 0 | ticks: 0`.
pub(crate) fn format_title(fps: f64, frame_ms: f64, camera: &Camera, ticks: u64) -> String {
    let (x, y) = camera.center;
    format!(
        "ug0 | {fps:.0} fps | frame: {frame_ms:.2} ms | zoom: {:.0} px/cell | x: {x}, y: {y} | ticks: {ticks}",
        camera.scale()
    )
}

pub struct App {
    window: Option<Arc<Window>>,
    context: Option<Context<Arc<Window>>>,
    surface: Option<Surface<Arc<Window>, Arc<Window>>>,
    camera: Camera,
    world: World,
    tiles: Tiles,
    clock: TickClock,
    /// Момент, когда нужно запросить следующий кадр. `None` — окно ещё не создано.
    next_frame: Option<Instant>,
    fps: FpsCounter,
    /// Последний установленный заголовок — чтобы не трогать окно без изменений.
    title: Option<String>,
    first_frame: bool,
    first_grass: bool,
    grass_milestone: u64,
    exit_reason: &'static str,
    /// Накопители для среднего разложения времени кадра (до следующей статистики).
    render_total: Duration,
    wait_total: Duration,
    copy_total: Duration,
    present_total: Duration,
    breakdown_frames: u32,
}

impl App {
    fn new() -> Result<Self, String> {
        let world = World::starting();
        let images = Images::load()?;
        let tiles = Tiles::build(&images);

        info!("images: {} embedded ({})", EMBEDDED.len(), images.summary());
        info!("world: {}x{} cells, 10 ticks/s", world.width, world.height);
        warn_unknown_images(&world, &images);

        Ok(Self {
            window: None,
            context: None,
            surface: None,
            camera: Camera::new(),
            world,
            tiles,
            clock: TickClock::new(),
            next_frame: None,
            fps: FpsCounter::default(),
            title: None,
            first_frame: false,
            first_grass: false,
            grass_milestone: 0,
            exit_reason: "event loop stopped",
            render_total: Duration::ZERO,
            wait_total: Duration::ZERO,
            copy_total: Duration::ZERO,
            present_total: Duration::ZERO,
            breakdown_frames: 0,
        })
    }

    fn redraw(&mut self) {
        let (width, height) = {
            let Some(window) = &self.window else {
                return;
            };
            let size = window.inner_size();
            let (Some(width), Some(height)) =
                (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
            else {
                return;
            };
            (width, height)
        };

        // Симуляция идёт реже отрисовки: один тик на 6 кадров (~10 в секунду).
        if self.clock.advance() {
            self.world.tick();
            self.log_simulation();
        }

        let Some(surface) = self.surface.as_mut() else {
            return;
        };

        surface
            .resize(width, height)
            .expect("не удалось изменить размер surface");

        let frame_start = Instant::now();
        let render_start = Instant::now();
        let frame = self
            .camera
            .render(&self.world, &self.tiles, width.get(), height.get());
        let render_time = render_start.elapsed();
        debug_assert_eq!(
            (frame.width, frame.height),
            (width.get(), height.get()),
            "камера вернула кадр не того размера"
        );
        debug_assert_eq!(
            frame.pixels.len(),
            width.get() as usize * height.get() as usize
        );

        // `buffer_mut` может блокироваться, пока композитор не освободит буфер,
        // поэтому ожидание считаем отдельно от рендера и копирования.
        let wait_start = Instant::now();
        let mut buffer = surface.buffer_mut().expect("не удалось получить буфер");
        let wait_time = wait_start.elapsed();

        let copy_start = Instant::now();
        buffer.copy_from_slice(&frame.pixels);
        let copy_time = copy_start.elapsed();

        let present_start = Instant::now();
        buffer.present().expect("не удалось показать кадр");
        let present_time = present_start.elapsed();

        let frame_time = frame_start.elapsed();
        self.render_total += render_time;
        self.wait_total += wait_time;
        self.copy_total += copy_time;
        self.present_total += present_time;
        self.breakdown_frames += 1;

        if !self.first_frame {
            self.first_frame = true;
            info!("first frame");
        }

        self.update_title(frame_time);
    }

    /// Короткие события симуляции: первая трава, вехи, статистика раз в 10 с.
    fn log_simulation(&mut self) {
        let percent = self.world.grass_percent();
        let ticks = self.clock.ticks();

        if self.world.grass_count() > 0 && !self.first_grass {
            self.first_grass = true;
            info!("first grass at tick {ticks}");
        }

        if let Some(milestone) = grass_milestone(self.grass_milestone, percent) {
            self.grass_milestone = milestone;
            info!("grass {percent}% at tick {ticks}");
        }

        if ticks.is_multiple_of(100) {
            info!(
                "stats: {:.0} fps, frame {:.2} ms, ticks {ticks}, grass {percent}%",
                self.fps.fps(),
                self.fps.frame_ms()
            );

            let frames = self.breakdown_frames.max(1) as f64;
            let per_frame = |total: Duration| total.as_secs_f64() * 1000.0 / frames;
            debug!(
                "frame avg: render {:.2} ms, wait {:.2} ms, copy {:.2} ms, present {:.2} ms",
                per_frame(self.render_total),
                per_frame(self.wait_total),
                per_frame(self.copy_total),
                per_frame(self.present_total)
            );

            self.render_total = Duration::ZERO;
            self.wait_total = Duration::ZERO;
            self.copy_total = Duration::ZERO;
            self.present_total = Duration::ZERO;
            self.breakdown_frames = 0;
        }
    }

    fn update_title(&mut self, frame_time: Duration) {
        self.fps.tick(Instant::now(), frame_time);

        let title = format_title(
            self.fps.fps(),
            self.fps.frame_ms(),
            &self.camera,
            self.clock.ticks(),
        );
        if self.title.as_deref() == Some(title.as_str()) {
            return;
        }

        if let Some(window) = &self.window {
            window.set_title(&title);
        }
        self.title = Some(title);
    }
}

/// Предупреждает о клетках, которые ссылаются на несуществующие картинки.
/// Одно предупреждение на имя.
fn warn_unknown_images(world: &World, images: &Images) {
    let mut reported = HashSet::new();

    for y in 0..world.height {
        for x in 0..world.width {
            let Some(name) = world.cell(x, y) else {
                continue;
            };
            if images.get(name).is_none() && reported.insert(name) {
                warning!("unknown image {name:?} at cell ({x}, {y})");
            }
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let window = Arc::new(
            event_loop
                .create_window(Window::default_attributes().with_title(format_title(
                    self.fps.fps(),
                    self.fps.frame_ms(),
                    &self.camera,
                    self.clock.ticks(),
                )))
                .expect("не удалось создать окно"),
        );
        let context = Context::new(window.clone()).expect("не удалось создать context");
        let surface = Surface::new(&context, window.clone()).expect("не удалось создать surface");

        let size = window.inner_size();
        info!(
            "window: {}x{} px, scale {:.1}",
            size.width,
            size.height,
            window.scale_factor()
        );
        debug!("surface created");

        window.request_redraw();
        self.window = Some(window);
        self.context = Some(context);
        self.surface = Some(surface);
        self.next_frame = Some(Instant::now());
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();

        let Some(next) = self.next_frame else {
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        };

        let mut next = next;
        if now >= next {
            if let Some(window) = &self.window {
                window.request_redraw();
            }

            // Фиксированный шаг без накопления дрейфа.
            next += FRAME_TIME;
            // Если отстали больше чем на кадр — не копим долг и не спим лишний кадр:
            // при перегрузке кадры идут подряд, с периодом ~времени рендера.
            if next < now {
                next = now;
            }
        }

        self.next_frame = Some(next);
        event_loop.set_control_flow(ControlFlow::WaitUntil(next));
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        // Сырые события уходят камере; она сама решает, что ей интересно.
        // Перерисовка не запрашивается здесь — кадр придёт на ближайшем тике.
        self.camera.handle_event(&event);

        match event {
            WindowEvent::CloseRequested => {
                self.exit_reason = "close requested";
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => self.redraw(),
            WindowEvent::Resized(size) => {
                debug!("resized to {}x{}", size.width, size.height);
            }
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed
                    && matches!(event.logical_key, Key::Named(NamedKey::Escape)) =>
            {
                self.exit_reason = "escape pressed";
                event_loop.exit();
            }
            _ => {}
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        info!(
            "exit: {}, {} ticks, {} frames",
            self.exit_reason,
            self.clock.ticks(),
            self.clock.frames()
        );
    }
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = EventLoop::new()?;
    let mut app = App::new()?;
    event_loop.run_app(&mut app)?;
    Ok(())
}
