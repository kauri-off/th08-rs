//! Discord Rich Presence: a read-only observer of the game state and a worker thread that talks to the local Discord app.

use std::sync::mpsc;
use std::time::Duration;
use th08_core::gamemgr::{Game, GameFlags};
use th08_core::types::SceneId;

pub const DISCORD_APP_ID: &str = "1568337388198035587";
// Discord accepts one activity update per 15 s.
pub const MIN_GAP: Duration = Duration::from_secs(15);
// Delay between connection attempts while Discord is not running.
const RETRY: Duration = Duration::from_secs(15);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Story,
    StagePractice,
    SpellPractice,
    Replay,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Menu,
    MusicRoom,
    Ending,
    Run {
        mode: Mode,
        shot: u8,
        difficulty: i32,
        stage: usize,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Presence {
    pub details: String,
    pub state: Option<String>,
    // Unix seconds when the run started (Discord shows the elapsed time).
    pub start: Option<i64>,
}

pub fn status(g: &Game) -> Status {
    match g.scene {
        SceneId::Game
        | SceneId::NextStage
        | SceneId::Retry
        | SceneId::Reload
        | SceneId::Advance
            if !g.has(GameFlags::DEMO) =>
        {
            let mode = if g.replay() {
                Mode::Replay
            } else if g.spell_practice() {
                Mode::SpellPractice
            } else if g.practice() {
                Mode::StagePractice
            } else {
                Mode::Story
            };
            Status::Run {
                mode,
                shot: g.shot,
                difficulty: g.difficulty,
                stage: g.stage,
            }
        }
        SceneId::MusicRoom => Status::MusicRoom,
        SceneId::Ending => Status::Ending,
        _ => Status::Menu,
    }
}

fn team(shot: u8) -> &'static str {
    const NAMES: [&str; 12] = [
        "Reimu & Yukari",
        "Marisa & Alice",
        "Sakuya & Remilia",
        "Youmu & Yuyuko",
        "Reimu",
        "Yukari",
        "Marisa",
        "Alice",
        "Sakuya",
        "Remilia",
        "Youmu",
        "Yuyuko",
    ];
    NAMES.get(usize::from(shot)).copied().unwrap_or("Unknown")
}

fn difficulty(d: i32) -> &'static str {
    ["Easy", "Normal", "Hard", "Lunatic", "Extra"][usize::try_from(d.clamp(0, 4)).unwrap_or(4)]
}

fn stage(s: usize) -> &'static str {
    const NAMES: [&str; 9] = [
        "Stage 1",
        "Stage 2",
        "Stage 3",
        "Stage 4A",
        "Stage 4B",
        "Stage 5",
        "Stage 6A",
        "Stage 6B",
        "Extra Stage",
    ];
    NAMES.get(s).copied().unwrap_or("Stage ?")
}

pub fn presence(s: Status, start: Option<i64>) -> Presence {
    let only = |d: &str| Presence {
        details: d.to_owned(),
        state: None,
        start: None,
    };
    match s {
        Status::Menu => only("In the menu"),
        Status::MusicRoom => only("In the music room"),
        Status::Ending => only("Watching the ending"),
        Status::Run {
            mode,
            shot,
            difficulty: d,
            stage: st,
        } => {
            let line = format!("{} · {} · {}", team(shot), difficulty(d), stage(st));
            let (details, state) = match mode {
                Mode::Replay => ("Watching a replay".to_owned(), line),
                Mode::Story => (line, "Story mode".to_owned()),
                Mode::StagePractice => (line, "Stage practice".to_owned()),
                Mode::SpellPractice => (line, "Spell practice".to_owned()),
            };
            Presence {
                details,
                state: Some(state),
                start,
            }
        }
    }
}

// Frame side: the presence for the game state, Some only when it changed.
#[derive(Default)]
pub struct Observer {
    last: Option<Status>,
    run_start: Option<i64>,
}

impl Observer {
    pub fn observe(&mut self, g: &Game, unix_now: i64) -> Option<Presence> {
        let s = status(g);
        if self.last == Some(s) {
            return None;
        }
        let same_run = matches!(
            (self.last, s),
            (Some(Status::Run { mode: a, .. }), Status::Run { mode: b, .. }) if a == b
        );
        if !matches!(s, Status::Run { .. }) {
            self.run_start = None;
        } else if !same_run {
            self.run_start = Some(unix_now);
        }
        self.last = Some(s);
        Some(presence(s, self.run_start))
    }
}

// Worker side: the latest wanted presence, sent on change and at most once per `MIN_GAP`.
#[derive(Default)]
pub struct Limiter {
    want: Option<Presence>,
    sent: Option<Presence>,
    last: Option<Duration>,
}

impl Limiter {
    pub fn offer(&mut self, p: Presence) {
        self.want = Some(p);
    }

    // Time until the pending presence may go out; None when nothing is pending.
    pub fn due(&self, now: Duration) -> Option<Duration> {
        (self.want != self.sent).then(|| {
            self.last
                .map_or(Duration::ZERO, |l| (l + MIN_GAP).saturating_sub(now))
        })
    }

    pub fn take(&mut self, now: Duration) -> Option<Presence> {
        if self.due(now)? > Duration::ZERO {
            return None;
        }
        self.sent.clone_from(&self.want);
        self.last = Some(now);
        self.sent.clone()
    }

    // A new connection has nothing shown yet.
    pub fn reset(&mut self) {
        self.sent = None;
        self.last = None;
    }
}

// The connection to Discord; every call may fail, the worker retries.
pub trait Sink {
    fn connect(&mut self) -> bool;
    fn set(&mut self, p: &Presence) -> bool;
    fn close(&mut self);
}

// The worker loop: ends when the sender is dropped.
pub fn run(sink: &mut dyn Sink, rx: &mpsc::Receiver<Presence>, now: &dyn Fn() -> Duration) {
    let mut lim = Limiter::default();
    let mut connected = false;
    let mut tried: Option<Duration> = None;
    loop {
        let t = now();
        let mut wait = lim.due(t).unwrap_or(RETRY);
        if !connected {
            wait = wait.max(tried.map_or(Duration::ZERO, |l| (l + RETRY).saturating_sub(t)));
        }
        match rx.recv_timeout(wait) {
            Ok(p) => lim.offer(p),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        while let Ok(p) = rx.try_recv() {
            lim.offer(p);
        }
        let t = now();
        if !connected && tried.is_none_or(|l| t >= l + RETRY) {
            tried = Some(t);
            connected = sink.connect();
            if connected {
                log::info!("discord: connected");
                lim.reset();
            }
        }
        if connected
            && let Some(p) = lim.take(t)
            && !sink.set(&p)
        {
            log::info!("discord: connection lost");
            connected = false;
            tried = Some(t);
            lim.reset();
        }
    }
    if connected {
        sink.close();
    }
}

#[cfg(all(feature = "discord", any(windows, target_os = "linux")))]
mod ipc {
    use super::{DISCORD_APP_ID, Presence, Sink};
    use discord_rich_presence::{DiscordIpc, DiscordIpcClient, activity};

    pub struct Ipc(Option<DiscordIpcClient>);

    impl Ipc {
        pub const fn new() -> Self {
            Self(None)
        }
    }

    impl Sink for Ipc {
        fn connect(&mut self) -> bool {
            let mut c = DiscordIpcClient::new(DISCORD_APP_ID);
            match c.connect() {
                Ok(()) => {
                    self.0 = Some(c);
                    true
                }
                Err(e) => {
                    log::debug!("discord: not reachable: {e}");
                    false
                }
            }
        }

        fn set(&mut self, p: &Presence) -> bool {
            let Some(c) = self.0.as_mut() else {
                return false;
            };
            let mut a = activity::Activity::new().details(p.details.as_str());
            if let Some(s) = &p.state {
                a = a.state(s.as_str());
            }
            if let Some(t) = p.start {
                a = a.timestamps(activity::Timestamps::new().start(t));
            }
            let ok = c.set_activity(a).is_ok();
            if !ok {
                let _ = c.close();
                self.0 = None;
            }
            ok
        }

        fn close(&mut self) {
            if let Some(mut c) = self.0.take() {
                let _ = c.clear_activity();
                let _ = c.close();
            }
        }
    }
}

// The app's handle: observes after each frame's logic and hands changes to the worker thread; inert when off.
pub struct Rpc {
    obs: Observer,
    tx: Option<mpsc::Sender<Presence>>,
}

impl Rpc {
    pub fn off() -> Self {
        Self {
            obs: Observer::default(),
            tx: None,
        }
    }

    pub fn new(on: bool) -> Self {
        let mut r = Self::off();
        if on {
            r.tx = spawn();
        }
        r
    }

    pub fn observe(&mut self, g: &Game) {
        let Some(tx) = &self.tx else {
            return;
        };
        let unix = web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs().cast_signed());
        if let Some(p) = self.obs.observe(g, unix)
            && tx.send(p).is_err()
        {
            self.tx = None;
        }
    }
}

#[cfg(all(feature = "discord", any(windows, target_os = "linux")))]
fn spawn() -> Option<mpsc::Sender<Presence>> {
    let (tx, rx) = mpsc::channel();
    let t0 = std::time::Instant::now();
    std::thread::Builder::new()
        .name("discord".into())
        .spawn(move || run(&mut ipc::Ipc::new(), &rx, &|| t0.elapsed()))
        .map_err(|e| log::warn!("discord: no thread: {e}"))
        .ok()?;
    Some(tx)
}

#[cfg(not(all(feature = "discord", any(windows, target_os = "linux"))))]
fn spawn() -> Option<mpsc::Sender<Presence>> {
    log::info!("discord: not available in this build");
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn run_status(mode: Mode) -> Status {
        Status::Run {
            mode,
            shot: 0,
            difficulty: 3,
            stage: 6,
        }
    }

    #[test]
    fn presence_text() {
        let p = presence(run_status(Mode::Story), Some(100));
        assert_eq!(p.details, "Reimu & Yukari · Lunatic · Stage 6A");
        assert_eq!(p.state.as_deref(), Some("Story mode"));
        assert_eq!(p.start, Some(100));
        let p = presence(run_status(Mode::Replay), Some(5));
        assert_eq!(p.details, "Watching a replay");
        assert_eq!(
            p.state.as_deref(),
            Some("Reimu & Yukari · Lunatic · Stage 6A")
        );
        let p = presence(
            Status::Run {
                mode: Mode::SpellPractice,
                shot: 9,
                difficulty: 4,
                stage: 8,
            },
            None,
        );
        assert_eq!(p.details, "Remilia · Extra · Extra Stage");
        assert_eq!(p.state.as_deref(), Some("Spell practice"));
        let p = presence(Status::Menu, Some(1));
        assert_eq!(
            (p.details.as_str(), p.state, p.start),
            ("In the menu", None, None)
        );
        assert_eq!(
            presence(Status::MusicRoom, None).details,
            "In the music room"
        );
        assert_eq!(
            presence(Status::Ending, None).details,
            "Watching the ending"
        );
    }

    fn p(d: &str) -> Presence {
        Presence {
            details: d.into(),
            state: None,
            start: None,
        }
    }

    #[test]
    fn limiter_gap_and_change() {
        let s = Duration::from_secs;
        let mut l = Limiter::default();
        assert_eq!(l.take(s(0)), None);
        l.offer(p("a"));
        assert_eq!(l.take(s(0)), Some(p("a")));
        l.offer(p("a"));
        assert_eq!(l.due(s(1)), None);
        l.offer(p("b"));
        l.offer(p("c"));
        assert_eq!(l.due(s(5)), Some(s(10)));
        assert_eq!(l.take(s(14)), None);
        assert_eq!(l.take(s(15)), Some(p("c")));
        l.offer(p("d"));
        l.offer(p("c"));
        assert_eq!(l.take(s(40)), None);
        l.reset();
        assert_eq!(l.take(s(41)), Some(p("c")));
    }

    #[derive(Clone, Default)]
    struct Fake {
        log: Arc<Mutex<Vec<String>>>,
        up: bool,
    }

    impl Sink for Fake {
        fn connect(&mut self) -> bool {
            self.log.lock().unwrap().push("connect".into());
            self.up
        }
        fn set(&mut self, p: &Presence) -> bool {
            self.log.lock().unwrap().push(p.details.clone());
            true
        }
        fn close(&mut self) {
            self.log.lock().unwrap().push("close".into());
        }
    }

    #[test]
    fn worker_absent_discord_then_close() {
        let (tx, rx) = mpsc::channel();
        let mut down = Fake::default();
        tx.send(p("x")).unwrap();
        drop(tx);
        run(&mut down, &rx, &|| Duration::ZERO);
        assert_eq!(*down.log.lock().unwrap(), ["connect"]);
        let (tx, rx) = mpsc::channel();
        let mut up = Fake {
            up: true,
            ..Fake::default()
        };
        let log = up.log.clone();
        let h = std::thread::spawn(move || run(&mut up, &rx, &|| Duration::ZERO));
        tx.send(p("x")).unwrap();
        while log.lock().unwrap().len() < 2 {
            std::thread::yield_now();
        }
        drop(tx);
        h.join().unwrap();
        assert_eq!(*log.lock().unwrap(), ["connect", "x", "close"]);
    }

    // A replay (TH08_DISCORD_RPY, else demo 0's record) played to its end: the observed changes and what a
    // fake sink gets through the limiter at 60 fps of fake time.
    #[test]
    fn replay_presence_sequence() {
        use th08_core::game::Ctx;
        let Some(dir) = crate::settings::test_game_dir() else {
            eprintln!("skipped: no game data (TH08_DIR unset)");
            return;
        };
        let src = crate::sources::native(dir);
        let base = crate::settings::get().clone();
        let load = |s: &crate::settings::Settings| {
            let io = s.tool_io(th08_core::drawlist::NullRenderer, &src);
            pollster::block_on(Ctx::load_with(src.clone(), io, s.core(), false)).unwrap()
        };
        let tmp = std::env::temp_dir().join(format!("th08_discord_{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let rpy = std::env::var_os("TH08_DISCORD_RPY").map_or_else(
            || {
                let path = tmp.join("demo.rpy");
                std::fs::write(&path, load(&base).archive.read("demorpy0.rpy").unwrap()).unwrap();
                path
            },
            std::path::PathBuf::from,
        );
        let mut s = base;
        s.replay = Some(rpy.to_string_lossy().into_owned());
        s.save_dir = Some(tmp.clone());
        let mut ctx = load(&s);
        let mut obs = Observer::default();
        let mut lim = Limiter::default();
        let mut seen = Vec::new();
        let mut sent = Vec::new();
        let mut was_replay = false;
        let mut t = Duration::ZERO;
        for f in 0..2_000_000u64 {
            t = Duration::from_millis(f * 1000 / 60);
            if let Some(pr) = obs.observe(&ctx.w.game, 1_000_000 + (f / 60).cast_signed()) {
                eprintln!(
                    "frame {f}: {} | {:?} | {:?}",
                    pr.details, pr.state, pr.start
                );
                seen.push(pr.clone());
                lim.offer(pr);
            }
            if let Some(pr) = lim.take(t) {
                sent.push(pr);
            }
            let replay = matches!(
                status(&ctx.w.game),
                Status::Run {
                    mode: Mode::Replay,
                    ..
                }
            );
            if was_replay && !replay {
                break;
            }
            was_replay |= replay;
            if !matches!(ctx.calc(), th08_core::task::Flow::Continue) {
                break;
            }
        }
        sent.extend(lim.take(t + MIN_GAP));
        let _ = std::fs::remove_dir_all(&tmp);
        eprintln!("sent through the limiter:");
        for pr in &sent {
            eprintln!("  {} | {:?}", pr.details, pr.state);
        }
        assert!(was_replay, "the replay never started");
        let runs: Vec<_> = seen
            .iter()
            .filter(|p| p.details == "Watching a replay")
            .collect();
        assert!(!runs.is_empty());
        assert!(
            runs.iter().all(|p| p.start == runs[0].start),
            "one run, one start time"
        );
        assert_ne!(
            seen.last().map(|p| p.details.as_str()),
            Some("Watching a replay")
        );
        assert_eq!(sent.first(), seen.first());
        assert_eq!(sent.last(), seen.last());
    }
}
