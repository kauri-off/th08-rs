//! Window and message loop (00 §1, §2: WinMain 0x4418C0, `Frame_Run` 0x441E70 pacing) on winit; plus debug run modes.

use crate::assets::Source;
use crate::game::Ctx;
use crate::gfx::Gfx;
use crate::settings::Settings;
use crate::task::Flow;
use std::sync::Arc;
use web_time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
#[cfg(not(target_arch = "wasm32"))]
use winit::keyboard::KeyCode;
use winit::keyboard::PhysicalKey;
#[cfg(not(target_arch = "wasm32"))]
use winit::window::Fullscreen;
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
    // Launcher mode: the window's GPU device is ready.
    Gfx(Box<Gfx>),
}

// The launcher screen in the game window, before `settings::init`.
struct Launch {
    window: Arc<Window>,
    gfx: Option<Box<Gfx>>,
    screen: Option<crate::launcher::Screen>,
    // Settings before the launcher (window size, vsync) and the files picked in the web page.
    start: Settings,
    folder: Option<crate::launcher::Folder>,
}

struct App {
    // None in launcher mode on native (the folder is chosen on screen).
    source: Option<Source>,
    proxy: EventLoopProxy<UserEvent>,
    run: Option<Box<Running>>,
    started: bool,
    // Launcher mode: settings before Play; then the window (`launch`).
    pre: Option<Settings>,
    launch: Option<Box<Launch>>,
    settings: Option<&'static Settings>,
    // Debug `TH08_SHOT` frame.
    shot: Option<u64>,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    shot_size: Option<(u32, u32)>,
    turbo: u32,
    // Port display: Alt held, current fullscreen state (Alt+Enter toggles and saves it).
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    alt: bool,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    fullscreen: bool,
}

async fn init(window: Arc<Window>, source: Source, vsync: bool) -> anyhow::Result<Box<Running>> {
    let gfx = Gfx::new(window.clone(), vsync).await?;
    load(window, source, gfx).await
}

async fn load(window: Arc<Window>, source: Source, gfx: Gfx) -> anyhow::Result<Box<Running>> {
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
        let s = self.pre.as_ref().or(self.settings).unwrap();
        let window = Arc::new(
            el.create_window(window_attrs(s.scale, self.fullscreen))
                .expect("window"),
        );
        let proxy = self.proxy.clone();
        let vsync = s.vsync;
        if let Some(start) = self.pre.take() {
            let folder = Some(match &self.source {
                #[cfg(target_arch = "wasm32")]
                Some(src) => crate::launcher::Folder::Web(src.file_names()),
                _ => crate::launcher::Folder::Native,
            });
            self.launch = Some(Box::new(Launch {
                window: window.clone(),
                gfx: None,
                screen: None,
                start,
                folder,
            }));
            spawn(async move {
                match Gfx::new(window, vsync).await {
                    Ok(g) => {
                        let _ = proxy.send_event(UserEvent::Gfx(Box::new(g)));
                    }
                    Err(e) => log::error!("startup failed: {e:#}"),
                }
            });
            return;
        }
        let source = self.source.clone().unwrap();
        spawn(async move {
            match init(window, source, vsync).await {
                Ok(r) => {
                    let _ = proxy.send_event(UserEvent::Ready(r));
                }
                Err(e) => log::error!("startup failed: {e:#}"),
            }
        });
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
            UserEvent::Gfx(gfx) => {
                let Some(l) = self.launch.as_mut() else {
                    return;
                };
                let model = crate::launcher::Model::new(&l.start, l.folder.take().unwrap());
                l.screen = Some(crate::launcher::Screen::new(&l.window, &gfx, model));
                l.gfx = Some(gfx);
                l.window.request_redraw();
            }
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        let Some(l) = self.launch.as_ref() else {
            return;
        };
        match l.screen.as_ref().and_then(|s| s.repaint_at) {
            Some(t) if t <= Instant::now() => {
                l.window.request_redraw();
                el.set_control_flow(ControlFlow::Wait);
            }
            Some(t) => el.set_control_flow(ControlFlow::WaitUntil(t)),
            None => el.set_control_flow(ControlFlow::Wait),
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _: WindowId, ev: WindowEvent) {
        if self.launch.is_some() {
            return self.launcher_event(el, ev);
        }
        let Some(r) = self.run.as_mut() else {
            if matches!(ev, WindowEvent::CloseRequested) {
                el.exit();
            }
            return;
        };
        match ev {
            WindowEvent::CloseRequested => {
                // Tool `replay_save`: closing the window mid-game still writes the recording.
                if r.ctx.settings.replay_save.is_some()
                    && r.ctx.game.scene == crate::gamemgr::SCENE_GAME
                {
                    crate::replay::finish(&mut r.ctx);
                }
                el.exit()
            }
            WindowEvent::Resized(s) => r.ctx.gfx.resize(s.width, s.height),
            #[cfg(not(target_arch = "wasm32"))]
            WindowEvent::ModifiersChanged(m) => self.alt = m.state().alt_key(),
            // Port-only F11 / Alt+Enter: toggle fullscreen and save it; the key press does not reach the game.
            #[cfg(not(target_arch = "wasm32"))]
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed
                    && is_fullscreen_key(self.alt, event.physical_key) =>
            {
                if !event.repeat {
                    self.fullscreen = !self.fullscreen;
                    r.window
                        .set_fullscreen(self.fullscreen.then_some(Fullscreen::Borderless(None)));
                    crate::settings::save_port_cfg(
                        "fullscreen",
                        if self.fullscreen { "on" } else { "off" },
                    );
                }
            }
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
                        // Frame_Run returns after the calc chain and Sound_Update: no draw, no Present (15 §2.2, 00 §2).
                        Flow::Quit => {
                            r.ctx.exit_save();
                            return el.exit();
                        }
                        // Result 2 (00 §2): the program restarts in the live config's mode; this frame has no
                        // draw and no Present. Port: window mode +0x22 is applied as the port's fullscreen setting.
                        Flow::Error => {
                            if let Err(e) = r.ctx.restart() {
                                log::error!("restart failed: {e:#}");
                                return el.exit();
                            }
                            #[cfg(not(target_arch = "wasm32"))]
                            {
                                self.fullscreen = r.ctx.game.cfg.windowed == 0;
                                r.window.set_fullscreen(
                                    self.fullscreen.then_some(Fullscreen::Borderless(None)),
                                );
                                crate::settings::save_port_cfg(
                                    "fullscreen",
                                    if self.fullscreen { "on" } else { "off" },
                                );
                            }
                            r.window.request_redraw();
                            return;
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
                crate::ascii::apply_pending_captures(&mut r.ctx.anm, &mut r.ctx.gfx);
                r.window.request_redraw();
            }
            _ => {}
        }
    }
}

impl App {
    fn new(el: &EventLoop<UserEvent>, source: Option<Source>, pre: Option<Settings>) -> Self {
        let mut app = App {
            source,
            proxy: el.create_proxy(),
            run: None,
            started: false,
            pre: None,
            launch: None,
            settings: None,
            shot: None,
            shot_size: None,
            turbo: 0,
            alt: false,
            fullscreen: false,
        };
        match pre {
            Some(s) => {
                app.fullscreen = s.fullscreen && cfg!(not(target_arch = "wasm32"));
                app.pre = Some(s);
            }
            None => app.adopt(crate::settings::get()),
        }
        app
    }

    // The final settings drive the run.
    fn adopt(&mut self, settings: &'static Settings) {
        self.settings = Some(settings);
        self.shot = settings.shot;
        // Debug `TH08_SHOT_SIZE=WxH`: the shot goes through the window's scaling blit.
        self.shot_size = settings.shot_size;
        self.turbo = settings.turbo;
        self.fullscreen = settings.fullscreen && cfg!(not(target_arch = "wasm32"));
    }

    fn launcher_event(&mut self, el: &ActiveEventLoop, ev: WindowEvent) {
        let l = self.launch.as_mut().unwrap();
        let (Some(screen), Some(gfx)) = (l.screen.as_mut(), l.gfx.as_mut()) else {
            if matches!(ev, WindowEvent::CloseRequested) {
                el.exit();
            }
            return;
        };
        match ev {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::RedrawRequested => match screen.redraw(&l.window, gfx) {
                Some(crate::launcher::Action::Play) => self.play(el),
                Some(crate::launcher::Action::Quit) => el.exit(),
                None => {
                    // The fullscreen box applies at once, not only on Play.
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        let s = screen.model().settings();
                        if s.fullscreen != self.fullscreen {
                            self.fullscreen = s.fullscreen;
                            l.window.set_fullscreen(
                                s.fullscreen.then_some(Fullscreen::Borderless(None)),
                            );
                            if !s.fullscreen {
                                let k = s.scale as f64;
                                let _ = l.window.request_inner_size(winit::dpi::LogicalSize::new(
                                    640.0 * k,
                                    480.0 * k,
                                ));
                            }
                        }
                    }
                }
            },
            #[cfg(not(target_arch = "wasm32"))]
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed
                    && is_fullscreen_key(self.alt, event.physical_key) =>
            {
                if !event.repeat {
                    screen.model_mut().toggle_fullscreen();
                    l.window.request_redraw();
                }
            }
            ev => {
                if let WindowEvent::Resized(s) = ev {
                    gfx.resize(s.width, s.height);
                }
                #[cfg(not(target_arch = "wasm32"))]
                if let WindowEvent::ModifiersChanged(m) = &ev {
                    self.alt = m.state().alt_key();
                }
                if screen.on_event(&l.window, &ev) {
                    l.window.request_redraw();
                }
            }
        }
    }

    // Play: save the edited settings, drop egui (and its GPU objects), apply the display settings, install the final
    // settings and load the game on the same device exactly as without the launcher.
    fn play(&mut self, el: &ActiveEventLoop) {
        let l = *self.launch.take().unwrap();
        let screen = l.screen.unwrap();
        let mut gfx = l.gfx.unwrap();
        screen.model().save();
        let s = screen.model().settings();
        drop(screen);
        el.set_control_flow(ControlFlow::Wait);
        if s.vsync != l.start.vsync {
            gfx.set_vsync(s.vsync);
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            if s.fullscreen != self.fullscreen {
                l.window
                    .set_fullscreen(s.fullscreen.then_some(Fullscreen::Borderless(None)));
            }
            if s.scale != l.start.scale && !s.fullscreen {
                let k = s.scale as f64;
                let _ = l
                    .window
                    .request_inner_size(winit::dpi::LogicalSize::new(640.0 * k, 480.0 * k));
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        let source = Source::native(s.game_dir.clone().expect("Play needs a game folder"));
        #[cfg(target_arch = "wasm32")]
        let source = self.source.take().expect("web files");
        crate::settings::init(s);
        self.adopt(crate::settings::get());
        let (proxy, window) = (self.proxy.clone(), l.window);
        spawn(async move {
            match load(window, source, *gfx).await {
                Ok(r) => {
                    let _ = proxy.send_event(UserEvent::Ready(r));
                }
                Err(e) => log::error!("startup failed: {e:#}"),
            }
        });
    }
}

// Port-only fullscreen toggle keys: F11, or Enter / keypad Enter with Alt held.
#[cfg(not(target_arch = "wasm32"))]
fn is_fullscreen_key(alt: bool, key: PhysicalKey) -> bool {
    match key {
        PhysicalKey::Code(KeyCode::F11) => true,
        PhysicalKey::Code(KeyCode::Enter | KeyCode::NumpadEnter) => alt,
        _ => false,
    }
}

// 640×480 client area (00 §1); the port window is resizable and scaled (port setting `scale`, native).
#[cfg_attr(target_arch = "wasm32", allow(unused_variables))]
fn window_attrs(scale: u32, fullscreen: bool) -> winit::window::WindowAttributes {
    let attrs = Window::default_attributes().with_title("th08");
    #[cfg(not(target_arch = "wasm32"))]
    let attrs = {
        let k = scale as f64;
        attrs
            .with_inner_size(winit::dpi::LogicalSize::new(640.0 * k, 480.0 * k))
            .with_fullscreen(fullscreen.then_some(Fullscreen::Borderless(None)))
    };
    #[cfg(target_arch = "wasm32")]
    let attrs = {
        use winit::platform::web::WindowAttributesExtWebSys;
        attrs
            .with_inner_size(winit::dpi::LogicalSize::new(640.0, 480.0))
            .with_append(true)
    };
    attrs
}

// Native start-up blocks on the future (as before the launcher); web runs it on the JS event loop.
fn spawn(fut: impl std::future::Future<Output = ()> + 'static) {
    #[cfg(not(target_arch = "wasm32"))]
    pollster::block_on(fut);
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_futures::spawn_local(fut);
}

// Game window with the settings already installed (`settings::init` or `get`).
pub fn run(source: Source) {
    run_app(Some(source), None);
}

// Game window that shows the launcher screen first; `settings::init` happens on Play. Native: `source` None (the
// folder comes from the screen); web: the files picked in the page.
pub fn run_launcher(source: Option<Source>, s: Settings) {
    run_app(source, Some(s));
}

fn run_app(source: Option<Source>, pre: Option<Settings>) {
    let el = EventLoop::<UserEvent>::with_user_event()
        .build()
        .expect("event loop");
    let app = App::new(&el, source, pre);
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
