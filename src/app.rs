//! Window and message loop on winit (00 §1, §2: WinMain 0x4418C0, `Frame_Run` 0x441E70 pacing); plus debug run modes.

use crate::launcher::{Action, Folder, Screen, View};
use crate::pacer::{Pacer, Tick};
use crate::settings::Settings;
use crate::transport::{Action as Transp, Transport};
use std::sync::Arc;
use th08_core::assets::Source;
use th08_core::game::Ctx;
use th08_core::task::Flow;
use th08_render::gfx::Gfx;
use web_time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{
    ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy, OwnedDisplayHandle,
};
use winit::keyboard::{KeyCode, PhysicalKey};
#[cfg(not(target_arch = "wasm32"))]
use winit::window::Fullscreen;
use winit::window::{Window, WindowId};

// A loaded game: the game owns its host settings by value (display, dirs, debug switches); the core ones went into
// its `CoreConfig` at load. The global `settings::get` is only for tools and tests.
struct Running {
    ctx: Ctx,
    pacer: Pacer,
    // Grid origin of the debug `present_hz` vsync.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    vsync_epoch: Instant,
    settings: Settings,
    // The touch overlay while a gamepad is in use; back on the next touch.
    hidden_overlay: Option<Box<dyn th08_render::gfx::Overlay>>,
    replay: ReplayTransport,
    // A save failure or screenshot toast and when it goes.
    toast: Option<(String, Instant)>,
    // A screenshot whose frame copy is still in flight (web maps the buffer asynchronously).
    snapshot: Option<th08_render::gfx::Readback>,
    // When the event loop next wakes the game for a redraw (None: on the next request only).
    wake: Option<Instant>,
    // While held: the last picture must be presented again (hold, resize, transport input, port menu closed).
    held_dirty: bool,
    // Practice slow motion: index into `transport::SLOW`; 0 = 1×.
    slow: usize,
    // BGM starts seen by the sound device (`now_playing` on), titles by file stem, the toast and when it goes.
    started: Option<crate::nowplaying::Started>,
    titles: Vec<(String, String)>,
    now_playing: Option<(String, Instant)>,
    discord: crate::discord::Rpc,
    // Play statistics recorder (`stats` on, live game with a save dir).
    stats: Option<crate::stats::Log>,
    // The sound device's port volume (port menu rows).
    volume: Option<th08_audio::audio::VolumeHandle>,
    // "Quit to title" from the port menu: input pulses still to send (quittitle.rs).
    quit: Option<crate::platform::quittitle::Quit>,
}

// Replay transport state of a running game (transport.rs); inert while no replay plays.
#[derive(Default)]
struct ReplayTransport {
    t: Transport,
    // A replay played at the last frame.
    playing: bool,
    // Score at the last frame of playback, logged when it ends.
    score: i64,
    label_until: Option<Instant>,
    // The touch strip (window px) while a replay plays with the touch overlay on; the last mouse position.
    strip: Option<[(Transp, [f32; 4]); 4]>,
    cursor: (f32, f32),
}

// An egui screen on a device of its own (launcher, error, loading frame).
struct Ui {
    gfx: Box<Gfx>,
    screen: Screen,
}

// The port shell around the game. Loading(device) → Setup | Loading(game); Setup → Loading(game) on Play;
// Loading(game) → Running | Error; Error → Setup on Back; Running ⇄ PortMenu on F1 / the touch MENU control /
// pad Select or a resume from the background (in a stage); Quit / close from any state.
enum Shell {
    // The launcher screen (also the saved `launcher = off` wait).
    Setup(Box<Ui>),
    // A start-up step runs: the window's device, then th08.dat / audio (its one "Loading" frame is already drawn).
    Loading,
    Running(Box<Running>),
    // The port menu over the game's last frame; the game's pacer is suspended and its audio paused.
    PortMenu(Box<Running>, Box<crate::platform::portmenu::PortMenu>),
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
async fn failed(
    window: Arc<Window>,
    display: OwnedDisplayHandle,
    e: anyhow::Error,
    vsync: Option<bool>,
) -> UserEvent {
    log::error!("startup failed: {e:#}");
    let gfx = match vsync {
        Some(v) => Gfx::new(window, display, v).await.ok().map(Box::new),
        None => None,
    };
    UserEvent::Failed {
        msg: format!("{e:#}"),
        gfx,
    }
}

struct App {
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    proxy: EventLoopProxy<UserEvent>,
    shell: Shell,
    // Created at the first `Resumed`, with the event loop's display handle (every device's wgpu instance gets it).
    window: Option<Arc<Window>>,
    display_handle: Option<OwnedDisplayHandle>,
    // Bumped at each `Suspended`: a device made before the native window changed has a dead surface.
    surface_gen: u32,
    // `surface_gen` when the running start-up step began.
    step_gen: u32,
    // Native: the start-up step, polled from the event loop so the load never blocks it (Android input ANR).
    #[cfg(not(target_arch = "wasm32"))]
    step: Option<std::pin::Pin<Box<dyn std::future::Future<Output = UserEvent>>>>,
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
    // The game's audio was last synced with a paused replay.
    replay_held: bool,
    // Last safe-area read (`refresh_safe`).
    safe_at: Option<Instant>,
    // The refresh rate the window's monitor last reported (Hz), logged on change.
    refresh_hz: Option<Option<f64>>,
    // Window focus, and whether audio pauses while unfocused: Android only (focus goes at Home / recents, before
    // `Suspended`); the desktop has the `mute_unfocused` row.
    focus: Focus,
    display: Display,
    // Android joystick motion among the touch events.
    sticks: crate::joystick::StickFilter<winit::event::DeviceId>,
    // Last gamepad list read (`refresh_pads`).
    pads_at: Option<Instant>,
}

#[derive(Clone, Copy)]
struct Focus {
    focused: bool,
    pause_audio: bool,
}

impl Focus {
    fn holds_audio(self) -> bool {
        self.pause_audio && !self.focused
    }
}

// Port display: Alt held; the fullscreen state, owned here (`set_fullscreen`).
#[derive(Clone, Copy)]
struct Display {
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    alt: bool,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    fullscreen: bool,
}

async fn load(source: Source, gfx: Gfx, s: Settings) -> anyhow::Result<Box<Running>> {
    // `TH08_DRAWLOG`: the draw log writer is opened here and passed in.
    #[cfg(not(target_arch = "wasm32"))]
    let drawlog = s.drawlog.as_deref().and_then(open_drawlog);
    #[cfg(target_arch = "wasm32")]
    let drawlog = None;
    // The system CJK font is read off the main thread while the game loads, so the first text frame does not wait.
    #[cfg(not(target_arch = "wasm32"))]
    let font = {
        let f = s.font.clone();
        std::thread::spawn(move || {
            th08_render::text::system_font(f.as_deref());
        })
    };
    th08_core::assets::yield_now().await;
    let mut renderer: Box<dyn th08_core::drawlist::Renderer> = Box::new(gfx);
    let overlay = s.mount_pack(renderer.as_mut());
    th08_core::assets::yield_now().await;
    let device = th08_audio::audio::Audio::new(source.clone(), s.audio());
    let volume = device.volume_handle();
    let mut audio: Box<dyn th08_core::game::AudioOut> = Box::new(device);
    let mut started = None;
    // The port menu can switch `now_playing` on: the game watches BGM starts unless it is a tool run.
    if s.now_playing || !s.is_tool_run() {
        let (w, st) = crate::nowplaying::Watch::new(audio);
        audio = Box::new(w);
        started = Some(st);
    }
    th08_core::assets::yield_now().await;
    let io = th08_core::game::Io {
        renderer,
        audio,
        host: s.app_host(),
        drawlog,
        overlay,
    };
    let mut ctx = Ctx::load_with(source, io, s.core(), s.uses_saves()).await?;
    #[cfg(not(target_arch = "wasm32"))]
    {
        while !font.is_finished() {
            th08_core::assets::yield_now().await;
        }
        let _ = font.join();
    }
    let titles = if started.is_some() {
        let cmt = ctx
            .archive
            .present()
            .read("musiccmt.txt")
            .unwrap_or_default();
        crate::nowplaying::titles(&cmt, &self::gfx(&mut ctx).pack().db)
    } else {
        Vec::new()
    };
    let cheats = s.active_cheats();
    if !cheats.is_empty() {
        log::warn!(
            "cheats on ({}): score.dat and replays are not saved",
            cheats.join(", ")
        );
        ctx.w.game.persist = false;
    }
    let discord = s.discord && !s.is_tool_run();
    #[cfg(not(target_arch = "wasm32"))]
    let stats = s
        .storage_dir()
        .filter(|_| s.stats && s.uses_saves() && !s.is_tool_run())
        .map(|d| crate::stats::Log::new(&d));
    #[cfg(target_arch = "wasm32")]
    let stats = None;
    Ok(Box::new(Running {
        ctx,
        pacer: Pacer::new(s.turbo),
        vsync_epoch: Instant::now(),
        settings: s,
        hidden_overlay: None,
        replay: ReplayTransport::default(),
        toast: None,
        snapshot: None,
        wake: None,
        held_dirty: false,
        slow: 0,
        started,
        titles,
        now_playing: None,
        discord: crate::discord::Rpc::new(discord),
        stats,
        volume: Some(volume),
        quit: None,
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
            el.create_window(window_attrs(self.setup.scale, self.display.fullscreen))
                .expect("window"),
        );
        self.window = Some(window.clone());
        let display = el.owned_display_handle();
        self.display_handle = Some(display.clone());
        let vsync = self.setup.vsync;
        self.spawn(el, async move {
            match Gfx::new(window.clone(), display.clone(), vsync).await {
                Ok(g) => UserEvent::Gfx(Box::new(g)),
                Err(e) => failed(window, display, e, None).await,
            }
        });
    }

    // The native window is gone (Android home / screen off): drop the surface, freeze logic, pause audio and write
    // score.dat from a World clone (the process may be killed in the background; a live save would draw RNG).
    fn suspended(&mut self, _: &ActiveEventLoop) {
        log::info!("lifecycle: suspended");
        self.suspended = true;
        self.surface_gen = self.surface_gen.wrapping_add(1);
        if let Shell::PortMenu(_, m) = &self.shell {
            m.save();
        }
        match &mut self.shell {
            Shell::Running(r) | Shell::PortMenu(r, _) => {
                r.pacer.suspend();
                gfx(&mut r.ctx).detach();
                let n = r.ctx.save_from_clone().len();
                log::info!("suspend: score.dat saved from a World clone ({n} bytes)");
            }
            Shell::Setup(ui) | Shell::Error(ui) => ui.gfx.detach(),
            Shell::Loading => {}
        }
        self.sync_audio();
    }

    // Web: a page unloaded for good (not kept in the back-forward cache) writes score.dat first.
    #[cfg(target_arch = "wasm32")]
    fn exiting(&mut self, _: &ActiveEventLoop) {
        self.save_clone("page unload");
    }

    fn user_event(&mut self, el: &ActiveEventLoop, ev: UserEvent) {
        let fix = surface_fix(self.step_gen, self.surface_gen, self.suspended);
        match ev {
            UserEvent::Ready(mut r) => {
                self.fix_surface(gfx(&mut r.ctx), fix);
                #[cfg(not(target_arch = "wasm32"))]
                r.ctx.start();
                r.ctx.set_windowed(!self.display.fullscreen);
                // Touch overlay: drawn by the renderer after the blit.
                if r.settings.touch.active() {
                    gfx(&mut r.ctx).overlay =
                        Some(Box::new(crate::platform::overlay::TouchOverlay::new(
                            crate::i18n::resolve(r.settings.language),
                            r.settings.font.clone(),
                        )));
                }
                gfx(&mut r.ctx).hud =
                    Some(Box::new(crate::platform::overlay::TransportOverlay::new(
                        crate::i18n::resolve(r.settings.language),
                        r.settings.font.clone(),
                        r.settings.now_playing,
                    )));
                r.pacer.start(Instant::now());
                gfx(&mut r.ctx).safe = self.read_safe();
                gfx(&mut r.ctx).blit_options = r.settings.blit;
                crate::platform::hitbox::install(gfx(&mut r.ctx), r.settings.hitboxes);
                if self.suspended {
                    r.pacer.suspend();
                } else {
                    self.window().request_redraw();
                }
                self.shell = Shell::Running(r);
                self.refresh_display();
                self.sync_audio();
            }
            UserEvent::Gfx(mut gfx) => {
                self.fix_surface(&mut gfx, fix);
                crate::platform::framerate::request_60(self.window());
                self.refresh_display();
                gfx.safe = self.read_safe();
                if self.launcher {
                    let view = {
                        let m = crate::launcher::Model::new(&self.setup, self.folder());
                        if !self.setup.launcher && m.can_play() && m.last_error().is_none() {
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
            UserEvent::Failed { msg, mut gfx } => {
                if let Some(g) = &mut gfx {
                    self.fix_surface(g, fix);
                }
                self.failed(el, msg, gfx);
            }
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        #[cfg(target_os = "android")]
        if winit::platform::android::activity::destroy_requested() {
            return self.destroyed(el);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if self.step.is_some() {
            return self.poll_step(el);
        }
        #[cfg(target_arch = "wasm32")]
        if self.current_gfx().is_some_and(|g| g.is_lost()) {
            return self.context_lost(el);
        }
        #[cfg(target_os = "android")]
        for _ in 0..crate::platform::back::take() {
            log::info!("back: OnBackInvokedCallback");
            self.back(el);
        }
        // The running game sleeps until its wake time (frame deadline, paused-replay HUD check).
        if let Shell::Running(r) = &mut self.shell {
            let due = r.wake.is_some_and(|t| t <= Instant::now());
            if due {
                r.wake = None;
            }
            let flow = r.wake.map_or(ControlFlow::Wait, ControlFlow::WaitUntil);
            if due {
                self.window().request_redraw();
            }
            return el.set_control_flow(flow);
        }
        let (waiting, repaint_at) = match &self.shell {
            Shell::Setup(ui) | Shell::Error(ui) => (
                ui.screen.model().view == View::Waiting,
                ui.screen.repaint_at,
            ),
            Shell::PortMenu(_, m) => (false, m.repaint_at),
            Shell::Loading | Shell::Running(_) => return,
        };
        // The game does not poll in the port menu: gilrs is pumped here for its Select (resume).
        if let Shell::PortMenu(r, _) = &mut self.shell
            && let Some(h) = r.ctx.host_as::<crate::platform::AppHost>()
            && h.joystick.needs_pump()
        {
            h.joystick.pump();
            if h.joystick.take_menu() {
                return self.resume_game(el);
            }
            let t = Instant::now() + PAD_PUMP;
            let t = repaint_at.map_or(t, |r| r.min(t));
            if t > Instant::now() {
                return el.set_control_flow(ControlFlow::WaitUntil(t));
            }
        }
        if let Some(t) = self.auto_at
            && waiting
        {
            if t <= Instant::now() {
                return self.play(el);
            }
            return el.set_control_flow(ControlFlow::WaitUntil(t));
        }
        match repaint_at {
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
            self.display.alt = m.state().alt_key();
        }
        // Port-only F11 / Alt+Enter in every state: the one place fullscreen is toggled; the key reaches nothing else.
        #[cfg(not(target_arch = "wasm32"))]
        if let WindowEvent::KeyboardInput { event, .. } = &ev
            && event.state == ElementState::Pressed
            && is_fullscreen_key(self.display.alt, event.physical_key)
        {
            if !event.repeat {
                self.set_fullscreen(!self.display.fullscreen);
            }
            return;
        }
        match &ev {
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                self.refresh_safe(true);
                self.refresh_display();
            }
            #[cfg(not(target_arch = "wasm32"))]
            WindowEvent::Moved(_) => self.refresh_display(),
            WindowEvent::Focused(f) => {
                self.focus.focused = *f;
                if *f {
                    self.refresh_safe(true);
                }
                self.sync_audio();
            }
            WindowEvent::RedrawRequested => {
                self.refresh_safe(false);
                self.refresh_pads();
            }
            // Web: a hidden tab may be discarded without another event; score.dat is written now.
            #[cfg(target_arch = "wasm32")]
            WindowEvent::Occluded(true) => self.save_clone("tab hidden"),
            // Android: joystick motion goes to the gamepad, never to a touch consumer.
            WindowEvent::Touch(t) if cfg!(target_os = "android") => {
                // The motion hook ran just before this event: a hat change arrives with a stick move.
                if let Some((x, y)) = crate::platform::gamepad::take_hat()
                    && let Some(h) = self.host()
                {
                    h.joystick.events.hat(x, y);
                }
                if let Some((x, y)) = self.sticks.classify(
                    t.device_id,
                    t.phase,
                    t.location.x as f32,
                    t.location.y as f32,
                ) {
                    if let Some(h) = self.host() {
                        h.joystick.events.stick(x, y);
                    }
                    return;
                }
            }
            _ => {}
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
            display_handle: None,
            surface_gen: 0,
            step_gen: 0,
            #[cfg(not(target_arch = "wasm32"))]
            step: None,
            display: Display {
                alt: false,
                fullscreen: s.fullscreen && cfg!(not(target_arch = "wasm32")),
            },
            setup: s,
            source,
            launcher,
            auto_at: None,
            suspended: false,
            replay_held: false,
            safe_at: None,
            refresh_hz: None,
            sticks: crate::joystick::StickFilter::default(),
            pads_at: None,
            focus: Focus {
                focused: true,
                pause_audio: cfg!(target_os = "android"),
            },
        }
    }

    fn window(&self) -> &Arc<Window> {
        self.window.as_ref().expect("window")
    }

    fn display_handle(&self) -> OwnedDisplayHandle {
        self.display_handle.clone().expect("display handle")
    }

    // Runs a start-up step; its event arrives as a user event. Native: polled from `about_to_wait` between window
    // events; web: on the JS event loop.
    fn spawn(
        &mut self,
        el: &ActiveEventLoop,
        fut: impl std::future::Future<Output = UserEvent> + 'static,
    ) {
        self.step_gen = self.surface_gen;
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.step = Some(Box::pin(fut));
            self.poll_step(el);
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = el;
            let proxy = self.proxy.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let _ = proxy.send_event(fut.await);
            });
        }
    }

    // One poll of the start-up step: it runs to its next yield point; while it is pending the loop keeps polling.
    #[cfg(not(target_arch = "wasm32"))]
    fn poll_step(&mut self, el: &ActiveEventLoop) {
        let Some(mut fut) = self.step.take() else {
            return;
        };
        let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
        match fut.as_mut().poll(&mut cx) {
            std::task::Poll::Ready(ev) => {
                el.set_control_flow(ControlFlow::Wait);
                self.user_event(el, ev);
            }
            std::task::Poll::Pending => {
                self.step = Some(fut);
                el.set_control_flow(ControlFlow::Poll);
            }
        }
    }

    // A device from a start-up step: a native window that went away meanwhile (Android Home during the load) left
    // it a surface on the old window.
    fn fix_surface(&self, g: &mut Gfx, fix: SurfaceFix) {
        match fix {
            SurfaceFix::Keep => {}
            SurfaceFix::Detach => g.detach(),
            SurfaceFix::Reattach => {
                log::info!("lifecycle: the window changed during start-up; new surface");
                g.detach();
                if let Err(e) = g.attach(self.window().clone()) {
                    log::error!("surface: {e:#}");
                }
            }
        }
    }

    // The device of the current shell, if any.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    fn current_gfx(&mut self) -> Option<&mut Gfx> {
        match &mut self.shell {
            Shell::Running(r) | Shell::PortMenu(r, _) => Some(gfx(&mut r.ctx)),
            Shell::Setup(ui) | Shell::Error(ui) => Some(&mut ui.gfx),
            Shell::Loading => None,
        }
    }

    // score.dat from a World clone (no RNG draws) while a game is loaded.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    fn save_clone(&mut self, why: &str) {
        if let Shell::Running(r) | Shell::PortMenu(r, _) = &mut self.shell {
            let n = r.ctx.save_from_clone().len();
            log::info!("{why}: score.dat saved from a World clone ({n} bytes)");
        }
    }

    // Web: the WebGL / WebGPU context is gone and wgpu cannot rebuild it there. score.dat is written, then the page
    // reloads (the files stay in the site storage) after a message.
    #[cfg(target_arch = "wasm32")]
    fn context_lost(&mut self, el: &ActiveEventLoop) {
        log::error!("graphics context lost: reloading the page");
        self.save_clone("context lost");
        self.shell = Shell::Loading;
        message_box("The graphics context was lost. The page reloads now.");
        if let Some(w) = web_sys::window() {
            let _ = w.location().reload();
        }
        el.exit();
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
        if cfg!(not(target_arch = "wasm32"))
            && model.settings().fullscreen != self.display.fullscreen
        {
            model.set_fullscreen(self.display.fullscreen);
        }
        model.view = view;
        if cfg!(not(any(target_arch = "wasm32", target_os = "android"))) {
            model.attach_pad();
        }
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

    // Android `onDestroy` (winit does not forward it) waits for `android_main` to return: save if the suspend did
    // not, then leave the event loop.
    #[cfg(target_os = "android")]
    fn destroyed(&mut self, el: &ActiveEventLoop) {
        log::info!("lifecycle: destroyed");
        if !self.suspended
            && let Shell::Running(r) | Shell::PortMenu(r, _) = &mut self.shell
        {
            let n = r.ctx.save_from_clone().len();
            log::info!("destroy: score.dat saved from a World clone ({n} bytes)");
        }
        el.exit();
    }

    // A second `Resumed`: a new surface for the native window on the same device. In a stage the port
    // menu opens over the last frame (the player is not dropped into live play); elsewhere the pacer deadline restarts
    // at now (no catch-up). Audio resumes unless the port menu holds the game.
    fn reattach(&mut self) {
        if !std::mem::take(&mut self.suspended) {
            return;
        }
        log::info!("lifecycle: resumed");
        let window = self.window().clone();
        let res = match &mut self.shell {
            Shell::Running(r) | Shell::PortMenu(r, _) => gfx(&mut r.ctx).attach(window.clone()),
            Shell::Setup(ui) | Shell::Error(ui) => ui.gfx.attach(window.clone()),
            Shell::Loading => Ok(()),
        };
        if let Err(e) = res {
            log::error!("surface: {e:#}");
        }
        crate::platform::framerate::request_60(&window);
        if let Shell::Running(r) = &mut self.shell {
            r.held_dirty = true;
            if in_stage(r) {
                self.open_port_menu();
            } else {
                r.pacer.resume(Instant::now());
            }
        }
        self.sync_audio();
        self.refresh_safe(true);
        window.request_redraw();
    }

    // Game audio is held while suspended, while the port menu is open, while a replay is paused and (opt-in) while
    // the window is unfocused; `mute_unfocused` silences it without holding it.
    fn sync_audio(&mut self) {
        let held = matches!(&self.shell, Shell::Running(r) if r.pacer.is_held());
        self.replay_held = held;
        let hold = self.suspended || self.focus.holds_audio() || held;
        let focused = self.focus.focused;
        if let Shell::Running(r) | Shell::PortMenu(r, _) = &mut self.shell {
            let mute = r.settings.mute_unfocused && !focused;
            r.ctx.audio.set_muted(mute);
        }
        match &mut self.shell {
            Shell::Running(r) if !hold => r.ctx.audio.resume(),
            Shell::Running(r) | Shell::PortMenu(r, _) => r.ctx.audio.pause(),
            _ => {}
        }
    }

    // The window's safe area now (platform insets + `screen_margin`).
    fn read_safe(&self) -> crate::platform::safearea::SafeArea {
        crate::platform::safearea::read(self.window(), self.setup.screen_margin)
    }

    // The window's monitor refresh rate: logged when it changes, handed to the game's pacer (`present_hz` overrides).
    fn refresh_display(&mut self) {
        let Some(w) = &self.window else { return };
        let hz = w
            .current_monitor()
            .and_then(|m| m.refresh_rate_millihertz())
            .map(|mhz| f64::from(mhz) / 1000.0);
        if self.refresh_hz != Some(hz) {
            match hz {
                Some(hz) => log::info!("display: {hz:.2} Hz"),
                None => log::info!("display: refresh rate unknown"),
            }
            self.refresh_hz = Some(hz);
        }
        if let Shell::Running(r) | Shell::PortMenu(r, _) = &mut self.shell {
            let sim = r.settings.present_hz.map(f64::from);
            r.pacer.set_display_hz(sim.or(hz));
        }
    }

    // Re-reads the safe area into the current Gfx: forced on resize / rotation / resume / focus; Android insets can
    // arrive after the resize, so redraws re-read it too, at most every `SAFE_EVERY`.
    fn refresh_safe(&mut self, force: bool) {
        let now = Instant::now();
        if self.window.is_none() || (!force && self.safe_at.is_some_and(|t| now < t + SAFE_EVERY)) {
            return;
        }
        self.safe_at = Some(now);
        let safe = self.read_safe();
        let g = match &mut self.shell {
            Shell::Running(r) | Shell::PortMenu(r, _) => gfx(&mut r.ctx),
            Shell::Setup(ui) | Shell::Error(ui) => &mut ui.gfx,
            Shell::Loading => return,
        };
        if g.safe != safe {
            log::info!("safe area {safe:?}");
            g.safe = safe;
            self.window().request_redraw();
        }
    }

    fn host(&mut self) -> Option<&mut crate::platform::AppHost> {
        match &mut self.shell {
            Shell::Running(r) | Shell::PortMenu(r, _) => r.ctx.host_as(),
            _ => None,
        }
    }

    // Android hot-plug: the connected gamepad list, re-read every `SAFE_EVERY`; a device that went away releases
    // everything the event pad holds.
    fn refresh_pads(&mut self) {
        let now = Instant::now();
        if !cfg!(target_os = "android") || self.pads_at.is_some_and(|t| now < t + SAFE_EVERY) {
            return;
        }
        self.pads_at = Some(now);
        #[cfg(target_os = "android")]
        if let Some(list) = crate::platform::gamepad::devices()
            && let Some(h) = self.host()
        {
            let pad = &mut h.joystick.events;
            if list != pad.devices {
                for d in list.iter().filter(|d| !pad.devices.contains(d)) {
                    log::info!("gamepad connected: {d}");
                }
                let gone: Vec<_> = pad.devices.iter().filter(|d| !list.contains(d)).collect();
                for d in &gone {
                    log::info!("gamepad disconnected: {d}");
                }
                if !gone.is_empty() {
                    pad.release();
                }
                pad.devices = list;
            }
        }
    }

    // The one fullscreen owner: F11 / Alt+Enter, the launcher's Fullscreen box and the game's restart land here.
    // Setup / Error: the box follows and is saved with the other edits on Play; otherwise port.cfg is written now and
    // the game's th08.cfg window mode follows. The web window cannot go fullscreen: the mode stays windowed.
    #[cfg_attr(target_arch = "wasm32", allow(unused_variables))]
    fn set_fullscreen(&mut self, on: bool) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.display.fullscreen = on;
            self.window()
                .set_fullscreen(on.then_some(Fullscreen::Borderless(None)));
            if let Shell::Setup(ui) | Shell::Error(ui) = &mut self.shell {
                ui.screen.model_mut().set_fullscreen(on);
                self.window().request_redraw();
                return;
            }
            crate::settings::save_port_cfg(
                &self.setup.dirs,
                "fullscreen",
                if on { "on" } else { "off" },
            );
        }
        if let Shell::Running(r) | Shell::PortMenu(r, _) = &mut self.shell {
            r.ctx.set_windowed(!self.display.fullscreen);
        }
    }

    fn act(&mut self, el: &ActiveEventLoop, a: &Action) {
        match a {
            Action::Play => self.play(el),
            Action::Quit => el.exit(),
            Action::Fullscreen(on) => self.set_fullscreen(*on),
            Action::Back => {
                if let Shell::Error(mut ui) = std::mem::replace(&mut self.shell, Shell::Loading) {
                    ui.screen.model_mut().view = View::Settings;
                    self.shell = Shell::Setup(ui);
                    self.window().request_redraw();
                }
            }
            Action::Resume => self.resume_game(el),
            Action::QuitToTitle => {
                if let Shell::PortMenu(r, _) = &mut self.shell {
                    log::info!("port menu: quit to title");
                    r.quit = Some(crate::platform::quittitle::Quit::default());
                }
                self.resume_game(el);
            }
            Action::Screenshot => {
                let window = self.window().clone();
                if let Shell::PortMenu(r, _) = &mut self.shell {
                    start_screenshot(r, window_size(&window));
                    window.request_redraw();
                }
            }
            Action::Speed(rate) => {
                let window = self.window().clone();
                if let Shell::PortMenu(r, menu) = &mut self.shell {
                    set_speed(r, *rate, Instant::now());
                    menu.speed = menu_speed(r);
                    menu.no_saves = no_score_saves(r);
                    window.request_redraw();
                }
            }
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
                Some(a) => self.act(el, &a),
            },
            ev => {
                if let WindowEvent::Resized(s) = ev {
                    ui.gfx.resize(s.width, s.height);
                }
                // A touch stands in for Shift on touch screens (no keyboard at start on Android).
                let shift = match &ev {
                    WindowEvent::Touch(t) => t.phase == winit::event::TouchPhase::Started,
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
            let k = f64::from(s.scale);
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
        gfx.safe = crate::platform::safearea::read(&window, s.screen_margin);
        if let Some(mut screen) = screen
            && !self.suspended
        {
            screen.redraw(&window, &mut gfx);
        }
        self.shell = Shell::Loading;
        self.auto_at = None;
        el.set_control_flow(ControlFlow::Wait);
        let (display, vsync) = (self.display_handle(), s.vsync);
        self.spawn(el, async move {
            match load(source, *gfx, s).await {
                Ok(r) => UserEvent::Ready(r),
                Err(e) => failed(window, display, e, Some(vsync)).await,
            }
        });
    }

    // A start-up error: Shell::Error (Back returns to Setup), or a message box and exit when there is no GPU. Tool
    // runs only log it and exit.
    fn failed(&mut self, el: &ActiveEventLoop, msg: String, gfx: Option<Box<Gfx>>) {
        let tool = self.setup.is_tool_run();
        if !tool {
            crate::crashlog::report("start-up failure", &msg, None);
        }
        // Android shows no egui screen: the process ends and the task returns to the Compose launcher.
        let gfx = match gfx {
            Some(g) if !tool && cfg!(not(target_os = "android")) => g,
            _ => {
                #[cfg(not(target_arch = "wasm32"))]
                if tool {
                    std::process::exit(1);
                }
                message_box(&format!("The game could not start:\n{msg}"));
                return el.exit();
            }
        };
        let mut gfx = gfx;
        gfx.safe = self.read_safe();
        self.shell = Shell::Error(self.ui(gfx, View::Error(msg)));
    }

    // F1 / MENU control / pad Select / resume from the background: the port menu over the game; its pacer stops, audio pauses and held
    // keys are released (as on focus loss).
    fn open_port_menu(&mut self) {
        let Shell::Running(mut r) = std::mem::replace(&mut self.shell, Shell::Loading) else {
            return;
        };
        log::info!("port menu: open");
        r.pacer.suspend();
        if let Some(h) = r.ctx.host_as::<crate::platform::AppHost>() {
            h.input.set_focus(false);
        }
        crate::platform::gestures::set_exclusion(&[]);
        let touch = gfx(&mut r.ctx).overlay.is_some().then(|| {
            r.ctx
                .host_as::<crate::platform::AppHost>()
                .map_or_else(|| r.settings.touch.clone(), |h| h.input.touch.cfg.clone())
        });
        let mut menu = crate::platform::portmenu::PortMenu::new(self.window(), &r.settings, touch);
        menu.quit_ok = r.quit.is_none() && crate::platform::quittitle::available(&r.ctx.w);
        menu.cheats = r.settings.active_cheats();
        menu.speed = menu_speed(&r);
        menu.no_saves = no_score_saves(&mut r);
        self.shell = Shell::PortMenu(r, Box::new(menu));
        self.sync_audio();
        self.window().request_redraw();
    }

    // Resume: the menu's GPU objects go, the pacer deadline restarts at now (the time in the menu is not caught up).
    fn resume_game(&mut self, el: &ActiveEventLoop) {
        let Shell::PortMenu(mut r, screen) = std::mem::replace(&mut self.shell, Shell::Loading)
        else {
            return;
        };
        screen.save();
        refresh_overlays(&mut r, screen.opened_with());
        drop(screen);
        if !self.suspended {
            r.pacer.resume(Instant::now());
        }
        r.held_dirty = true;
        if let Some(h) = r.ctx.host_as::<crate::platform::AppHost>() {
            h.input.set_focus(true);
        }
        log::info!("port menu: resume");
        self.shell = Shell::Running(r);
        self.sync_audio();
        el.set_control_flow(ControlFlow::Wait);
        self.window().request_redraw();
    }

    // Back (key or OnBackInvokedCallback): in the game one X / Esc pulse; in the port menu a step back, then resume.
    fn back(&mut self, el: &ActiveEventLoop) {
        match &mut self.shell {
            Shell::Running(r) => {
                let bit = crate::platform::touch::TouchScene::back_bit_of(&r.ctx.w);
                if let Some(h) = r.ctx.host_as::<crate::platform::AppHost>() {
                    h.input.touch.back_key(bit);
                }
            }
            Shell::PortMenu(_, screen) => {
                if screen.back() {
                    self.window().request_redraw();
                } else {
                    self.resume_game(el);
                }
            }
            _ => {}
        }
    }

    fn menu_event(&mut self, el: &ActiveEventLoop, ev: WindowEvent) {
        let window = self.window().clone();
        let Shell::PortMenu(r, screen) = &mut self.shell else {
            return;
        };
        match ev {
            WindowEvent::CloseRequested => {
                screen.save();
                close(r, el);
            }
            WindowEvent::KeyboardInput { event, .. }
                if event.physical_key == PhysicalKey::Code(SCREENSHOT_KEY) =>
            {
                if event.state == ElementState::Pressed && !event.repeat {
                    start_screenshot(r, window_size(&window));
                    window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                if let Some(m) = poll_screenshot(r) {
                    screen.status = Some(m);
                } else if r.snapshot.is_some() {
                    window.request_redraw();
                }
                screen.controller = r
                    .ctx
                    .host_as::<crate::platform::AppHost>()
                    .and_then(|h| h.joystick.active_name());
                let a = screen.redraw(&window, gfx(&mut r.ctx));
                let changed = screen.take_changed();
                if let Some(rows) = &changed {
                    for (k, v) in rows {
                        apply_row(&mut r.settings, k, v);
                        apply_row(&mut self.setup, k, v);
                    }
                    apply_live(r);
                }
                if let Some(l) = screen.take_layout() {
                    self.setup.touch.layout = l.clone();
                    save_layout(r, l);
                }
                if changed.is_some() {
                    self.sync_audio();
                }
                if let Some(a) = a {
                    self.act(el, &a);
                }
            }
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed
                    && !event.repeat
                    && (is_back(&event)
                        || matches!(
                            event.physical_key,
                            PhysicalKey::Code(KeyCode::F1 | KeyCode::Escape)
                        )
                        || pad_button(&event) == Some(crate::joystick::MENU_BUTTON)) =>
            {
                log::info!("back: key {:?}", event.physical_key);
                self.back(el);
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
        self.game_event_inner(el, ev);
        if let Shell::Running(r) = &self.shell
            && r.pacer.is_held() != self.replay_held
        {
            self.sync_audio();
        }
    }

    fn game_event_inner(&mut self, el: &ActiveEventLoop, ev: WindowEvent) {
        let window = self.window().clone();
        let Shell::Running(r) = &mut self.shell else {
            return;
        };
        // Web: the AudioContext opens (or resumes) inside a user gesture.
        #[cfg(target_arch = "wasm32")]
        if is_gesture(&ev) {
            r.ctx.start();
        }
        match ev {
            WindowEvent::CloseRequested => close(r, el),
            WindowEvent::Resized(s) => {
                gfx(&mut r.ctx).resize(s.width, s.height);
                r.held_dirty = true;
                r.wake = Some(Instant::now());
            }
            WindowEvent::Occluded(false) => {
                r.held_dirty = true;
                r.wake = Some(Instant::now());
            }
            WindowEvent::Focused(f) => {
                r.held_dirty = true;
                if let Some(h) = r.ctx.host_as::<crate::platform::AppHost>() {
                    h.input.set_focus(f);
                }
                if !f && pauses_unfocused(r) {
                    self.open_port_menu();
                }
            }
            // Web: the tab was hidden.
            #[cfg(target_arch = "wasm32")]
            WindowEvent::Occluded(true) if pauses_unfocused(r) => self.open_port_menu(),
            // Port-only F1: the port menu; the key press does not reach the game.
            WindowEvent::KeyboardInput { event, .. }
                if event.physical_key == PhysicalKey::Code(KeyCode::F1) =>
            {
                if event.state == ElementState::Pressed && !event.repeat {
                    self.open_port_menu();
                }
            }
            // Port-only replay transport keys (F5–F8): never in the input word; inert while no replay plays.
            WindowEvent::KeyboardInput { event, .. } if transport_key(&event).is_some() => {
                let a = transport_key(&event).expect("transport key");
                if event.state == ElementState::Pressed && (!event.repeat || a == Transp::Step) {
                    transport_act(r, a, Instant::now());
                }
            }
            // Port-only hitbox viewer and screenshot keys: the press does not reach the game.
            WindowEvent::KeyboardInput { event, .. } if port_key(event.physical_key).is_some() => {
                if event.state == ElementState::Pressed && !event.repeat {
                    match port_key(event.physical_key) {
                        Some(SCREENSHOT_KEY) => game_screenshot(r, &window),
                        _ => {
                            crate::platform::hitbox::cycle(gfx(&mut r.ctx));
                        }
                    }
                }
            }
            // Android Back (button, gesture, a pad's Back key): one X pulse in menus, one Esc pulse in play and in the
            // pause menu, through the touch source so replays record it like a key.
            WindowEvent::KeyboardInput { event, .. } if is_back(&event) => {
                if event.state == ElementState::Pressed && !event.repeat {
                    log::info!("back: key {:?}", event.physical_key);
                    self.back(el);
                }
            }
            // Gamepad buttons (Android keycodes): Select opens the port menu in a stage.
            WindowEvent::KeyboardInput { event, .. } if pad_button(&event).is_some() => {
                let i = pad_button(&event).expect("pad button");
                let down = event.state == ElementState::Pressed;
                if i == crate::joystick::MENU_BUTTON {
                    if down && !event.repeat && in_stage(r) {
                        self.open_port_menu();
                    }
                } else if let Some(h) = r.ctx.host_as::<crate::platform::AppHost>() {
                    h.joystick.events.button(i, down);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let (PhysicalKey::Code(k), Some(h)) = (
                    event.physical_key,
                    r.ctx.host_as::<crate::platform::AppHost>(),
                ) {
                    h.input
                        .keyboard
                        .key(k, event.state == ElementState::Pressed);
                }
            }
            WindowEvent::Touch(t) => {
                if t.phase == winit::event::TouchPhase::Started
                    && let Some(o) = r.hidden_overlay.take()
                {
                    gfx(&mut r.ctx).overlay = Some(o);
                }
                let (x, y) = (t.location.x as f32, t.location.y as f32);
                if t.phase == winit::event::TouchPhase::Started
                    && let Some(a) = strip_hit(r, x, y)
                {
                    transport_act(r, a, Instant::now());
                    return;
                }
                if let Some(h) = r.ctx.host_as::<crate::platform::AppHost>()
                    && h.input.touch.cfg.active()
                {
                    let (x, y) = (t.location.x as f32, t.location.y as f32);
                    h.input.touch.touch(t.id, t.phase, x, y);
                }
            }
            // Dev `touch_mouse`: the left mouse button is a finger.
            WindowEvent::CursorMoved { position, .. } => {
                r.replay.cursor = (position.x as f32, position.y as f32);
                if let Some(h) = r.ctx.host_as::<crate::platform::AppHost>()
                    && h.input.touch.cfg.mouse
                {
                    h.input
                        .touch
                        .mouse_move(position.x as f32, position.y as f32);
                }
            }
            WindowEvent::MouseInput {
                state,
                button: winit::event::MouseButton::Left,
                ..
            } => {
                let (x, y) = r.replay.cursor;
                let mouse = r
                    .ctx
                    .host_as::<crate::platform::AppHost>()
                    .is_some_and(|h| h.input.touch.cfg.mouse);
                if mouse
                    && state == ElementState::Pressed
                    && let Some(a) = strip_hit(r, x, y)
                {
                    transport_act(r, a, Instant::now());
                    return;
                }
                if let Some(h) = r.ctx.host_as::<crate::platform::AppHost>()
                    && h.input.touch.cfg.mouse
                {
                    h.input.touch.mouse_button(state == ElementState::Pressed);
                }
            }
            WindowEvent::RedrawRequested => match frame(r, &window) {
                Frame::Continue => {
                    if pad_frame(r) {
                        self.open_port_menu();
                    }
                }
                Frame::Exit => el.exit(),
                Frame::Restarted(on) => {
                    self.set_fullscreen(on);
                    window.request_redraw();
                }
            },
            _ => {}
        }
    }
}

// After a game frame: gamepad use hides the touch overlay; a gilrs Select (in a stage) or the touch MENU control
// asks for the port menu.
fn pad_frame(r: &mut Running) -> bool {
    let stage = in_stage(r);
    let Some(h) = r.ctx.host_as::<crate::platform::AppHost>() else {
        return false;
    };
    let used = h.joystick.take_used();
    let menu = (h.joystick.take_menu() && stage) | h.input.touch.take_menu();
    #[cfg(target_arch = "wasm32")]
    if used {
        r.ctx.start();
    }
    if used && let Some(o) = gfx(&mut r.ctx).overlay.take() {
        log::info!("gamepad in use: touch overlay hidden");
        r.hidden_overlay = Some(o);
    }
    menu
}

// An input the browser counts as a user gesture for audio.
#[cfg(target_arch = "wasm32")]
fn is_gesture(ev: &WindowEvent) -> bool {
    match ev {
        WindowEvent::KeyboardInput { event, .. } => event.state == ElementState::Pressed,
        WindowEvent::MouseInput { state, .. } => *state == ElementState::Pressed,
        WindowEvent::Touch(t) => matches!(
            t.phase,
            winit::event::TouchPhase::Started | winit::event::TouchPhase::Ended
        ),
        _ => false,
    }
}

// The gamepad button index of an Android gamepad keycode event.
fn pad_button(e: &winit::event::KeyEvent) -> Option<usize> {
    match e.physical_key {
        PhysicalKey::Unidentified(winit::keyboard::NativeKeyCode::Android(c)) => {
            crate::joystick::android_button(c)
        }
        _ => None,
    }
}

// The layout editor's result: the live overlay takes it now and port.cfg keeps it ("" = the defaults).
fn save_layout(r: &mut Running, l: crate::platform::touch::TouchLayout) {
    let v = l.cfg_value();
    log::info!("touch layout saved: {v:?}");
    if let Some(h) = r.ctx.host_as::<crate::platform::AppHost>() {
        h.input.touch.cfg.layout = l.clone();
    }
    r.settings.touch.layout = l;
    let v = (!v.is_empty()).then_some(v.as_str());
    crate::settings::write_port_cfg(&r.settings.dirs, &[("touch_layout", v)]);
}

// A port menu row's value into a game's settings (the row widgets only give values the entry accepts).
fn apply_row(s: &mut Settings, key: &str, v: &str) {
    if let Err(e) = s.set_from(crate::settings::Origin::Launcher, key, v) {
        log::warn!("port menu: {key} = {v}: {e}");
    }
}

// The port menu's live rows on the held game at once: picture filters, port volume, touch opacity. Mute, the
// overlays and the screenshot size are read from the settings where they are used.
fn apply_live(r: &mut Running) {
    gfx(&mut r.ctx).blit_options = r.settings.blit;
    if let Some(v) = &r.volume {
        v.set(r.settings.volume);
    }
    let opacity = r.settings.touch.opacity;
    if let Some(h) = r.ctx.host_as::<crate::platform::AppHost>() {
        h.input.touch.cfg.opacity = opacity;
    }
}

// On resume: the HUD and touch overlays are made again when the language or `now_playing` changed in the menu.
fn refresh_overlays(r: &mut Running, (lang, now_playing): (crate::i18n::Lang, bool)) {
    let new_lang = crate::i18n::resolve(r.settings.language);
    if new_lang == lang && r.settings.now_playing == now_playing {
        return;
    }
    let font = r.settings.font.clone();
    let g = gfx(&mut r.ctx);
    g.hud = Some(Box::new(crate::platform::overlay::TransportOverlay::new(
        new_lang,
        font.clone(),
        r.settings.now_playing,
    )));
    if new_lang == lang {
        return;
    }
    let touch = || -> Box<dyn th08_render::gfx::Overlay> {
        Box::new(crate::platform::overlay::TouchOverlay::new(
            new_lang,
            font.clone(),
        ))
    };
    if g.overlay.is_some() {
        g.overlay = Some(touch());
    }
    if r.hidden_overlay.is_some() {
        r.hidden_overlay = Some(touch());
    }
}

// In a stage (scene game): where pad Select and a resume from the background open the port menu.
fn in_stage(r: &Running) -> bool {
    r.ctx.w.game.scene == th08_core::types::SceneId::Game
}

// `pause_unfocused`: focus loss in a stage opens the port menu (Android: notification shade, split screen, dialogs).
fn pauses_unfocused(r: &Running) -> bool {
    r.settings.pause_unfocused && !r.settings.is_tool_run() && in_stage(r)
}

// The Android Back button (logical BrowserBack / GoBack, physical BrowserBack); on Android Escape counts too. With
// the OnBackInvokedCallback active the Back key arrives there as well, so only Escape is taken as a key.
fn is_back(e: &winit::event::KeyEvent) -> bool {
    use winit::keyboard::{Key, NamedKey};
    let esc = e.physical_key == PhysicalKey::Code(KeyCode::Escape);
    if crate::platform::back::callback_active() && !esc {
        return false;
    }
    matches!(
        e.logical_key,
        Key::Named(NamedKey::BrowserBack | NamedKey::GoBack)
    ) || e.physical_key == PhysicalKey::Code(KeyCode::BrowserBack)
        || (cfg!(target_os = "android") && esc)
}

// Closing the window mid-game: score.dat is written from a World clone (play time, card history, unlocks since the
// last save point). Tool `replay_save`: the recording is still written.
fn close(r: &mut Running, el: &ActiveEventLoop) {
    if r.ctx.core.replay_save.is_some() && r.ctx.w.game.scene == th08_core::types::SceneId::Game {
        th08_core::replay::finish(&mut r.ctx);
    }
    close_save(r);
    el.exit();
}

// The score.dat bytes written at a window close (None: tool runs write nothing).
fn close_save(r: &mut Running) -> Option<Vec<u8>> {
    if r.settings.is_tool_run() {
        return None;
    }
    let bytes = r.ctx.save_from_clone();
    log::info!(
        "close: score.dat saved from a World clone ({} bytes)",
        bytes.len()
    );
    Some(bytes)
}

// One redraw of the running game: Frame_Run timing (00 §2, pacer.rs), the logic frames, draw and Present.
fn frame(r: &mut Running, window: &Window) -> Frame {
    let now = Instant::now();
    r.wake = None;
    if let Some(m) = poll_screenshot(r) {
        r.toast = Some((m, now + TOAST));
        r.held_dirty = true;
    }
    follow_replay(r, now);
    follow_practice(r, now);
    r.discord.observe(&r.ctx.w.game);
    let shot = r.settings.shot;
    let (per_draw, draws) = match r.pacer.poll(now) {
        Tick::Run { per_draw, draws } => (per_draw, draws),
        Tick::Suspended => return Frame::Continue,
        // Paused replay: the last picture is presented again only when it or the HUD changes.
        Tick::Held => {
            let changed = update_hud(r, window, now);
            if (changed || std::mem::take(&mut r.held_dirty)) && !gfx(&mut r.ctx).present_held() {
                r.held_dirty = true;
            }
            r.wake = Some(now + HELD_POLL);
            return Frame::Continue;
        }
        // Native: the event loop sleeps until SPIN before the deadline, then ≤ 1 ms sleeps for Sleep(0).
        #[cfg_attr(target_arch = "wasm32", allow(unused_variables))]
        Tick::Wait(left) => {
            #[cfg(not(target_arch = "wasm32"))]
            {
                if let Some(d) = r.pacer.next_deadline()
                    && d > now + SPIN
                {
                    r.wake = Some(d - SPIN);
                    return Frame::Continue;
                }
                std::thread::sleep(std::time::Duration::from_secs_f64(left.min(0.001)));
            }
            window.request_redraw();
            return Frame::Continue;
        }
    };
    // Web `?shot=<n>`: logic freezes at frame n so the page can be screenshotted.
    let frozen = cfg!(target_arch = "wasm32") && shot.is_some_and(|f| r.ctx.w.frame >= f);
    let touch = gfx(&mut r.ctx).overlay.is_some();
    let safe = gfx(&mut r.ctx).safe;
    if touch && let Some(h) = r.ctx.host_as::<crate::platform::AppHost>() {
        let size = window.inner_size();
        // A `--shot` through the scaling blit is laid out for the shot's size.
        let (w, hgt) = r.settings.shot_size.unwrap_or((size.width, size.height));
        h.input
            .touch
            .set_window(w, hgt, window.scale_factor(), safe);
    }
    if let Some(f) = run_batch(r, per_draw, draws, frozen, touch) {
        return f;
    }
    let size = window.inner_size();
    gfx(&mut r.ctx).resize(size.width, size.height);
    if touch {
        update_overlay(&mut r.ctx);
    }
    update_hud(r, window, now);
    let excl = if touch {
        r.ctx
            .host_as::<crate::platform::AppHost>()
            .map(|h| h.input.touch.exclusion())
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    crate::platform::gestures::set_exclusion(&excl);
    crate::platform::hitbox::update(&mut r.ctx);
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
    r.pacer
        .record_work(now.elapsed().as_secs_f64(), per_draw * draws);
    r.ctx.present();
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(hz) = r.settings.present_hz {
        sim_vsync(r.vsync_epoch, f64::from(hz));
    }
    pace_log(r, now);
    th08_core::ascii::apply_pending_captures(&mut r.ctx.w.anm, &mut r.ctx.rec);
    window.request_redraw();
    Frame::Continue
}

// Debug `present_hz`: the present blocks until the next tick of an `hz` grid, like Fifo on such a display.
#[cfg(not(target_arch = "wasm32"))]
fn sim_vsync(epoch: Instant, hz: f64) {
    let t = epoch.elapsed().as_secs_f64();
    let next = ((t * hz + 1e-6).floor() + 1.0) / hz;
    std::thread::sleep(std::time::Duration::from_secs_f64(next - t));
}

// Logic frames and presents per wall second, every 10 s (info with `present_hz`, else debug).
fn pace_log(r: &mut Running, now: Instant) {
    let Some((secs, frames, runs)) = r.pacer.take_stats(now, 10.0) else {
        return;
    };
    let level = if r.settings.present_hz.is_some() {
        log::Level::Info
    } else {
        log::Level::Debug
    };
    log::log!(
        level,
        "pace: {:.2} logic frames/s, {:.2} presents/s over {secs:.1} s{}",
        frames as f64 / secs,
        runs as f64 / secs,
        if r.pacer.is_catching_up() {
            " (catch-up)"
        } else {
            ""
        }
    );
}

// Replay transport above 1× or the pacer's catch-up below 60 Hz: `draws` × (`per_draw` logic frames, draw); every draw
// but the last runs with emit off (draw-phase steps only), the last is the caller's, recorded and presented. Above 1×
// fewer when playback ends inside.
fn run_batch(
    r: &mut Running,
    per_draw: u32,
    draws: u32,
    frozen: bool,
    touch: bool,
) -> Option<Frame> {
    for d in 0..draws {
        if let Some(f) = logic_frames(r, per_draw, frozen, touch) {
            return Some(f);
        }
        // Above 1× the batch ends with playback; at 1× it is the pacer's catch-up and every frame runs.
        if !replay_active(&r.ctx) && !r.pacer.is_original() {
            break;
        }
        if let Some(p) = r.ctx.w.player.as_deref() {
            r.replay.score = p.stats.score10 * 10;
        }
        if d + 1 == draws {
            break;
        }
        r.ctx.set_emit(false);
        r.ctx.draw();
        r.ctx.set_emit(true);
    }
    None
}

// `n` logic frames before one draw (n > 1: debug `TH08_TURBO=<n>`, no frame pacing); Some: the frame ends here.
fn logic_frames(r: &mut Running, n: u32, frozen: bool, touch: bool) -> Option<Frame> {
    let shot = r.settings.shot;
    for _ in 0..n {
        if shot.is_some_and(|f| r.ctx.w.frame >= f) && r.pacer.turbo() > 0 {
            break;
        }
        // Touch drag steers towards the player's position of this frame (a read between frames).
        if touch {
            let scene = crate::platform::touch::TouchScene::of(&r.ctx.w);
            if let Some(h) = r.ctx.host_as::<crate::platform::AppHost>() {
                h.input.touch.set_scene(scene);
            }
        }
        quit_step(r);
        let flow = if frozen { Flow::Continue } else { r.ctx.calc() };
        if !frozen && let Some(st) = &mut r.stats {
            st.frame(&r.ctx.w);
        }
        if let Some(h) = r.ctx.host_as::<crate::platform::AppHost>() {
            h.clock.frame();
        }
        match flow {
            Flow::Continue => {}
            // Frame_Run returns after the calc chain and Sound_Update: no draw, no Present (15 §2.2, 00 §2).
            Flow::Quit => {
                r.ctx.exit_save();
                return Some(Frame::Exit);
            }
            // Result 2 (00 §2): the program restarts in the live config's mode; this frame has no draw and no
            // Present. Port: window mode +0x22 is applied as the port's fullscreen setting.
            Flow::Error => {
                if let Err(e) = r.ctx.restart() {
                    log::error!("restart failed: {e:#}");
                    return Some(Frame::Exit);
                }
                return Some(Frame::Restarted(r.ctx.w.game.cfg.windowed == 0));
            }
        }
    }
    None
}

// Port menu "Quit to title": the next Esc / Q pulse into the touch input, before a logic frame.
fn quit_step(r: &mut Running) {
    let Some(q) = &mut r.quit else { return };
    let pending = r
        .ctx
        .host_as::<crate::platform::AppHost>()
        .is_some_and(|h| h.input.touch.pulse_pending());
    match q.next(&r.ctx.w, pending) {
        Some(Some(bit)) => {
            if let Some(h) = r.ctx.host_as::<crate::platform::AppHost>() {
                h.input.touch.back_key(bit);
            }
        }
        Some(None) => {}
        None => r.quit = None,
    }
}

// A replay (not an attract demo) is playing.
fn replay_active(ctx: &Ctx) -> bool {
    let g = &ctx.w.game;
    g.replay() && g.rpy.is_some() && !g.has(th08_core::gamemgr::GameFlags::DEMO)
}

// Playback started: the transport starts at the `replay_rate` setting; ended: back to 1× and the final score logged.
fn follow_replay(r: &mut Running, now: Instant) {
    let on = replay_active(&r.ctx);
    if on && !r.replay.playing {
        r.replay.t = Transport {
            rate: r.settings.replay_rate,
            paused: false,
        };
        if r.replay.t != Transport::default() {
            r.replay.label_until = Some(now + LABEL);
        }
        apply_transport(r, now);
    } else if !on && r.replay.playing {
        log::info!("replay: playback ended, score {}", r.replay.score);
        r.replay.t = Transport::default();
        r.replay.label_until = None;
        apply_transport(r, now);
    }
    r.replay.playing = on;
}

fn apply_transport(r: &mut Running, now: Instant) {
    let t = r.replay.t;
    let rate = if r.replay.playing || replay_active(&r.ctx) {
        t.ratio()
    } else {
        crate::transport::SLOW[r.slow]
    };
    if r.pacer.rate() != rate {
        r.pacer.set_rate(rate, now);
    }
    if r.pacer.is_held() != t.paused {
        r.pacer.hold(t.paused, now);
    }
    // Off 1× the game's clock is virtual from this moment, so time spent paused or slowed never reaches it.
    let virt = !r.pacer.is_original();
    if let Some(h) = r.ctx.host_as::<crate::platform::AppHost>() {
        h.clock.set_virtual(virt);
    }
}

// Live practice (stage or spell practice): slow motion may be set in a stage and stays through retry,
// reload and the next stage.
fn practice_live(g: &th08_core::gamemgr::Game, set: bool) -> bool {
    use th08_core::types::SceneId;
    let scene_ok = if set {
        g.scene == SceneId::Game
    } else {
        matches!(
            g.scene,
            SceneId::Game | SceneId::Retry | SceneId::Reload | SceneId::NextStage
        )
    };
    (g.practice() || g.spell_practice()) && !g.replay() && scene_ok
}

fn slow_allowed(r: &Running) -> bool {
    practice_live(&r.ctx.w.game, true)
}

// Slow motion ends with the practice run.
fn follow_practice(r: &mut Running, now: Instant) {
    if r.slow != 0 && !practice_live(&r.ctx.w.game, false) {
        log::info!("practice slow motion: off (practice left)");
        r.slow = 0;
        apply_transport(r, now);
    }
}

// The port menu's speed row: the rate now, None when it is hidden (no live practice stage, no replay playing).
fn menu_speed(r: &Running) -> Option<(u32, u32)> {
    if r.replay.playing {
        Some(r.replay.t.ratio())
    } else if slow_allowed(r) {
        Some(crate::transport::SLOW[r.slow])
    } else {
        None
    }
}

fn no_score_saves(r: &mut Running) -> bool {
    r.ctx
        .host_as::<crate::platform::AppHost>()
        .is_some_and(|h| h.no_score_saves)
}

// Port menu speed: the replay transport rate during playback, else practice slow motion. A slowed live run keeps
// no score.dat or replay for the rest of the session.
fn set_speed(r: &mut Running, rate: (u32, u32), now: Instant) {
    if r.replay.playing {
        if let Some(i) = crate::transport::RATES.iter().position(|&x| x == rate) {
            r.replay.t.rate = i;
        }
    } else if slow_allowed(r) {
        let Some(i) = crate::transport::SLOW.iter().position(|&x| x == rate) else {
            return;
        };
        r.slow = i;
        if i != 0 && !no_score_saves(r) {
            if let Some(h) = r.ctx.host_as::<crate::platform::AppHost>() {
                h.no_score_saves = true;
            }
            log::warn!(
                "practice slow motion: score.dat and replays are not saved for the rest of the session"
            );
            let lang = crate::i18n::resolve(r.settings.language);
            r.toast = Some((
                crate::i18n::ui(lang, "slow_nosave").to_string(),
                now + TOAST,
            ));
        }
    } else {
        return;
    }
    log::info!(
        "speed: {}",
        crate::transport::rate_label(crate::i18n::Lang::En, rate)
    );
    apply_transport(r, now);
    r.replay.label_until = Some(now + LABEL);
}

fn transport_act(r: &mut Running, a: Transp, now: Instant) {
    if !r.replay.playing {
        return;
    }
    r.replay.t.act(a);
    apply_transport(r, now);
    if a == Transp::Step {
        r.pacer.step();
    }
    r.held_dirty = true;
    r.wake = Some(now);
    r.replay.label_until = Some(now + LABEL);
    log::info!(
        "replay transport: {}",
        r.replay.t.label(crate::i18n::Lang::En)
    );
}

fn transport_key(e: &winit::event::KeyEvent) -> Option<Transp> {
    match e.physical_key {
        PhysicalKey::Code(k) => Transp::of_key(k),
        PhysicalKey::Unidentified(_) => None,
    }
}

fn strip_hit(r: &Running, x: f32, y: f32) -> Option<Transp> {
    r.replay
        .strip
        .as_ref()
        .and_then(|s| crate::transport::hit(s, x, y))
}

// Port-only screenshot key.
const SCREENSHOT_KEY: KeyCode = KeyCode::F12;

// The in-game hitbox viewer and screenshot keys.
fn port_key(k: PhysicalKey) -> Option<KeyCode> {
    match k {
        PhysicalKey::Code(c) if c == SCREENSHOT_KEY || c == crate::platform::hitbox::KEY => Some(c),
        _ => None,
    }
}

// Starts a screenshot of the last rendered frame (640×480, or the window size with `screenshot_size = window`).
fn start_screenshot(r: &mut Running, window: (u32, u32)) {
    if r.snapshot.is_some() {
        return;
    }
    let size = r.settings.screenshot_window.then_some(window);
    r.snapshot = Some(gfx(&mut r.ctx).snapshot(size));
}

fn window_size(w: &Window) -> (u32, u32) {
    let s = w.inner_size();
    (s.width, s.height)
}

// F12 in the game: the toast shows once the PNG is saved (native: at once).
fn game_screenshot(r: &mut Running, window: &Window) {
    start_screenshot(r, window_size(window));
    if let Some(m) = poll_screenshot(r) {
        r.toast = Some((m, Instant::now() + TOAST));
        r.held_dirty = true;
        window.request_redraw();
    }
}

// A finished screenshot saved: the toast text (None while none is pending or its copy is still in flight).
fn poll_screenshot(r: &mut Running) -> Option<String> {
    let res = r.snapshot.as_ref()?.poll(&gfx(&mut r.ctx).device)?;
    r.snapshot = None;
    #[cfg(not(target_arch = "wasm32"))]
    let dir = r.settings.storage_dir();
    #[cfg(target_arch = "wasm32")]
    let dir: Option<std::path::PathBuf> = None;
    let lang = crate::i18n::resolve(r.settings.language);
    Some(
        match res.and_then(|img| crate::platform::screenshot::save(&img, dir.as_deref())) {
            Ok(p) => {
                log::info!("screenshot: {p}");
                crate::i18n::ui_fmt(lang, "screenshot_saved", &[("path", &p)])
            }
            Err(e) => {
                log::warn!("screenshot failed: {e}");
                crate::i18n::ui_fmt(lang, "screenshot_failed", &[("err", &e)])
            }
        },
    )
}

// The transport HUD: the label while it lasts (always while paused), the strip while a replay plays with touch on.
// True when what it shows changed.
fn update_hud(r: &mut Running, window: &Window, now: Instant) -> bool {
    let touch = gfx(&mut r.ctx).overlay.is_some();
    let size = window.inner_size();
    r.replay.strip = (r.replay.playing && touch).then(|| {
        let safe = gfx(&mut r.ctx).safe.rect(size.width, size.height);
        crate::transport::strip(safe, window.scale_factor() as f32)
    });
    let t = r.replay.t;
    let lang = crate::i18n::resolve(r.settings.language);
    let fresh = r.replay.label_until.is_some_and(|u| now < u);
    let label = if r.replay.playing {
        (t.paused || fresh).then(|| t.label(lang))
    } else {
        (r.slow != 0 && fresh)
            .then(|| crate::transport::rate_label(lang, crate::transport::SLOW[r.slow]))
    };
    let strip = r.replay.strip;
    if let Some(stem) = r
        .started
        .as_ref()
        .and_then(crate::nowplaying::Started::take)
        && r.settings.now_playing
        && let Some(title) = crate::nowplaying::title(&r.titles, &stem)
    {
        let text = crate::i18n::ui_fmt(lang, "now_playing", &[("title", title)]);
        r.now_playing = Some((text, now + NOW_PLAYING));
    }
    if r.now_playing
        .as_ref()
        .is_some_and(|t| now >= t.1 || !r.settings.now_playing)
    {
        r.now_playing = None;
    }
    let now_playing = r.now_playing.as_ref().map(|t| t.0.clone());
    let inputs = r.settings.input_display.map(|corner| {
        let i = &r.ctx.w.input;
        crate::platform::overlay::InputView {
            word: if in_stage(r) { i.game[0] } else { i.held },
            corner,
            opacity: r.settings.input_opacity,
        }
    });
    let safe = gfx(&mut r.ctx).safe.rect(size.width, size.height);
    if r.toast.as_ref().is_none_or(|t| now >= t.1)
        && let Some((name, err)) = crate::crashlog::take_save_error()
    {
        let text = crate::i18n::ui_fmt(lang, "save_failed", &[("name", &name), ("err", &err)]);
        r.toast = Some((text, now + TOAST));
    }
    if r.toast.as_ref().is_some_and(|t| now >= t.1) {
        r.toast = None;
    }
    let toast = r.toast.as_ref().map(|t| t.0.clone());
    if let Some(o) = gfx(&mut r.ctx).hud.as_mut().and_then(|o| {
        o.as_any_mut()
            .downcast_mut::<crate::platform::overlay::TransportOverlay>()
    }) {
        let view = crate::platform::overlay::TransportView {
            label,
            strip,
            paused: t.paused,
            toast,
            now_playing,
            inputs,
        };
        o.safe = safe;
        if o.view != view {
            o.view = view;
            return true;
        }
    }
    false
}

const LABEL: std::time::Duration = std::time::Duration::from_millis(1500);
// Native frame wait: the last stretch before a deadline, slept in ≤ 1 ms steps instead of left to the event loop.
#[cfg(not(target_arch = "wasm32"))]
const SPIN: std::time::Duration = std::time::Duration::from_micros(1500);
// A paused replay re-checks its HUD (label, save toast) this often.
const HELD_POLL: std::time::Duration = std::time::Duration::from_millis(200);
const TOAST: std::time::Duration = std::time::Duration::from_secs(5);
const NOW_PLAYING: std::time::Duration = std::time::Duration::from_secs(4);

// Hands the touch state to the overlay the renderer draws after the blit.
fn update_overlay(ctx: &mut Ctx) {
    let Some(view) = ctx
        .host_as::<crate::platform::AppHost>()
        .map(|h| h.input.touch.view())
    else {
        return;
    };
    if let Some(o) = gfx(ctx).overlay.as_mut().and_then(|o| {
        o.as_any_mut()
            .downcast_mut::<crate::platform::overlay::TouchOverlay>()
    }) {
        o.view = view;
    }
}

// gilrs pump interval while the port menu is open.
const PAD_PUMP: std::time::Duration = std::time::Duration::from_millis(50);

// Safe-area re-read interval on redraws (`App::refresh_safe`, `App::refresh_pads`).
const SAFE_EVERY: std::time::Duration = std::time::Duration::from_secs(1);

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
        let k = f64::from(scale);
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

#[derive(Clone, Copy, Debug, PartialEq)]
enum SurfaceFix {
    Keep,
    Detach,
    Reattach,
}

// What a device made by a start-up step that began at `made_at` needs now: none while suspended, a new one when a
// `Suspended` came in between.
fn surface_fix(made_at: u32, now: u32, suspended: bool) -> SurfaceFix {
    if suspended {
        SurfaceFix::Detach
    } else if made_at != now {
        SurfaceFix::Reattach
    } else {
        SurfaceFix::Keep
    }
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

// Android: the game on the NativeActivity's event loop, never a settings screen: the Compose launcher is the only one,
// and a `:game` process recreated after a background kill starts at the title. Game files: `source` (bundled APK
// assets), else the `game_dir` folder.
#[cfg(target_os = "android")]
pub fn run_android(
    android: winit::platform::android::activity::AndroidApp,
    source: Option<Source>,
    s: Settings,
) {
    use winit::platform::android::EventLoopBuilderExtAndroid;
    let Some(source) = source.or_else(|| s.game_dir.clone().map(crate::sources::native)) else {
        log::error!("no game folder: the game cannot start");
        crate::crashlog::report(
            "start-up failure",
            "no game folder: import the game files",
            None,
        );
        return;
    };
    crate::platform::safearea::set_app(&android);
    crate::platform::back::bind(&android);
    crate::platform::gamepad::bind_hat();
    let el = EventLoop::<UserEvent>::with_user_event()
        .with_android_app(android)
        .build()
        .expect("event loop");
    run_on(el, Some(source), s, false);
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

#[cfg(test)]
mod tests {
    use super::*;

    // A `Running` over the game files that plays demo 0's record as an ordinary replay; None without TH08_DIR.
    fn running(tag: &str) -> Option<Running> {
        let Some(dir) = crate::settings::test_game_dir() else {
            let t = std::thread::current();
            eprintln!(
                "skipped: no game data (TH08_DIR unset): {}",
                t.name().unwrap_or(tag)
            );
            return None;
        };
        let src = crate::sources::native(dir);
        let base = crate::settings::get().clone();
        let load = |s: &Settings| {
            let io = s.tool_io(th08_core::drawlist::NullRenderer, &src);
            pollster::block_on(Ctx::load_with(src.clone(), io, s.core(), false)).unwrap()
        };
        let bytes = load(&base).archive.read("demorpy0.rpy").unwrap();
        let path =
            std::env::temp_dir().join(format!("th08_transport_{}_{tag}.rpy", std::process::id()));
        std::fs::write(&path, bytes).unwrap();
        let mut s = base;
        s.replay = Some(path.to_string_lossy().into_owned());
        // Own save dir per test: parallel tests would otherwise overwrite each other's score.dat.
        let saves =
            std::env::temp_dir().join(format!("th08_transport_{}_{tag}", std::process::id()));
        std::fs::create_dir_all(&saves).unwrap();
        s.save_dir = Some(saves);
        let ctx = load(&s);
        Some(Running {
            ctx,
            pacer: Pacer::new(0),
            vsync_epoch: Instant::now(),
            settings: s,
            hidden_overlay: None,
            replay: ReplayTransport::default(),
            toast: None,
            snapshot: None,
            wake: None,
            held_dirty: false,
            slow: 0,
            started: None,
            titles: Vec::new(),
            now_playing: None,
            discord: crate::discord::Rpc::off(),
            stats: None,
            volume: None,
            quit: None,
        })
    }

    // F12 path on a headless GPU: 300 frames, then the read-back of the frame saved as PNG under
    // `<save dir>/screenshots/`, equal to the frame `capture` renders; the window-size variant scales it.
    #[test]
    fn screenshot_saves_frame() {
        let Some(dir) = crate::settings::test_game_dir() else {
            eprintln!("skipped: no game data (TH08_DIR unset)");
            return;
        };
        let Ok(g) = pollster::block_on(Gfx::headless()) else {
            eprintln!("skipped: no GPU adapter");
            return;
        };
        let src = crate::sources::native(dir);
        let mut s = crate::settings::get().clone();
        let saves = std::env::temp_dir().join(format!("th08_screenshot_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&saves);
        std::fs::create_dir_all(&saves).unwrap();
        s.save_dir = Some(saves.clone());
        let io = s.tool_io(g, &src);
        let ctx = pollster::block_on(Ctx::load_with(src.clone(), io, s.core(), false)).unwrap();
        let mut r = Running {
            ctx,
            pacer: Pacer::new(0),
            vsync_epoch: Instant::now(),
            settings: s,
            hidden_overlay: None,
            replay: ReplayTransport::default(),
            toast: None,
            snapshot: None,
            wake: None,
            held_dirty: false,
            slow: 0,
            started: None,
            titles: Vec::new(),
            now_playing: None,
            discord: crate::discord::Rpc::off(),
            stats: None,
            volume: None,
            quit: None,
        };
        for _ in 0..300 {
            r.ctx.calc();
            r.ctx.draw();
            r.ctx.present();
        }
        r.ctx.calc();
        r.ctx.draw();
        let (g, rec) = gfx_rec(&mut r.ctx);
        let want = g.capture(rec, None);
        start_screenshot(&mut r, (1280, 960));
        let msg = poll_screenshot(&mut r).expect("native read-back is immediate");
        let shots: Vec<_> = std::fs::read_dir(saves.join("screenshots"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        assert_eq!(shots.len(), 1, "{msg}");
        let name = shots[0].file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with("th08_") && name.ends_with(".png") && msg.contains(&name));
        let got = image::open(&shots[0]).unwrap().to_rgb8();
        assert_eq!(got.dimensions(), (640, 480));
        let want = image::DynamicImage::ImageRgba8(want).to_rgb8();
        assert!(got == want, "screenshot differs from the rendered frame");
        assert!(
            got.pixels().any(|p| p != got.get_pixel(0, 0)),
            "blank frame"
        );
        std::fs::remove_file(&shots[0]).unwrap();
        r.settings.screenshot_window = true;
        start_screenshot(&mut r, (1280, 960));
        poll_screenshot(&mut r).unwrap();
        let p = std::fs::read_dir(saves.join("screenshots"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        assert_eq!(image::image_dimensions(&p).unwrap(), (1280, 960));
        let _ = std::fs::remove_dir_all(&saves);
    }

    // Port menu "Quit to title" in stage 1: the Esc / Q pulses take the game's own pause menu to the title scene;
    // score.dat keeps every record.
    #[test]
    fn quit_to_title() {
        let Some(dir) = crate::settings::test_game_dir() else {
            eprintln!("skipped: no game data (TH08_DIR unset)");
            return;
        };
        let src = crate::sources::native(dir);
        let mut s = crate::settings::get().clone();
        s.set_from(crate::settings::Origin::Cli, "stage", "1")
            .unwrap();
        let saves = std::env::temp_dir().join(format!("th08_quit_title_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&saves);
        std::fs::create_dir_all(&saves).unwrap();
        s.save_dir = Some(saves.clone());
        let mut io = s.tool_io(th08_core::drawlist::NullRenderer, &src);
        io.host = s.app_host();
        let ctx = pollster::block_on(Ctx::load_with(src.clone(), io, s.core(), true)).unwrap();
        let mut r = Running {
            ctx,
            pacer: Pacer::new(0),
            vsync_epoch: Instant::now(),
            settings: s,
            hidden_overlay: None,
            replay: ReplayTransport::default(),
            toast: None,
            snapshot: None,
            wake: None,
            held_dirty: false,
            slow: 0,
            started: None,
            titles: Vec::new(),
            now_playing: None,
            discord: crate::discord::Rpc::off(),
            stats: None,
            volume: None,
            quit: None,
        };
        let step = |r: &mut Running| {
            assert!(logic_frames(r, 1, false, false).is_none());
            r.ctx.draw();
            r.ctx.present();
        };
        for _ in 0..300 {
            step(&mut r);
        }
        let scene = |r: &Running| r.ctx.w.game.scene;
        assert_eq!(scene(&r), th08_core::types::SceneId::Game);
        assert!(crate::platform::quittitle::available(&r.ctx.w));
        let score = || std::fs::read(saves.join("score.dat")).ok();
        let before = score();
        r.quit = Some(crate::platform::quittitle::Quit::default());
        let mut n = 0;
        let mut paused = false;
        while r.quit.is_some() {
            step(&mut r);
            paused |= r.ctx.w.game.paused != 0;
            n += 1;
            assert!(n < 1000, "quit to title did not finish");
        }
        assert!(paused, "the pause menu never opened");
        assert_eq!(
            scene(&r),
            th08_core::types::SceneId::Menu,
            "after {n} frames"
        );
        assert!(!crate::platform::quittitle::available(&r.ctx.w));
        eprintln!(
            "title after {n} frames; score.dat {} → {} bytes",
            before.as_ref().map_or(0, Vec::len),
            score().as_ref().map_or(0, Vec::len)
        );
        for _ in 0..120 {
            step(&mut r);
        }
        assert_eq!(scene(&r), th08_core::types::SceneId::Menu);
        // The pause-menu quit saves (Result_Register(2)): only the PLST play statistics (15 §2.9) and the
        // container header may differ; scores, clears and card history stay as they were.
        let old = th08_formats::scoredat::decode_container(&before.unwrap()).unwrap();
        let new = th08_formats::scoredat::decode_container(&score().unwrap()).unwrap();
        assert_eq!(old.len(), new.len());
        let plst = old.windows(4).position(|w| w == b"PLST").unwrap();
        let header = old.windows(4).position(|w| w == b"HSCR").unwrap();
        for i in (0..old.len()).filter(|&i| old[i] != new[i]) {
            assert!(
                i < header || (plst..plst + 0x228).contains(&i),
                "score.dat byte {i:#x} changed"
            );
        }
        let _ = std::fs::remove_dir_all(&saves);
    }

    fn state(r: &Running) -> (u64, i8, [i64; 22]) {
        let w = &r.ctx.w;
        (
            w.frame,
            w.skip_counter,
            th08_core::trace::demo_sample(&r.ctx, 0),
        )
    }

    // One presented frame as `frame` runs it, without the window.
    fn present(r: &mut Running, per_draw: u32, draws: u32) {
        assert!(run_batch(r, per_draw, draws, false, false).is_none());
        r.ctx.draw();
        r.ctx.present();
    }

    // 4×: the same states as 1× at every fourth frame (each logic frame keeps its draw).
    #[test]
    fn four_x_matches_one_x() {
        const N: usize = 1600;
        let Some(mut a) = running("4x_a") else {
            return;
        };
        let mut b = running("4x_b").unwrap();
        let mut want = Vec::new();
        for _ in 0..N {
            present(&mut a, 1, 1);
            want.push(state(&a));
        }
        assert!(replay_active(&a.ctx) && a.ctx.w.game.scene == th08_core::types::SceneId::Game);
        for k in 0..N / 4 {
            present(&mut b, 1, 4);
            assert_eq!(state(&b), want[4 * k + 3], "4× batch {k}");
        }
        assert!(want[N - 1].2[0] > 600, "game frames {}", want[N - 1].2[0]);
    }

    #[test]
    fn surface_fix_after_suspend() {
        assert_eq!(surface_fix(0, 0, false), SurfaceFix::Keep);
        assert_eq!(surface_fix(0, 1, false), SurfaceFix::Reattach);
        assert_eq!(surface_fix(0, 1, true), SurfaceFix::Detach);
        assert_eq!(surface_fix(3, 3, true), SurfaceFix::Detach);
        assert_eq!(surface_fix(u32::MAX, 0, false), SurfaceFix::Reattach);
    }

    // The start-up load yields to the event loop: a no-op-waker poll comes back pending before it is ready.
    #[test]
    fn yield_now_is_pending_once() {
        let mut f = std::pin::pin!(th08_core::assets::yield_now());
        let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
        assert!(f.as_mut().poll(&mut cx).is_pending());
        assert!(f.as_mut().poll(&mut cx).is_ready());
    }

    // A window close mid-stage writes score.dat from a clone: the game state (RNG included) is untouched.
    #[test]
    fn close_saves_from_clone() {
        let Some(mut r) = running("close") else {
            return;
        };
        for _ in 0..900 {
            present(&mut r, 1, 1);
        }
        assert_eq!(r.ctx.w.game.scene, th08_core::types::SceneId::Game);
        let before = state(&r);
        let rng = r.ctx.w.rng.clone();
        // Test loads skip the save files; a real run writes them, so this one does too.
        r.ctx.w.game.persist = true;
        let saved = close_save(&mut r).expect("not a tool run");
        assert_eq!(state(&r), before);
        assert_eq!(format!("{:?}", r.ctx.w.rng), format!("{rng:?}"));
        assert!(!saved.is_empty());
        // Play time comes from the wall clock, so a second save may differ; the written file must be these bytes.
        let file = r.ctx.host.read("score.dat");
        assert!(
            file.as_deref() == Some(&saved[..]),
            "score.dat {:?} bytes, saved {} bytes",
            file.as_ref().map(Vec::len),
            saved.len()
        );
        r.settings.shot = Some(1);
        assert!(close_save(&mut r).is_none());
    }

    // Paused: no logic frames; a step runs exactly one; resuming runs again.
    // Slow motion is offered in a live practice stage, kept through retry and reload, dropped at the result screen
    // and never offered for a normal run or a replay.
    #[test]
    fn practice_slow_scope() {
        use th08_core::gamemgr::{Game, GameFlags};
        use th08_core::types::SceneId;
        let mut g = Game::default();
        g.scene = SceneId::Game;
        assert!(!practice_live(&g, true));
        g.set_game_flags(GameFlags::PRACTICE);
        assert!(practice_live(&g, true));
        g.scene = SceneId::Retry;
        assert!(!practice_live(&g, true) && practice_live(&g, false));
        g.scene = SceneId::Result;
        assert!(!practice_live(&g, false));
        g.scene = SceneId::Game;
        g.set_game_flags(GameFlags::SPELL_PRACTICE);
        assert!(practice_live(&g, true));
        g.set_game_flags(GameFlags::PRACTICE | GameFlags::REPLAY);
        assert!(!practice_live(&g, true));
    }

    #[test]
    fn pause_and_step() {
        let Some(mut r) = running("pause") else {
            return;
        };
        let t0 = Instant::now();
        r.pacer.start(t0);
        follow_replay(&mut r, t0);
        assert!(r.replay.playing);
        // Polls 20 ms apart on a synthetic clock: always past the 1/60 s deadline.
        let mut ms = 0;
        let run = |r: &mut Running, ms: &mut u64| {
            *ms += 20;
            match r.pacer.poll(t0 + std::time::Duration::from_millis(*ms)) {
                Tick::Run { per_draw, draws } => present(r, per_draw, draws),
                Tick::Held => {}
                t => panic!("{t:?}"),
            }
        };
        let act = |r: &mut Running, a: Transp, ms: u64| {
            transport_act(r, a, t0 + std::time::Duration::from_millis(ms));
        };
        for _ in 0..5 {
            run(&mut r, &mut ms);
        }
        act(&mut r, Transp::Pause, ms);
        let f = r.ctx.w.frame;
        for _ in 0..5 {
            run(&mut r, &mut ms);
        }
        assert_eq!(r.ctx.w.frame, f);
        act(&mut r, Transp::Step, ms);
        run(&mut r, &mut ms);
        assert_eq!(r.ctx.w.frame, f + 1);
        run(&mut r, &mut ms);
        assert_eq!(r.ctx.w.frame, f + 1);
        act(&mut r, Transp::Faster, ms);
        act(&mut r, Transp::Pause, ms);
        run(&mut r, &mut ms);
        assert_eq!(r.ctx.w.frame, f + 3);
    }
}
