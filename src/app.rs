use crate::archive::Archive;
use crate::assets::Source;
use crate::audio::Audio;
use crate::gfx::{Gfx, TextureId};
use crate::input::Input;
use std::sync::Arc;
use web_time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::keyboard::PhysicalKey;
use winit::window::{Window, WindowId};

const FRAME: f64 = 1.0 / 60.0;

pub struct Game {
    pub archive: Archive,
    pub audio: Audio,
    pub input: Input,
    pub frame: u64,
    title: TextureId,
}

impl Game {
    async fn load(source: Source, gfx: &mut Gfx) -> anyhow::Result<Self> {
        let archive = Archive::from_bytes(source.read("th08.dat").await?)?;
        let audio = Audio::new(source, &archive.read("thbgm.fmt")?);
        let title = gfx.load_image(&archive.read("title00.png")?)?;
        Ok(Self { archive, audio, input: Input::default(), frame: 0, title })
    }

    #[cfg(target_arch = "wasm32")]
    fn start_once(&mut self) {
        if !self.audio.is_started() {
            self.start();
        }
    }

    fn start(&mut self) {
        self.audio.ensure_started();
        self.audio.play_bgm("th08_01.wav");
    }

    fn update(&mut self) {
        self.input.tick();
        self.frame += 1;
    }

    fn draw(&self, gfx: &mut Gfx) {
        gfx.image(self.title, 0.0, 0.0);
    }
}

struct Running {
    window: Arc<Window>,
    gfx: Gfx,
    game: Game,
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
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    shot: Option<u64>,
}

async fn init(window: Arc<Window>, source: Source) -> anyhow::Result<Box<Running>> {
    let mut gfx = Gfx::new(window.clone()).await?;
    let game = Game::load(source, &mut gfx).await?;
    Ok(Box::new(Running { window, gfx, game, deadline: 0.0, epoch: Instant::now() }))
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.started {
            return;
        }
        self.started = true;
        let attrs = Window::default_attributes()
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
                r.game.start();
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
            WindowEvent::Resized(s) => r.gfx.resize(s.width, s.height),
            WindowEvent::KeyboardInput { event, .. } => {
                #[cfg(target_arch = "wasm32")]
                r.game.start_once();
                if let PhysicalKey::Code(k) = event.physical_key {
                    r.game.input.key(k, event.state == ElementState::Pressed);
                }
            }
            WindowEvent::RedrawRequested => {
                // Original timing: one logic step per 1/60 s deadline; late frames are dropped, not caught up.
                let now = r.epoch.elapsed().as_secs_f64();
                if now <= r.deadline {
                    #[cfg(not(target_arch = "wasm32"))]
                    std::thread::sleep(std::time::Duration::from_secs_f64((r.deadline - now).min(0.001)));
                    r.window.request_redraw();
                    return;
                }
                while r.deadline < now {
                    r.deadline += FRAME;
                }
                r.game.update();
                let size = r.window.inner_size();
                r.gfx.resize(size.width, size.height);
                r.game.draw(&mut r.gfx);
                #[cfg(not(target_arch = "wasm32"))]
                if self.shot.is_some_and(|f| r.game.frame >= f) {
                    r.gfx.capture().save("/tmp/th08_shot.png").expect("save shot");
                    el.exit();
                    return;
                }
                r.gfx.present();
                r.window.request_redraw();
            }
            _ => {}
        }
    }
}

pub fn run(source: Source) {
    let el = EventLoop::<UserEvent>::with_user_event().build().expect("event loop");
    let shot = std::env::var("TH08_SHOT").ok().and_then(|v| v.parse().ok());
    let app = App { source, proxy: el.create_proxy(), run: None, started: false, shot };
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
