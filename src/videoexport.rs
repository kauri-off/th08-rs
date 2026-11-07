//! Replay to video: plays a .rpy or attract demo headless on a virtual clock, mixes its sound offline and encodes both through ffmpeg (or writes PNG + WAV).

use crate::platform::hitbox::{self, Hitboxes};
use crate::platform::overlay::{Corner, InputView, TransportOverlay};
use crate::settings::Settings;
use anyhow::{Context, Result, bail};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use th08_core::game::Ctx;
use th08_core::types::SceneId;
use th08_render::gfx::Gfx;

pub const FPS: u32 = 60;
// Boot frames allowed before the playback must have started.
const BOOT_LIMIT: u64 = 2000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    File(PathBuf),
    // Attract demo `demorpyN.rpy` of the archive.
    Demo(u8),
}

impl Source {
    pub fn parse(v: &str) -> Result<Self> {
        match v.strip_prefix("demo:") {
            Some(n) => Ok(Self::Demo(n.parse().context("demo:N needs a number")?)),
            None => Ok(Self::File(v.into())),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Output {
    // .mp4 (H.264 / AAC) or .mkv through ffmpeg.
    Video(PathBuf),
    // DIR/NNNNNN.png per frame plus DIR/audio.wav.
    PngWav(PathBuf),
}

#[derive(Clone, Debug)]
pub struct Options {
    pub source: Source,
    pub output: Output,
    // Render scale k: frames are 640k × 480k.
    pub scale: u32,
    // Replay stage record (0–8) played alone; None = the whole replay.
    pub stage: Option<usize>,
    // Video frames `from..=to`, counted from the first frame of the playback.
    pub from: u64,
    pub to: Option<u64>,
    pub hitboxes: Hitboxes,
    pub inputs: Option<Corner>,
    pub rate: u32,
    pub crf: u32,
    pub preset: String,
    pub ffmpeg: PathBuf,
}

impl Options {
    pub fn new(source: Source, output: Output) -> Self {
        Self {
            source,
            output,
            scale: 1,
            stage: None,
            from: 0,
            to: None,
            hitboxes: Hitboxes::Off,
            inputs: None,
            rate: 44100,
            crf: 18,
            preset: "fast".into(),
            ffmpeg: "ffmpeg".into(),
        }
    }

    fn overlays(&self) -> bool {
        self.hitboxes != Hitboxes::Off || self.inputs.is_some()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Progress {
    // Video frames written.
    pub frames: u64,
    // Estimate from the replay's input records (0 = unknown).
    pub total: u64,
    pub stage: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    pub frames: u64,
    // Stereo sample frames written.
    pub samples: u64,
    pub rate: u32,
    pub size: (u32, u32),
    // Logic frames run in all (boot and skipped ones included).
    pub run_frames: u64,
}

impl Report {
    // Audio length minus video length.
    pub fn drift_secs(&self) -> f64 {
        self.samples as f64 / f64::from(self.rate) - self.frames as f64 / f64::from(FPS)
    }
}

#[derive(Debug)]
pub struct Cancelled;

impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("cancelled")
    }
}

impl std::error::Error for Cancelled {}

#[derive(Debug)]
pub struct NoFfmpeg(pub PathBuf);

impl std::fmt::Display for NoFfmpeg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ffmpeg not found ({}): install ffmpeg or write a PNG sequence + WAV instead (--png-wav DIR)",
            self.0.display()
        )
    }
}

impl std::error::Error for NoFfmpeg {}

// `ffmpeg -version` runs.
pub fn ffmpeg_available(ffmpeg: &Path) -> bool {
    Command::new(ffmpeg)
        .arg("-version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn gfx(ctx: &mut Ctx) -> &mut Gfx {
    ctx.renderer
        .as_any_mut()
        .downcast_mut::<Gfx>()
        .expect("renderer is Gfx")
}

// Plays `o.source` and writes every video frame with its 1/60 s of sound; `cancel` stops it between frames.
pub fn export(
    s: &Settings,
    o: &Options,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> Result<Report> {
    anyhow::ensure!((1..=3).contains(&o.scale), "scale must be 1, 2 or 3");
    anyhow::ensure!(
        (8000..=192_000).contains(&o.rate),
        "audio rate 8000–192000 Hz"
    );
    let dir = s.game_dir.clone().context("no game folder set")?;
    let size = (640 * o.scale, 480 * o.scale);
    let mut sink = Sink::open(o, size)?;
    let res = run(s, o, &dir, size, &mut sink, cancel, &mut progress);
    match res {
        Ok(r) => {
            sink.finish(o)?;
            Ok(r)
        }
        Err(e) => {
            sink.abort();
            Err(e)
        }
    }
}

fn run(
    s: &Settings,
    o: &Options,
    dir: &Path,
    size: (u32, u32),
    sink: &mut Sink,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(Progress),
) -> Result<Report> {
    let src = crate::sources::native(dir.to_path_buf());
    let mut g = pollster::block_on(Gfx::headless())?;
    g.offscreen = true;
    g.render_scale = th08_render::gfx::RenderScale::Fixed(o.scale);
    g.blit_options = th08_render::gfx::BlitOptions {
        filter: th08_render::gfx::ScaleFilter::Nearest,
        ..Default::default()
    };
    if o.hitboxes != Hitboxes::Off {
        hitbox::install(&mut g, o.hitboxes);
    }
    if o.inputs.is_some() {
        let lang = crate::i18n::resolve(s.language);
        g.hud = Some(Box::new(TransportOverlay::new(lang, s.font.clone(), false)));
    }
    let mut renderer: Box<dyn th08_core::drawlist::Renderer> = Box::new(g);
    let overlay = s.mount_pack(renderer.as_mut());
    let (audio, mut mix) = th08_audio::audio::Audio::offline(src.clone(), s.audio(), o.rate);
    // No save dir: the game's own defaults (th08.cfg, score.dat) and nothing written.
    let mut host = crate::platform::ToolHost::new(crate::platform::Storage::new(None));
    host.clock.set_virtual(true);
    let io = th08_core::game::Io {
        renderer,
        audio: Box::new(audio),
        host: Box::new(host),
        drawlog: None,
        overlay,
    };
    let core = th08_core::coreconfig::CoreConfig {
        font: s.font.clone(),
        ..Default::default()
    };
    let mut ctx = pollster::block_on(Ctx::load_with(src, io, core, false))?;
    let (bytes, demo) = match &o.source {
        Source::File(p) => (read_replay(p)?, None),
        Source::Demo(n) => (ctx.archive.read(&format!("demorpy{n}.rpy"))?, Some(*n)),
    };
    let r = th08_core::replay::parse(&bytes, ctx.w.game.ver_list.get())?;
    if let Some(st) = o.stage {
        anyhow::ensure!(
            r.stages.get(st).is_some_and(Option::is_some),
            "the replay has no stage record {} (it has {})",
            st + 1,
            stage_list(&r)
        );
    }
    let total = estimate(&r, o);
    anyhow::ensure!(
        th08_core::replay::start_playback(&mut ctx.w.game, r, o.stage, demo),
        "the replay has no stage"
    );
    let (mut run_frames, mut video, mut written, mut samples) = (0u64, None::<u64>, 0u64, 0u64);
    let mut pcm = Vec::new();
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(Cancelled.into());
        }
        let flow = ctx.run_frame();
        crate::platform::tool_host(&mut ctx).clock.frame();
        run_frames += 1;
        pcm.clear();
        mix.frame(&mut pcm);
        let g = &ctx.w.game;
        let playing =
            g.scene == SceneId::Game && g.replay() && o.stage.is_none_or(|s| g.stage == s);
        let i = match video {
            None if playing => 0,
            None => {
                anyhow::ensure!(run_frames < BOOT_LIMIT, "the replay did not start");
                continue;
            }
            Some(_) if !playing || flow != th08_core::task::Flow::Continue => break,
            Some(n) => n + 1,
        };
        video = Some(i);
        if o.to.is_some_and(|t| i > t) {
            break;
        }
        if i < o.from {
            continue;
        }
        let img = if o.overlays() {
            hitbox::update(&mut ctx);
            let word = {
                let inp = &ctx.w.input;
                if ctx.w.game.scene == SceneId::Game {
                    inp.game[0]
                } else {
                    inp.held
                }
            };
            let g = gfx(&mut ctx);
            if let Some(h) = g
                .hud
                .as_mut()
                .and_then(|h| h.as_any_mut().downcast_mut::<TransportOverlay>())
            {
                h.safe = [0.0, 0.0, size.0 as f32, size.1 as f32];
                h.view.inputs = o.inputs.map(|corner| InputView {
                    word,
                    corner,
                    opacity: s.input_opacity,
                });
            }
            g.read_frame_overlaid()
        } else {
            gfx(&mut ctx).read_frame()
        };
        anyhow::ensure!(
            img.dimensions() == size,
            "frame is {:?}, expected {size:?}",
            img.dimensions()
        );
        sink.frame(i, &img, &pcm)?;
        written += 1;
        samples += (pcm.len() / 2) as u64;
        progress(Progress {
            frames: written,
            total,
            stage: ctx.w.game.stage,
        });
    }
    anyhow::ensure!(written > 0, "no frame in the chosen range");
    Ok(Report {
        frames: written,
        samples,
        rate: mix.rate(),
        size,
        run_frames,
    })
}

fn read_replay(p: &Path) -> Result<Vec<u8>> {
    let len = std::fs::metadata(p)
        .with_context(|| p.display().to_string())?
        .len();
    anyhow::ensure!(
        len <= crate::saves::MAX_REPLAY,
        "{}: too large",
        p.display()
    );
    std::fs::read(p).with_context(|| p.display().to_string())
}

// The stage records present, 1-based ("1 2 3 4 5 6").
pub fn stage_list(r: &th08_core::replay::Replay) -> String {
    let v: Vec<String> = (0..r.stages.len())
        .filter(|&i| r.stages[i].is_some())
        .map(|i| (i + 1).to_string())
        .collect();
    v.join(" ")
}

// Frames the export will write, from the input records (each holds one word per frame), clipped to the range.
fn estimate(r: &th08_core::replay::Replay, o: &Options) -> u64 {
    let n: usize = match o.stage {
        Some(s) => r.stages[s].as_ref().map_or(0, |x| x.inputs.len()),
        None => r.stages.iter().flatten().map(|x| x.inputs.len()).sum(),
    };
    let n = n as u64;
    o.to.map_or(n, |t| n.min(t + 1)).saturating_sub(o.from)
}

#[expect(clippy::cast_possible_truncation, reason = "clamped to the i16 range")]
fn to_i16(x: f32) -> i16 {
    (x * 32767.0).round().clamp(-32768.0, 32767.0) as i16
}

fn wav_writer(
    path: &Path,
    rate: u32,
) -> Result<hound::WavWriter<std::io::BufWriter<std::fs::File>>> {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    hound::WavWriter::create(path, spec).with_context(|| path.display().to_string())
}

type Wav = hound::WavWriter<std::io::BufWriter<std::fs::File>>;

// Where the frames and samples go.
enum Sink {
    // ffmpeg encoding the raw RGB frames from its stdin into a temporary video-only file; the sound goes to a temporary
    // WAV; `finish` muxes both into the output.
    Ffmpeg {
        child: Child,
        stdin: Option<ChildStdin>,
        log: std::thread::JoinHandle<String>,
        video: PathBuf,
        wav: Option<Wav>,
        wav_path: PathBuf,
        out: PathBuf,
        rgb: Vec<u8>,
    },
    PngWav {
        dir: PathBuf,
        wav: Option<Wav>,
    },
}

fn temp_beside(out: &Path, suffix: &str) -> PathBuf {
    let mut name = std::ffi::OsString::from(".");
    name.push(out.file_name().unwrap_or_default());
    name.push(suffix);
    out.with_file_name(name)
}

fn stderr_reader(child: &mut Child) -> std::thread::JoinHandle<String> {
    let err = child.stderr.take();
    std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(mut e) = err {
            let _ = std::io::Read::read_to_string(&mut e, &mut s);
        }
        s
    })
}

fn last_lines(s: &str) -> String {
    let v: Vec<&str> = s.lines().rev().take(6).collect();
    v.into_iter().rev().collect::<Vec<_>>().join("\n")
}

impl Sink {
    fn open(o: &Options, (w, h): (u32, u32)) -> Result<Self> {
        match &o.output {
            Output::PngWav(dir) => {
                std::fs::create_dir_all(dir).with_context(|| dir.display().to_string())?;
                Ok(Self::PngWav {
                    dir: dir.clone(),
                    wav: Some(wav_writer(&dir.join("audio.wav"), o.rate)?),
                })
            }
            Output::Video(out) => {
                let ext = out
                    .extension()
                    .map(|e| e.to_string_lossy().to_ascii_lowercase());
                anyhow::ensure!(
                    matches!(ext.as_deref(), Some("mp4" | "mkv")),
                    "{}: the video file must end in .mp4 or .mkv",
                    out.display()
                );
                if !ffmpeg_available(&o.ffmpeg) {
                    return Err(NoFfmpeg(o.ffmpeg.clone()).into());
                }
                if let Some(d) = out.parent().filter(|d| !d.as_os_str().is_empty()) {
                    std::fs::create_dir_all(d).with_context(|| d.display().to_string())?;
                }
                let video = temp_beside(out, ".video.mkv");
                let wav_path = temp_beside(out, ".audio.wav");
                let mut child = Command::new(&o.ffmpeg)
                    .args(["-hide_banner", "-loglevel", "error", "-y"])
                    .args(["-f", "rawvideo", "-pix_fmt", "rgb24"])
                    .args(["-s", &format!("{w}x{h}"), "-r", &FPS.to_string(), "-i", "-"])
                    .args(["-c:v", "libx264", "-preset", &o.preset])
                    .args(["-crf", &o.crf.to_string(), "-pix_fmt", "yuv420p"])
                    // BT.709 limited range, tagged, so players do not guess the matrix by frame height.
                    .args(["-vf", "scale=out_color_matrix=bt709:out_range=tv"])
                    .args(["-colorspace", "bt709", "-color_primaries", "bt709"])
                    .args(["-color_trc", "bt709", "-color_range", "tv"])
                    .arg(&video)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::null())
                    .stderr(Stdio::piped())
                    .spawn()
                    .map_err(|_| NoFfmpeg(o.ffmpeg.clone()))?;
                let log = stderr_reader(&mut child);
                let stdin = child.stdin.take();
                Ok(Self::Ffmpeg {
                    child,
                    stdin,
                    log,
                    video,
                    wav: Some(wav_writer(&wav_path, o.rate)?),
                    wav_path,
                    out: out.clone(),
                    rgb: Vec::with_capacity((w * h * 3) as usize),
                })
            }
        }
    }

    fn frame(&mut self, i: u64, img: &image::RgbaImage, pcm: &[f32]) -> Result<()> {
        let (wav, rgb_out) = match self {
            Self::Ffmpeg {
                stdin, wav, rgb, ..
            } => {
                rgb.clear();
                rgb.extend(
                    img.as_raw()
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .flat_map(|p| [p[0], p[1], p[2]]),
                );
                (wav, Some((stdin, &*rgb)))
            }
            Self::PngWav { dir, wav } => {
                let path = dir.join(format!("{i:06}.png"));
                image::DynamicImage::ImageRgba8(img.clone())
                    .to_rgb8()
                    .save(&path)
                    .with_context(|| path.display().to_string())?;
                (wav, None)
            }
        };
        if let Some((stdin, rgb)) = rgb_out {
            stdin
                .as_mut()
                .context("ffmpeg input closed")?
                .write_all(rgb)
                .context("ffmpeg stopped taking frames")?;
        }
        let w = wav.as_mut().context("wav closed")?;
        for &x in pcm {
            w.write_sample(to_i16(x))?;
        }
        Ok(())
    }

    fn finish(mut self, o: &Options) -> Result<()> {
        match &mut self {
            Self::PngWav { wav, .. } => {
                if let Some(w) = wav.take() {
                    w.finalize()?;
                }
                Ok(())
            }
            Self::Ffmpeg {
                child,
                stdin,
                log,
                video,
                wav,
                wav_path,
                out,
                ..
            } => {
                drop(stdin.take());
                let st = child.wait()?;
                let log = std::mem::replace(log, std::thread::spawn(String::new))
                    .join()
                    .unwrap_or_default();
                if let Some(w) = wav.take() {
                    w.finalize()?;
                }
                let res = if st.success() {
                    mux(o, video, wav_path, out)
                } else {
                    Err(anyhow::anyhow!(
                        "ffmpeg failed ({st}):\n{}",
                        last_lines(&log)
                    ))
                };
                let _ = std::fs::remove_file(&*video);
                let _ = std::fs::remove_file(&*wav_path);
                res
            }
        }
    }

    fn abort(self) {
        match self {
            Self::PngWav { wav, .. } => drop(wav.map(hound::WavWriter::finalize)),
            Self::Ffmpeg {
                mut child,
                stdin,
                video,
                wav,
                wav_path,
                ..
            } => {
                drop(stdin);
                let _ = child.kill();
                let _ = child.wait();
                drop(wav);
                let _ = std::fs::remove_file(video);
                let _ = std::fs::remove_file(wav_path);
            }
        }
    }
}

// Video stream copied, sound encoded to AAC; mp4 gets its index in front.
fn mux(o: &Options, video: &Path, wav: &Path, out: &Path) -> Result<()> {
    let mut c = Command::new(&o.ffmpeg);
    c.args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
        .arg(video)
        .arg("-i")
        .arg(wav)
        .args([
            "-map", "0:v", "-map", "1:a", "-c:v", "copy", "-c:a", "aac", "-b:a", "192k",
        ]);
    if out
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("mp4"))
    {
        c.args(["-movflags", "+faststart"]);
    }
    let r = c
        .arg(out)
        .stdin(Stdio::null())
        .output()
        .map_err(|_| NoFfmpeg(o.ffmpeg.clone()))?;
    if !r.status.success() {
        let _ = std::fs::remove_file(out);
        bail!(
            "ffmpeg mux failed ({}):\n{}",
            r.status,
            last_lines(&String::from_utf8_lossy(&r.stderr))
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sources_and_temps() {
        assert_eq!(Source::parse("demo:2").unwrap(), Source::Demo(2));
        assert!(Source::parse("demo:x").is_err());
        assert_eq!(
            Source::parse("a/b.rpy").unwrap(),
            Source::File("a/b.rpy".into())
        );
        assert_eq!(
            temp_beside(Path::new("/v/out.mp4"), ".audio.wav"),
            PathBuf::from("/v/.out.mp4.audio.wav")
        );
        assert_eq!(to_i16(1.5), 32767);
        assert_eq!(to_i16(-1.0), -32767);
    }

    #[test]
    fn range_estimate() {
        let mut r = th08_core::replay::Replay::default();
        for (s, n) in [(0, 100), (1, 50)] {
            r.stages[s] = Some(th08_core::replay::StageRecord {
                inputs: vec![0; n],
                ..Default::default()
            });
        }
        let mut o = Options::new(Source::Demo(0), Output::PngWav("/x".into()));
        assert_eq!(estimate(&r, &o), 150);
        o.stage = Some(1);
        assert_eq!(estimate(&r, &o), 50);
        o.stage = None;
        o.from = 10;
        o.to = Some(19);
        assert_eq!(estimate(&r, &o), 10);
        assert_eq!(stage_list(&r), "1 2");
    }

    #[test]
    fn ffmpeg_missing_is_reported() {
        let mut o = Options::new(
            Source::Demo(0),
            Output::Video(std::env::temp_dir().join("th08-nofmpeg.mp4")),
        );
        o.ffmpeg = "/nonexistent/ffmpeg".into();
        let e = Sink::open(&o, (640, 480)).err().unwrap();
        assert!(e.downcast_ref::<NoFfmpeg>().is_some(), "{e}");
        o.output = Output::Video("x.avi".into());
        assert!(Sink::open(&o, (640, 480)).is_err());
    }
}
