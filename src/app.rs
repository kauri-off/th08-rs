//! Window and message loop (00 §1, §2: WinMain 0x4418C0, `Frame_Run` 0x441E70 pacing) on winit; plus debug run modes.

use crate::launcher::{Action, Folder, Screen, View};
use crate::pacer::{Pacer, Tick};
use crate::settings::Settings;
use std::sync::Arc;
use th08_core::assets::Source;
use th08_core::game::Ctx;
use th08_core::task::Flow;
use th08_render::gfx::Gfx;
use web_time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{KeyCode, PhysicalKey};
#[cfg(not(target_arch = "wasm32"))]
use winit::window::Fullscreen;
use winit::window::{Window, WindowId};

// A loaded game: the game owns its host settings by value (display, dirs, debug switches); the core ones went into
// its `CoreConfig` at load. The global `settings::get` is only for tools and tests.
struct Running {
    ctx: Ctx,
    pacer: Pacer,
    settings: Settings,
}

// An egui screen on a device of its own (launcher, error, loading frame).
struct Ui {
    gfx: Box<Gfx>,
    screen: Screen,
}

// The port shell around the game (port UI, no spec section). Transitions:
// Loading(device) → Setup | Loading(game); Setup → Loading(game) on Play; Loading(game) → Running | Error;
// Error → Setup on Back; Running ⇄ PortMenu on F1; Quit / close from any state.
enum Shell {
    // The launcher screen (also the saved `launcher = off` wait).
    Setup(Box<Ui>),
    // A start-up step runs: the window's device, then th08.dat / audio (its one "Loading" frame is already drawn).
    Loading,
    Running(Box<Running>),
    // F1 over the game, whose pacer is suspended; the screen draws on the game's own device.
    PortMenu(Box<Running>, Box<Screen>),
    Error(Box<Ui>),
}

enum UserEvent {
    Ready(Box<Running>),
    // The window's GPU device is ready.
    Gfx(Box<Gfx>),
    // Start-up failed; `gfx` is a fresh device for the error screen (None: no GPU, a message box instead).
    Failed { msg: String, gfx: Option<Box<Gfx>> },
}

// The `Failed` event for a start-up error: the error screen needs a new device, the failed load consumed the old one.
async fn failed(window: Arc<Window>, e: anyhow::Error, vsync: Option<bool>) -> UserEvent {
    log::error!("startup failed: {e:#}");
    let gfx = match vsync {
        Some(v) => Gfx::new(window, v).await.ok().map(Box::new),
        None => None,
    };
    UserEvent::Failed {
        msg: format!("{e:#}"),
        gfx,
    }
}

struct App {
    proxy: EventLoopProxy<UserEvent>,
    shell: Shell,
    // Created at the first `Resumed`.
    window: Option<Arc<Window>>,
    // The settings Setup edits: the start-up ones, then the ones the last Play used (Back from Error keeps the edits).
    // Direct starts: the final settings, cloned into `Running`.
    setup: Settings,
    // Known up front for direct starts, web (the files picked in the page) and Android bundled assets; native
    // launcher: `game_dir` at Play.
    source: Option<Source>,
    // Show Setup first (else load at once).
    launcher: bool,
    // Saved `launcher = off`: when the game starts unless Shift was held (set at the first Waiting redraw).
    auto_at: Option<Instant>,
    // Between `Suspended` and the next `Resumed` (no surface, no logic frames).
    suspended: bool,
    // Port display: Alt held; the fullscreen state, owned here (`set_fullscreen`).
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    alt: bool,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    fullscreen: bool,
}

async fn load(source: Source, gfx: Gfx, s: Settings) -> anyhow::Result<Box<Running>> {
    // `TH08_DRAWLOG`: the draw log writer is opened here and passed in (STEP10_CRATES 10a).
    #[cfg(not(target_arch = "wasm32"))]
    let drawlog = s.drawlog.as_deref().and_then(open_drawlog);
    #[cfg(target_arch = "wasm32")]
    let drawlog = None;
    let io = th08_core::game::Io {
        renderer: Box::new(gfx),
        audio: Box::new(th08_audio::audio::Audio::new(source.clone(), s.audio())),
        host: s.app_host(),
        drawlog,
    };
    let ctx = Ctx::load(source, io, s.core()).await?;
    Ok(Box::new(Running {
        ctx,
        pacer: Pacer::new(s.turbo),
        settings: s,
    }))
}

// What a game frame asks of the shell.
enum Frame {
    Continue,
    Exit,
    // Result 2 restart: the live config's window mode (true = fullscreen).
    Restarted(bool),
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return self.reattach();
        }
        let window = Arc::new(
            el.create_window(window_attrs(self.setup.scale, self.fullscreen))
                .expect("window"),
        );
        self.window = Some(window.clone());
        let (proxy, vsync) = (self.proxy.clone(), self.setup.vsync);
        spawn(async move {
            let ev = match Gfx::new(window.clone(), vsync).await {
                Ok(g) => UserEvent::Gfx(Box::new(g)),
                Err(e) => failed(window, e, None).await,
            };
            let _ = proxy.send_event(ev);
        });
    }

    // ARCH_D §5: the native window is gone (Android home / screen off); drop the surface, freeze logic.
    fn suspended(&mut self, _: &ActiveEventLoop) {
        self.suspended = true;
        match &mut self.shell {
            Shell::Running(r) | Shell::PortMenu(r, _) => {
                r.pacer.suspend();
                gfx(&mut r.ctx).detach();
            }
            Shell::Setup(ui) | Shell::Error(ui) => ui.gfx.detach(),
            Shell::Loading => {}
        }
    }

    fn user_event(&mut self, el: &ActiveEventLoop, ev: UserEvent) {
        match ev {
            UserEvent::Ready(mut r) => {
                #[cfg(not(target_arch = "wasm32"))]
                r.ctx.start();
                r.pacer.start(Instant::now());
                if self.suspended {
                    r.pacer.suspend();
                    gfx(&mut r.ctx).detach();
                } else {
                    self.window().request_redraw();
                }
                self.shell = Shell::Running(r);
            }
            UserEvent::Gfx(gfx) => {
                if self.launcher {
                    let view = {
                        let m = crate::launcher::Model::new(&self.setup, self.folder());
                        if !self.setup.launcher && m.can_play() {
                            View::Waiting
                        } else {
                            View::Settings
                        }
                    };
                    self.shell = Shell::Setup(self.ui(gfx, view));
                } else {
                    let source = self.source.clone().expect("direct start has a source");
                    let s = self.setup.clone();
                    let screen = (!s.is_tool_run()).then(|| self.screen(&gfx, View::Loading));
                    self.start_load(el, gfx, screen, s, source);
                }
            }
            UserEvent::Failed { msg, gfx } => self.failed(el, msg, gfx),
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        let screen = match &self.shell {
            Shell::Setup(ui) | Shell::Error(ui) => &ui.screen,
            Shell::PortMenu(_, s) => &**s,
            Shell::Loading | Shell::Running(_) => return,
        };
        if let Some(t) = self.auto_at
            && screen.model().view == View::Waiting
        {
            if t <= Instant::now() {
                return self.play(el);
            }
            return el.set_control_flow(ControlFlow::WaitUntil(t));
        }
        match screen.repaint_at {
            Some(t) if t <= Instant::now() => {
                self.window().request_redraw();
                el.set_control_flow(ControlFlow::Wait);
            }
            Some(t) => el.set_control_flow(ControlFlow::WaitUntil(t)),
            None => el.set_control_flow(ControlFlow::Wait),
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _: WindowId, ev: WindowEvent) {
        #[cfg(not(target_arch = "wasm32"))]
        if let WindowEvent::ModifiersChanged(m) = &ev {
            self.alt = m.state().alt_key();
        }
        // Port-only F11 / Alt+Enter in every state: the one place fullscreen is toggled; the key reaches nothing else.
        #[cfg(not(target_arch = "wasm32"))]
        if let WindowEvent::KeyboardInput { event, .. } = &ev
            && event.state == ElementState::Pressed
            && is_fullscreen_key(self.alt, event.physical_key)
        {
            if !event.repeat {
                self.set_fullscreen(!self.fullscreen);
            }
            return;
        }
        match self.shell {
            Shell::Loading => {
                if matches!(ev, WindowEvent::CloseRequested) {
                    el.exit();
                }
            }
            Shell::Setup(_) | Shell::Error(_) => self.ui_event(el, ev),
            Shell::Running(_) => self.game_event(el, ev),
            Shell::PortMenu(..) => self.menu_event(el, ev),
        }
    }
}

impl App {
    fn new(el: &EventLoop<UserEvent>, source: Option<Source>, s: Settings, launcher: bool) -> Self {
        App {
            proxy: el.create_proxy(),
            shell: Shell::Loading,
            window: None,
            fullscreen: s.fullscreen && cfg!(not(target_arch = "wasm32")),
            setup: s,
            source,
            launcher,
            auto_at: None,
            suspended: false,
            alt: false,
        }
    }

    fn window(&self) -> &Arc<Window> {
        self.window.as_ref().expect("window")
    }

    // Where Setup's game files come from: the `game_dir` row (native) or the files picked in the web page.
    fn folder(&self) -> Folder {
        match &self.source {
            #[cfg(target_arch = "wasm32")]
            Some(src) => Folder::Web(src.names()),
            #[cfg(target_os = "android")]
            Some(src) => Folder::Bundled(src.names()),
            _ => Folder::Native,
        }
    }

    // A screen for `view`, editing `setup`; the box mirrors the window's fullscreen state.
    fn screen(&self, gfx: &Gfx, view: View) -> Screen {
        let mut model = crate::launcher::Model::new(&self.setup, self.folder());
        if cfg!(not(target_arch = "wasm32")) && model.settings().fullscreen != self.fullscreen {
            model.set_fullscreen(self.fullscreen);
        }
        model.view = view;
        Screen::new(self.window(), gfx, model)
    }

    fn ui(&self, mut gfx: Box<Gfx>, view: View) -> Box<Ui> {
        let screen = self.screen(&gfx, view);
        if self.suspended {
            gfx.detach();
        }
        self.window().request_redraw();
        Box::new(Ui { gfx, screen })
    }

    // A second `Resumed` (ARCH_D §5): a new surface for the native window on the same device; the pacer deadline
    // restarts at now (not while the port menu holds the game). Audio is not touched here.
    fn reattach(&mut self) {
        if !std::mem::take(&mut self.suspended) {
            return;
        }
        let window = self.window().clone();
        let res = match &mut self.shell {
            Shell::Running(r) => {
                r.pacer.resume(Instant::now());
                gfx(&mut r.ctx).attach(window.clone())
            }
            Shell::PortMenu(r, _) => gfx(&mut r.ctx).attach(window.clone()),
            Shell::Setup(ui) | Shell::Error(ui) => ui.gfx.attach(window.clone()),
            Shell::Loading => Ok(()),
        };
        if let Err(e) = res {
            log::error!("surface: {e:#}");
        }
        window.request_redraw();
    }

    // The one fullscreen owner: F11 / Alt+Enter, the launcher's Fullscreen box and the game's restart land here.
    // Setup / Error: the box follows and is saved with the other edits on Play; otherwise port.cfg is written now.
    #[cfg_attr(target_arch = "wasm32", allow(unused_variables))]
    fn set_fullscreen(&mut self, on: bool) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.fullscreen = on;
            self.window()
                .set_fullscreen(on.then_some(Fullscreen::Borderless(None)));
            match &mut self.shell {
                Shell::Setup(ui) | Shell::Error(ui) => {
                    ui.screen.model_mut().set_fullscreen(on);
                    self.window().request_redraw();
                }
                _ => crate::settings::save_port_cfg(
                    &self.setup.dirs,
                    "fullscreen",
                    if on { "on" } else { "off" },
                ),
            }
        }
    }

    fn act(&mut self, el: &ActiveEventLoop, a: Action) {
        match a {
            Action::Play => self.play(el),
            Action::Quit => el.exit(),
            Action::Fullscreen(on) => self.set_fullscreen(on),
            Action::Back => {
                if let Shell::Error(mut ui) = std::mem::replace(&mut self.shell, Shell::Loading) {
                    ui.screen.model_mut().view = View::Settings;
                    self.shell = Shell::Setup(ui);
                    self.window().request_redraw();
                }
            }
            Action::Resume => self.resume_game(el),
        }
    }

    // Setup / Error events: egui input, the redraw pass and its action, Shift during the Waiting screen.
    fn ui_event(&mut self, el: &ActiveEventLoop, ev: WindowEvent) {
        let window = self.window().clone();
        let (Shell::Setup(ui) | Shell::Error(ui)) = &mut self.shell else {
            return;
        };
        match ev {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::RedrawRequested => match ui.screen.redraw(&window, &mut ui.gfx) {
                // Saved `launcher = off`: Shift may still open the screen for a moment after the first frame.
                None if ui.screen.model().view == View::Waiting => {
                    self.auto_at
                        .get_or_insert_with(|| Instant::now() + AUTO_WAIT);
                }
                None => {}
                Some(a) => self.act(el, a),
            },
            ev => {
                if let WindowEvent::Resized(s) = ev {
                    ui.gfx.resize(s.width, s.height);
                }
                let shift = match &ev {
                    WindowEvent::ModifiersChanged(m) => m.state().shift_key(),
                    WindowEvent::KeyboardInput { event, .. } => matches!(
                        event.physical_key,
                        PhysicalKey::Code(KeyCode::ShiftLeft | KeyCode::ShiftRight)
                    ),
                    _ => false,
                };
                let model = ui.screen.model_mut();
                if shift && model.view == View::Waiting {
                    model.view = View::Settings;
                    window.request_redraw();
                }
                if ui.screen.on_event(&window, &ev) {
                    window.request_redraw();
                }
            }
        }
    }

    // Play: save the edited settings, apply the display settings, draw the Loading frame, drop egui (and its GPU
    // objects) and load the game on the same device exactly as without the launcher.
    fn play(&mut self, el: &ActiveEventLoop) {
        let Shell::Setup(ui) = std::mem::replace(&mut self.shell, Shell::Loading) else {
            return;
        };
        let Ui {
            mut gfx,
            mut screen,
        } = *ui;
        screen.model().save();
        let s = screen.model().settings();
        el.set_control_flow(ControlFlow::Wait);
        if s.vsync != self.setup.vsync {
            gfx.set_vsync(s.vsync);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if s.scale != self.setup.scale && !s.fullscreen {
            let k = s.scale as f64;
            let _ = self
                .window()
                .request_inner_size(winit::dpi::LogicalSize::new(640.0 * k, 480.0 * k));
        }
        // Native: the `game_dir` row; Android with bundled assets and web: the source given at start.
        #[cfg(not(target_arch = "wasm32"))]
        let source = self.source.clone().unwrap_or_else(|| {
            crate::sources::native(s.game_dir.clone().expect("Play needs a game folder"))
        });
        #[cfg(target_arch = "wasm32")]
        let source = self.source.clone().expect("web files");
        screen.model_mut().view = View::Loading;
        self.setup = s.clone();
        self.start_load(el, gfx, Some(screen), s, source);
    }

    // Shell::Loading: one "Loading" frame (unless tool runs), then the screen and its GPU objects are dropped and the
    // game loads on this device.
    fn start_load(
        &mut self,
        el: &ActiveEventLoop,
        mut gfx: Box<Gfx>,
        screen: Option<Screen>,
        s: Settings,
        source: Source,
    ) {
        let window = self.window().clone();
        if let Some(mut screen) = screen
            && !self.suspended
        {
            screen.redraw(&window, &mut gfx);
        }
        self.shell = Shell::Loading;
        self.auto_at = None;
        el.set_control_flow(ControlFlow::Wait);
        let (proxy, vsync) = (self.proxy.clone(), s.vsync);
        spawn(async move {
            let ev = match load(source, *gfx, s).await {
                Ok(r) => UserEvent::Ready(r),
                Err(e) => failed(window, e, Some(vsync)).await,
            };
            let _ = proxy.send_event(ev);
        });
    }

    // A start-up error: Shell::Error (Back returns to Setup), or a message box and exit when there is no GPU. Tool
    // runs only log it and exit.
    fn failed(&mut self, el: &ActiveEventLoop, msg: String, gfx: Option<Box<Gfx>>) {
        let tool = self.setup.is_tool_run();
        let gfx = match gfx {
            Some(g) if !tool => g,
            _ => {
                #[cfg(not(target_arch = "wasm32"))]
                if tool {
                    std::process::exit(1);
                }
                message_box(&format!("The game could not start:\n{msg}"));
                return el.exit();
            }
        };
        self.shell = Shell::Error(self.ui(gfx, View::Error(msg)));
    }

    // F1: the port menu over the game; its pacer stops and held keys are released (as on focus loss).
    fn open_port_menu(&mut self) {
        let Shell::Running(mut r) = std::mem::replace(&mut self.shell, Shell::Loading) else {
            return;
        };
        r.pacer.suspend();
        if let Some(h) = r.ctx.host_as::<crate::platform::AppHost>() {
            h.input.set_focus(false);
        }
        let mut model = crate::launcher::Model::new(&r.settings, Folder::Native);
        model.view = View::PortMenu;
        let screen = Screen::new(self.window(), gfx(&mut r.ctx), model);
        self.shell = Shell::PortMenu(r, Box::new(screen));
        self.window().request_redraw();
    }

    // Resume: the screen's GPU objects go, the pacer deadline restarts at now (the time in the menu is not caught up).
    fn resume_game(&mut self, el: &ActiveEventLoop) {
        let Shell::PortMenu(mut r, screen) = std::mem::replace(&mut self.shell, Shell::Loading)
        else {
            return;
        };
        drop(screen);
        if !self.suspended {
            r.pacer.resume(Instant::now());
        }
        if let Some(h) = r.ctx.host_as::<crate::platform::AppHost>() {
            h.input.set_focus(true);
        }
        self.shell = Shell::Running(r);
        el.set_control_flow(ControlFlow::Wait);
        self.window().request_redraw();
    }

    fn menu_event(&mut self, el: &ActiveEventLoop, ev: WindowEvent) {
        let window = self.window().clone();
        let Shell::PortMenu(r, screen) = &mut self.shell else {
            return;
        };
        match ev {
            WindowEvent::CloseRequested => close(r, el),
            WindowEvent::RedrawRequested => {
                if let Some(a) = screen.redraw(&window, gfx(&mut r.ctx)) {
                    self.act(el, a);
                }
            }
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed
                    && !event.repeat
                    && matches!(
                        event.physical_key,
                        PhysicalKey::Code(KeyCode::F1 | KeyCode::Escape)
                    ) =>
            {
                self.resume_game(el)
            }
            ev => {
                if let WindowEvent::Resized(s) = ev {
                    gfx(&mut r.ctx).resize(s.width, s.height);
                }
                if screen.on_event(&window, &ev) {
                    window.request_redraw();
                }
            }
        }
    }

    fn game_event(&mut self, el: &ActiveEventLoop, ev: WindowEvent) {
        let window = self.window().clone();
        let Shell::Running(r) = &mut self.shell else {
            return;
        };
        match ev {
            WindowEvent::CloseRequested => close(r, el),
            WindowEvent::Resized(s) => gfx(&mut r.ctx).resize(s.width, s.height),
            WindowEvent::Focused(f) => {
                if let Some(h) = r.ctx.host_as::<crate::platform::AppHost>() {
                    h.input.set_focus(f);
                }
            }
            // Port-only F1: the port menu; the key press does not reach the game.
            WindowEvent::KeyboardInput { event, .. }
                if event.physical_key == PhysicalKey::Code(KeyCode::F1) =>
            {
                if event.state == ElementState::Pressed && !event.repeat {
                    self.open_port_menu();
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                #[cfg(target_arch = "wasm32")]
                r.ctx.start_once();
                if let (PhysicalKey::Code(k), Some(h)) = (
                    event.physical_key,
                    r.ctx.host_as::<crate::platform::AppHost>(),
                ) {
                    h.input
                        .keyboard
                        .key(k, event.state == ElementState::Pressed);
                }
            }
            WindowEvent::RedrawRequested => match frame(r, &window) {
                Frame::Continue => {}
                Frame::Exit => el.exit(),
                Frame::Restarted(on) => {
                    #[cfg(not(target_arch = "wasm32"))]
                    self.set_fullscreen(on);
                    #[cfg(target_arch = "wasm32")]
                    let _ = on;
                    window.request_redraw();
                }
            },
            _ => {}
        }
    }
}

// Closing the window mid-game. Tool `replay_save`: the recording is still written.
fn close(r: &mut Running, el: &ActiveEventLoop) {
    if r.ctx.core.replay_save.is_some() && r.ctx.w.game.scene == th08_core::gamemgr::SCENE_GAME {
        th08_core::replay::finish(&mut r.ctx);
    }
    el.exit()
}

// One redraw of the running game: Frame_Run timing (00 §2, pacer.rs), the logic frames, draw and Present.
fn frame(r: &mut Running, window: &Window) -> Frame {
    // Desktop: the port sleeps ≤ 1 ms for Sleep(0) while waiting.
    let turbo = r.pacer.turbo();
    let shot = r.settings.shot;
    let frames = match r.pacer.poll(Instant::now()) {
        Tick::Run(n) => n,
        Tick::Suspended => return Frame::Continue,
        #[cfg_attr(target_arch = "wasm32", allow(unused_variables))]
        Tick::Wait(left) => {
            #[cfg(not(target_arch = "wasm32"))]
            std::thread::sleep(std::time::Duration::from_secs_f64(left.min(0.001)));
            window.request_redraw();
            return Frame::Continue;
        }
    };
    // Web `?shot=<n>`: logic freezes at frame n so the page can be screenshotted.
    let frozen = cfg!(target_arch = "wasm32") && shot.is_some_and(|f| r.ctx.w.frame >= f);
    // Debug `TH08_TURBO=<n>`: n logic frames per drawn frame, no frame pacing.
    for _ in 0..frames {
        if shot.is_some_and(|f| r.ctx.w.frame >= f) && turbo > 0 {
            break;
        }
        match if frozen { Flow::Continue } else { r.ctx.calc() } {
            Flow::Continue => {}
            // Frame_Run returns after the calc chain and Sound_Update: no draw, no Present (15 §2.2, 00 §2).
            Flow::Quit => {
                r.ctx.exit_save();
                return Frame::Exit;
            }
            // Result 2 (00 §2): the program restarts in the live config's mode; this frame has no draw and no
            // Present. Port: window mode +0x22 is applied as the port's fullscreen setting.
            Flow::Error => {
                if let Err(e) = r.ctx.restart() {
                    log::error!("restart failed: {e:#}");
                    return Frame::Exit;
                }
                return Frame::Restarted(r.ctx.w.game.cfg.windowed == 0);
            }
        }
    }
    let size = window.inner_size();
    gfx(&mut r.ctx).resize(size.width, size.height);
    r.ctx.draw();
    // Debug `TH08_SHOT` frame (`TH08_SHOT_SIZE=WxH`: through the window's scaling blit).
    #[cfg(not(target_arch = "wasm32"))]
    if shot.is_some_and(|f| r.ctx.w.frame >= f) {
        let size = r.settings.shot_size;
        let (g, rec) = gfx_rec(&mut r.ctx);
        g.capture(rec, size)
            .save("/tmp/th08_shot.png")
            .expect("save shot");
        return Frame::Exit;
    }
    r.ctx.present();
    th08_core::ascii::apply_pending_captures(&mut r.ctx.w.anm, &mut r.ctx.rec);
    window.request_redraw();
    Frame::Continue
}

// How long a saved `launcher = off` waits for Shift after the first frame (the window must be mapped and focused for
// the modifier state to arrive).
const AUTO_WAIT: std::time::Duration = std::time::Duration::from_millis(500);

// Start-up error without a window to draw in.
fn message_box(msg: &str) {
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title("th08")
        .set_description(msg)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
    #[cfg(target_arch = "wasm32")]
    if let Some(w) = web_sys::window() {
        let _ = w.alert_with_message(msg);
    }
    #[cfg(target_os = "android")]
    let _ = msg;
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

// Game window that loads at once with these final settings (owned by the game, not installed globally).
pub fn run(source: Source, s: Settings) {
    run_app(Some(source), s, false);
}

// Game window that shows the launcher screen first. Native: `source` None (the folder comes from the screen); web:
// the files picked in the page.
pub fn run_launcher(source: Option<Source>, s: Settings) {
    run_app(source, s, true);
}

// Android (ARCH_D §6.3 runs 1–2): the launcher on the NativeActivity's event loop; game files from `source` (bundled
// APK assets) or, when None, the `game_dir` folder.
#[cfg(target_os = "android")]
pub fn run_android(
    android: winit::platform::android::activity::AndroidApp,
    source: Option<Source>,
    s: Settings,
) {
    use winit::platform::android::EventLoopBuilderExtAndroid;
    let el = EventLoop::<UserEvent>::with_user_event()
        .with_android_app(android)
        .build()
        .expect("event loop");
    run_on(el, source, s, true);
}

fn run_app(source: Option<Source>, s: Settings, launcher: bool) {
    let el = EventLoop::<UserEvent>::with_user_event()
        .build()
        .expect("event loop");
    run_on(el, source, s, launcher);
}

fn run_on(el: EventLoop<UserEvent>, source: Option<Source>, s: Settings, launcher: bool) {
    let app = App::new(&el, source, s, launcher);
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

// The app's `Ctx` always runs on a `Gfx` (resize, surface attach / detach, screenshot).
fn gfx(ctx: &mut Ctx) -> &mut Gfx {
    gfx_rec(ctx).0
}

fn gfx_rec(ctx: &mut Ctx) -> (&mut Gfx, &mut th08_core::drawlist::Recorder) {
    let g = ctx
        .renderer
        .as_any_mut()
        .downcast_mut()
        .expect("app renderer is Gfx");
    (g, &mut ctx.rec)
}

// `TH08_DRAWLOG` in the game: plain text only (`.xz` logs are written by the tools, which own liblzma).
#[cfg(not(target_arch = "wasm32"))]
fn open_drawlog(v: &str) -> Option<th08_core::drawfmt::Writer> {
    let (path, range) = th08_core::drawfmt::parse_spec(v);
    if path.ends_with(".xz") {
        log::error!(
            "TH08_DRAWLOG={path}: the game writes plain text; use the session / drawrun tools for .xz logs"
        );
        return None;
    }
    match std::fs::File::create(path) {
        Ok(f) => Some(th08_core::drawfmt::Writer::stream(
            Box::new(std::io::BufWriter::new(f)),
            range,
        )),
        Err(e) => {
            log::error!("TH08_DRAWLOG={path}: {e}");
            None
        }
    }
}
