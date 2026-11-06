//! Headless context (game directory, in-memory saves, no renderer or audio) and the hashed session / demo runs.

use crate::session::{Recording, format_row};
use th08_core::assets::{BoxFuture, DataSource, Source};
use th08_core::game::{AudioOut, Ctx, Io};
use th08_core::host::{Host, LocalTime, MAX_PLAYERS, RawInput};
use th08_core::task::Flow;
use th08_core::trace::Row;

// Game directory, file names matched without case.
pub struct GameDir(pub std::path::PathBuf);

impl GameDir {
    fn find(&self, name: &str) -> std::path::PathBuf {
        let exact = self.0.join(name);
        if exact.exists() {
            return exact;
        }
        std::fs::read_dir(&self.0)
            .into_iter()
            .flatten()
            .flatten()
            .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(name))
            .map_or(exact, |e| e.path())
    }
}

impl DataSource for GameDir {
    fn names(&self) -> Vec<String> {
        std::fs::read_dir(&self.0)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect()
    }

    fn fetch<'a>(
        &'a self,
        name: &'a str,
        range: Option<(u64, usize)>,
    ) -> BoxFuture<'a, anyhow::Result<Vec<u8>>> {
        Box::pin(async move {
            use std::io::{Read, Seek, SeekFrom};
            let path = self.find(name);
            Ok(match range {
                None => std::fs::read(path)?,
                Some((off, len)) => {
                    let mut f = std::fs::File::open(path)?;
                    f.seek(SeekFrom::Start(off))?;
                    let mut buf = vec![0; len];
                    f.read_exact(&mut buf)?;
                    buf
                }
            })
        })
    }
}

// No sound device and no MIDI synth (`midi_ready` false).
struct NoAudio;

impl AudioOut for NoAudio {
    fn load(&mut self, _: th08_core::content::PresentView<'_>) {}
    fn midi_available(&mut self) -> bool {
        false
    }
    fn exec(&mut self, _: th08_core::sound::SoundCmd) {}
    fn is_started(&self) -> bool {
        true
    }
    fn ensure_started(&mut self) {}
}

// Saves in memory, fixed clocks; each poll answers the recorded value of poll index `cursor`, then advances it.
#[derive(Default)]
pub struct ReplayHost {
    pub files: std::collections::BTreeMap<String, Vec<u8>>,
    pub polls: Vec<u16>,
    pub cursor: i64,
    pub time: LocalTime,
}

impl Host for ReplayHost {
    fn poll_input(&mut self) -> RawInput {
        let mut keys = [0; MAX_PLAYERS];
        keys[0] = usize::try_from(self.cursor)
            .ok()
            .and_then(|i| self.polls.get(i).copied())
            .unwrap_or(0);
        self.cursor += 1;
        RawInput {
            keys,
            ..Default::default()
        }
    }
    fn boot_clock_ms(&mut self) -> u32 {
        0
    }
    fn ticks_ms(&mut self) -> u64 {
        0
    }
    fn local_time(&mut self) -> LocalTime {
        self.time
    }
    fn read(&mut self, name: &str) -> Option<Vec<u8>> {
        self.files.get(name).cloned()
    }
    fn write(&mut self, name: &str, data: &[u8]) {
        self.files.insert(name.to_string(), data.to_vec());
    }
    fn mkdir(&mut self, _: &str) {}
    fn list(&mut self, prefix: &str) -> Vec<String> {
        self.files
            .keys()
            .filter(|n| n.starts_with(prefix))
            .cloned()
            .collect()
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

// Polls a future whose reads complete at once (plain file reads), without an executor.
fn block_on<F: std::future::Future>(f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    loop {
        if let std::task::Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
    }
}

// A context over the game directory with default core settings and no saves of its own.
pub fn load(game: std::path::PathBuf) -> anyhow::Result<Ctx> {
    let io = Io {
        renderer: Box::new(th08_core::drawlist::NullRenderer),
        audio: Box::new(NoAudio),
        host: Box::<ReplayHost>::default(),
        drawlog: None,
        overlay: None,
    };
    block_on(Ctx::load_with(
        Source::new(GameDir(game)),
        io,
        th08_core::coreconfig::CoreConfig::default(),
        false,
    ))
}

const FNV_BASIS: u64 = 0xCBF2_9CE4_8422_2325;

fn fnv(mut h: u64, bytes: &[u8]) -> u64 {
    for &b in bytes {
        h = (h ^ u64::from(b)).wrapping_mul(0x100_0000_01B3);
    }
    h
}

// FNV-1a of a trace row as the session tool writes it (one trace.txt line).
pub fn row_hash(r: &Row) -> u64 {
    fnv(FNV_BASIS, format_row(r).as_bytes())
}

// Receives one hash line per trace row and the final digest.
pub struct Stream<W: std::io::Write> {
    pub out: W,
    pub rows: u64,
    pub digest: u64,
    // Also the row text itself (the session tool's trace.txt lines), for checks against it.
    pub text: Option<Box<dyn std::io::Write>>,
}

impl<W: std::io::Write> Stream<W> {
    pub fn new(out: W) -> Self {
        Self {
            out,
            rows: 0,
            digest: FNV_BASIS,
            text: None,
        }
    }

    pub fn row(&mut self, r: &Row) -> std::io::Result<()> {
        let line = format_row(r);
        let h = fnv(FNV_BASIS, line.as_bytes());
        self.digest = fnv(self.digest, &h.to_le_bytes());
        writeln!(self.out, "{} {h:016x}", self.rows)?;
        if let Some(t) = &mut self.text {
            writeln!(t, "{line}")?;
        }
        self.rows += 1;
        Ok(())
    }

    pub fn finish(mut self) -> std::io::Result<(u64, u64)> {
        writeln!(self.out, "digest {:016x} rows {}", self.digest, self.rows)?;
        self.out.flush()?;
        if let Some(t) = &mut self.text {
            t.flush()?;
        }
        Ok((self.digest, self.rows))
    }
}

// Plays a recorded session to its end; every trace row (boot rows included) goes to `sink`.
pub fn session(
    ctx: &mut Ctx,
    rec: &Recording,
    mut sink: impl FnMut(&Row) -> std::io::Result<()>,
) -> anyhow::Result<()> {
    let files = rec.start(ctx)?;
    let host: &mut ReplayHost = ctx.host_as().expect("ReplayHost");
    host.files.extend(files);
    host.polls.clone_from(&rec.input);
    if let Some(t) = rec.recorded_at {
        host.time = t;
    }
    let mut err = None;
    rec.run(
        ctx,
        usize::MAX,
        |ctx, poll| {
            ctx.host_as::<ReplayHost>().expect("ReplayHost").cursor = poll;
            Some(ctx.run_frame())
        },
        |r, _| match sink(r) {
            Ok(()) => true,
            Err(e) => {
                err = Some(e);
                false
            }
        },
    );
    err.map_or(Ok(()), |e| Err(e.into()))
}

// Plays the archive's demorpyN.rpy as the demo sync check does (calc only); one demo-trace row per demo frame, in the
// session row layout (columns 0–21, the rest 0).
pub fn demo(
    ctx: &mut Ctx,
    n: u8,
    mut sink: impl FnMut(&Row) -> std::io::Result<()>,
) -> anyhow::Result<()> {
    let r = th08_core::replay::parse(
        &ctx.archive.read(&format!("demorpy{n}.rpy"))?,
        ctx.w.game.ver_list.get(),
    )?;
    ctx.w.game.replay_ff = false;
    anyhow::ensure!(
        th08_core::replay::start_playback(&mut ctx.w.game, r, None, Some(n)),
        "demo has no stage"
    );
    let rng_base = ctx.w.rng.total;
    for _ in 0..1_000_000 {
        let before = ctx.w.game.demo_frame;
        if ctx.calc() != Flow::Continue {
            break;
        }
        if ctx.w.game.demo_frame > before {
            let mut row = [0i64; 36];
            row[..22].copy_from_slice(&th08_core::trace::demo_sample(ctx, rng_base));
            sink(&row)?;
        }
        if ctx.w.game.flags & th08_core::gamemgr::F_DEMO == 0 {
            break;
        }
    }
    Ok(())
}
