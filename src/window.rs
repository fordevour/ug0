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
use crate::world::World;

/// 60 кадров в секунду.
const FRAME_TIME: Duration = Duration::from_nanos(1_000_000_000 / 60);

/// Средний FPS за последнюю секунду.
pub(crate) struct FpsCounter {
    window_start: Instant,
    frames: u32,
    fps: f64,
}

impl FpsCounter {
    pub(crate) fn new(now: Instant) -> Self {
        Self {
            window_start: now,
            frames: 0,
            fps: 0.0,
        }
    }

    /// Учитывает отрисованный кадр; раз в секунду пересчитывает среднее.
    pub(crate) fn tick(&mut self, now: Instant) {
        self.frames += 1;

        let elapsed = now.duration_since(self.window_start);
        if elapsed >= Duration::from_secs(1) {
            self.fps = self.frames as f64 / elapsed.as_secs_f64();
            self.frames = 0;
            self.window_start = now;
        }
    }

    pub(crate) fn fps(&self) -> f64 {
        self.fps
    }
}

impl Default for FpsCounter {
    fn default() -> Self {
        Self::new(Instant::now())
    }
}

/// Заголовок окна: `ug0 | 60 fps | zoom: 25.0 px/cell | x: 0.00, y: 0.00`.
pub(crate) fn format_title(fps: f64, camera: &Camera) -> String {
    let (x, y) = camera.center;
    format!(
        "ug0 | {fps:.0} fps | zoom: {:.1} px/cell | x: {x:.2}, y: {y:.2}",
        camera.scale
    )
}

#[derive(Default)]
pub struct App {
    window: Option<Arc<Window>>,
    context: Option<Context<Arc<Window>>>,
    surface: Option<Surface<Arc<Window>, Arc<Window>>>,
    camera: Camera,
    world: World,
    /// Момент, когда нужно запросить следующий кадр. `None` — окно ещё не создано.
    next_frame: Option<Instant>,
    fps: FpsCounter,
    /// Последний установленный заголовок — чтобы не трогать окно без изменений.
    title: Option<String>,
}

impl App {
    fn new() -> Self {
        Self {
            camera: Camera::new(),
            world: World::default(),
            ..Self::default()
        }
    }

    fn redraw(&mut self) {
        let (Some(window), Some(surface)) = (&self.window, self.surface.as_mut()) else {
            return;
        };

        let size = window.inner_size();
        let (Some(width), Some(height)) =
            (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
        else {
            return;
        };

        surface
            .resize(width, height)
            .expect("не удалось изменить размер surface");

        let frame = self.camera.render(&self.world, width.get(), height.get());
        debug_assert_eq!(
            (frame.width, frame.height),
            (width.get(), height.get()),
            "камера вернула кадр не того размера"
        );
        debug_assert_eq!(
            frame.pixels.len(),
            width.get() as usize * height.get() as usize
        );

        let mut buffer = surface.buffer_mut().expect("не удалось получить буфер");
        buffer.copy_from_slice(&frame.pixels);
        buffer.present().expect("не удалось показать кадр");

        self.update_title();
    }

    fn update_title(&mut self) {
        self.fps.tick(Instant::now());

        let title = format_title(self.fps.fps(), &self.camera);
        if self.title.as_deref() == Some(title.as_str()) {
            return;
        }

        if let Some(window) = &self.window {
            window.set_title(&title);
        }
        self.title = Some(title);
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title(format_title(self.fps.fps(), &self.camera)),
                )
                .expect("не удалось создать окно"),
        );
        let context = Context::new(window.clone()).expect("не удалось создать context");
        let surface = Surface::new(&context, window.clone()).expect("не удалось создать surface");

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
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => self.redraw(),
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed
                    && matches!(event.logical_key, Key::Named(NamedKey::Escape)) =>
            {
                event_loop.exit();
            }
            _ => {}
        }
    }
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = EventLoop::new()?;
    let mut app = App::new();
    event_loop.run_app(&mut app)?;
    Ok(())
}
