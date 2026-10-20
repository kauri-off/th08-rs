//! Window and message loop (00 §1, §2: WinMain 0x4418C0, `Frame_Run` 0x441E70 pacing) on winit; plus debug run modes.

use crate::assets::Source;
use crate::game::Ctx;
use crate::gfx::Gfx;
use crate::task::Flow;
use std::sync::Arc;
use web_time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::keyboard::PhysicalKey;
use winit::window::{Window, WindowId};

// Frame_Run deadline step, 1/60 s (00 §2).
const FRAME: f64 = 1.0 / 60.0;

struct Running {
    window: Arc<Window>,
    ctx: Ctx,
    // Frame_Run's next-frame deadline, seconds since `epoch`.
    deadline: f64,
    epoch: Instant,
}

enum UserEvent {
    Ready(Box<Running>),
}

struct App {
    source: Source,
    proxy: EventLoopProxy<UserEvent>,
    run: Option<Box<Running>>,
    started: bool,
    // Debug `TH08_SHOT` frame.
    shot: Option<u64>,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    shot_size: Option<(u32, u32)>,
    turbo: u32,
}

async fn init(window: Arc<Window>, source: Source) -> anyhow::Result<Box<Running>> {
    let gfx = Gfx::new(window.clone()).await?;
    let ctx = Ctx::load(source, gfx).await?;
    Ok(Box::new(Running {
        window,
        ctx,
        deadline: 0.0,
        epoch: Instant::now(),
    }))
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.started {
            return;
        }
        self.started = true;
        let attrs = Window::default_attributes()
            // 640×480 client area (00 §1); the port window is resizable and scaled.
            .with_title("th08")
            .with_inner_size(winit::dpi::LogicalSize::new(640.0, 480.0));
        #[cfg(target_arch = "wasm32")]
        let attrs = {
            use winit::platform::web::WindowAttributesExtWebSys;
            attrs.with_append(true)
        };
        let window = Arc::new(el.create_window(attrs).expect("window"));
        let (proxy, source) = (self.proxy.clone(), self.source.clone());
        let fut = async move {
            match init(window, source).await {
                Ok(r) => {
                    let _ = proxy.send_event(UserEvent::Ready(r));
                }
                Err(e) => log::error!("startup failed: {e:#}"),
            }
        };
        #[cfg(not(target_arch = "wasm32"))]
        pollster::block_on(fut);
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(fut);
    }

    fn user_event(&mut self, _: &ActiveEventLoop, ev: UserEvent) {
        match ev {
            UserEvent::Ready(mut r) => {
                #[cfg(not(target_arch = "wasm32"))]
                r.ctx.start();
                r.epoch = Instant::now();
                r.window.request_redraw();
                self.run = Some(r);
            }
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _: WindowId, ev: WindowEvent) {
        let Some(r) = self.run.as_mut() else {
            if matches!(ev, WindowEvent::CloseRequested) {
                el.exit();
            }
            return;
        };
        match ev {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(s) => r.ctx.gfx.resize(s.width, s.height),
            WindowEvent::KeyboardInput { event, .. } => {
                #[cfg(target_arch = "wasm32")]
                r.ctx.start_once();
                if let PhysicalKey::Code(k) = event.physical_key {
                    r.ctx.input.key(k, event.state == ElementState::Pressed);
                }
            }
            WindowEvent::RedrawRequested => {
                // Frame_Run timing (00 §2): one logic step per 1/60 s deadline; late frames are dropped, not caught up.
                // The monotonic clock never runs backwards, so the timer-wrap reset is not needed; the port sleeps ≤ 1 ms for Sleep(0).
                let now = r.epoch.elapsed().as_secs_f64();
                if self.turbo == 0 && now <= r.deadline {
                    #[cfg(not(target_arch = "wasm32"))]
                    std::thread::sleep(std::time::Duration::from_secs_f64(
                        (r.deadline - now).min(0.001),
                    ));
                    r.window.request_redraw();
                    return;
                }
                while r.deadline < now {
                    r.deadline += FRAME;
                }
                // Web `?shot=<n>`: logic freezes at frame n so the page can be screenshotted.
                let frozen =
                    cfg!(target_arch = "wasm32") && self.shot.is_some_and(|f| r.ctx.frame >= f);
                // Debug `TH08_TURBO=<n>`: n logic frames per drawn frame, no frame pacing.
                for _ in 0..self.turbo.max(1) {
                    if self.shot.is_some_and(|f| r.ctx.frame >= f) && self.turbo > 0 {
                        break;
                    }
                    match if frozen { Flow::Continue } else { r.ctx.calc() } {
                        Flow::Continue => {}
                        Flow::Quit => return el.exit(),
                        Flow::Error => {
                            log::error!("calc chain requested error exit");
                            return el.exit();
                        }
                    }
                }
                let size = r.window.inner_size();
                r.ctx.gfx.resize(size.width, size.height);
                r.ctx.draw();
                #[cfg(not(target_arch = "wasm32"))]
                if self.shot.is_some_and(|f| r.ctx.frame >= f) {
                    r.ctx
                        .gfx
                        .capture(self.shot_size)
                        .save("/tmp/th08_shot.png")
                        .expect("save shot");
                    el.exit();
                    return;
                }
                r.ctx.gfx.present();
                r.window.request_redraw();
            }
            _ => {}
        }
    }
}

pub fn run(source: Source) {
    let el = EventLoop::<UserEvent>::with_user_event()
        .build()
        .expect("event loop");
    let shot = crate::env::var("TH08_SHOT").and_then(|v| v.parse().ok());
    let turbo = crate::env::var("TH08_TURBO")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    // Debug `TH08_SHOT_SIZE=WxH`: the shot goes through the window's scaling blit.
    let shot_size = crate::env::var("TH08_SHOT_SIZE").and_then(|v| {
        let (w, h) = v.split_once('x')?;
        Some((w.parse().ok()?, h.parse().ok()?))
    });
    let app = App {
        source,
        proxy: el.create_proxy(),
        run: None,
        started: false,
        shot,
        shot_size,
        turbo,
    };
    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut app = app;
        el.run_app(&mut app).expect("run");
    }
    #[cfg(target_arch = "wasm32")]
    {
        use winit::platform::web::EventLoopExtWebSys;
        el.spawn_app(app);
    }
}
