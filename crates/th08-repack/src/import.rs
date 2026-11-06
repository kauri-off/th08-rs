//! Android game-file import: picked th08 / thbgm files (.dat or .pak) → checked th08.pak + thbgm.pak in a game folder.

use std::fs::{self, File};
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use th08_core::content::{BGM_PACK, CONTENT_PACK};
use th08_formats::pack::Pack;
use th08_formats::thbgm::{Track, parse_fmt};

use crate::{Ctl, Error, Result, Stage, bgm, content};

// Work folder inside the game folder; renamed into place only when everything checked out.
pub const WORK_DIR: &str = ".import";

#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub level: i32,
    pub frame_mb: usize,
    pub vorbis_q: f32,
    pub jobs: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            // zstd 9: 26 MB in ~1.5 s on a desktop core (19: 23 MB, 14 s); the game holds th08.pak in memory.
            level: 9,
            frame_mb: 4,
            vorbis_q: 0.4,
            jobs: std::thread::available_parallelism()
                .map_or(2, std::num::NonZero::get)
                .clamp(1, 4),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Role {
    ContentDat,
    ContentPak,
    BgmDat,
    BgmPak,
}

fn role(name: &str) -> Option<Role> {
    let n = name.to_ascii_lowercase();
    Some(match n.as_str() {
        "th08.dat" => Role::ContentDat,
        "th08.pak" => Role::ContentPak,
        "thbgm.dat" => Role::BgmDat,
        "thbgm.pak" => Role::BgmPak,
        _ => return None,
    })
}

// libvorbis VBR at `quality` (0.4 = ffmpeg -q:a 4) with a fixed stream serial; stops early when `cancel` is set.
pub fn vorbis(
    quality: f32,
    cancel: Option<Arc<AtomicBool>>,
) -> impl Fn(&[u8]) -> std::io::Result<Vec<u8>> + Sync {
    use std::num::{NonZeroU8, NonZeroU32};
    use vorbis_rs::{VorbisBitrateManagementStrategy, VorbisEncoderBuilder};
    move |pcm: &[u8]| {
        let io = |e: vorbis_rs::VorbisError| std::io::Error::other(e.to_string());
        let mut out = Vec::new();
        let mut enc = VorbisEncoderBuilder::new_with_serial(
            NonZeroU32::new(44100).expect("nonzero"),
            NonZeroU8::new(2).expect("nonzero"),
            &mut out,
            0x7408,
        )
        .bitrate_management_strategy(VorbisBitrateManagementStrategy::QualityVbr {
            target_quality: quality,
        })
        .build()
        .map_err(io)?;
        let (mut l, mut r) = (Vec::with_capacity(4096), Vec::with_capacity(4096));
        for block in pcm.as_chunks::<4>().0.chunks(4096) {
            if cancel.as_ref().is_some_and(|c| c.load(Ordering::Relaxed)) {
                return Err(std::io::Error::other("cancelled"));
            }
            l.clear();
            r.clear();
            for c in block {
                l.push(f32::from(i16::from_le_bytes([c[0], c[1]])) / 32768.0);
                r.push(f32::from(i16::from_le_bytes([c[2], c[3]])) / 32768.0);
            }
            enc.encode_audio_block([&l, &r]).map_err(io)?;
        }
        enc.finish().map_err(io)?;
        Ok(out)
    }
}

fn write_synced(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut f = File::create(path)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    Ok(())
}

fn read_all(mut f: File) -> Result<Vec<u8>> {
    let mut v = Vec::new();
    f.read_to_end(&mut v)?;
    Ok(v)
}

fn fmt_tracks(p: &Pack) -> Result<Vec<Track>> {
    let t = parse_fmt(
        &p.read("thbgm.fmt")
            .map_err(|_| Error::NotGameData("th08 content has no thbgm.fmt".into()))?,
    );
    if t.is_empty() {
        return Err(Error::NotGameData("thbgm.fmt lists no tracks".into()));
    }
    Ok(t)
}

// Converts and checks the picked files in `<game>/.import`, then renames th08.pak / thbgm.pak into `game`.
// On any error or cancel the work folder is removed and `game` is left as it was.
pub fn run(
    inputs: Vec<(String, File)>,
    game: &Path,
    o: &Options,
    cancel: Option<Arc<AtomicBool>>,
    ctl: Ctl,
) -> Result<()> {
    let work = game.join(WORK_DIR);
    let _ = fs::remove_dir_all(&work);
    fs::create_dir_all(&work)?;
    let r = convert(inputs, game, &work, o, cancel, ctl);
    if r.is_ok() {
        let mut moved = Ok(());
        for n in [CONTENT_PACK, BGM_PACK] {
            let p = work.join(n);
            if p.is_file() {
                moved = moved.and(fs::rename(&p, game.join(n)));
            }
        }
        let _ = fs::remove_dir_all(&work);
        return moved.map_err(Into::into);
    }
    let _ = fs::remove_dir_all(&work);
    r
}

fn convert(
    inputs: Vec<(String, File)>,
    game: &Path,
    work: &Path,
    o: &Options,
    cancel: Option<Arc<AtomicBool>>,
    ctl: Ctl,
) -> Result<()> {
    let mut content_in = None;
    let mut bgm_in = None;
    for (name, f) in inputs {
        let r = role(&name).ok_or_else(|| {
            Error::NotGameData(format!(
                "{name}: not th08.dat, thbgm.dat, th08.pak or thbgm.pak"
            ))
        })?;
        let slot = if matches!(r, Role::ContentDat | Role::ContentPak) {
            &mut content_in
        } else {
            &mut bgm_in
        };
        if slot.is_some() {
            return Err(Error::NotGameData(format!(
                "{name}: picked twice (one th08 and one thbgm file)"
            )));
        }
        *slot = Some((r, f));
    }
    let content = match content_in {
        Some((Role::ContentDat, f)) => {
            let ar = content::archive(read_all(f)?)?;
            let built = content::build(
                &ar,
                o.frame_mb << 20,
                &mut |b| content::zstd_frame(b, o.level),
                ctl,
            )?;
            let p = content::verify(&built.bytes, &built.data)?;
            content::require_vanilla(&p)?;
            write_synced(&work.join(CONTENT_PACK), &built.bytes)?;
            p
        }
        Some((_, f)) => {
            let bytes = read_all(f)?;
            write_synced(&work.join(CONTENT_PACK), &bytes)?;
            let p = content::check_pack(bytes, ctl)?;
            content::require_vanilla(&p)?;
            p
        }
        None if bgm_in.is_none() => return Err(Error::NotGameData("no files picked".into())),
        None => {
            let p = game.join(CONTENT_PACK);
            let bytes = fs::read(&p).map_err(|_| {
                Error::NotGameData("pick th08.dat (or th08.pak) together with thbgm.dat".into())
            })?;
            Pack::from_bytes(bytes)
                .map_err(|e| Error::NotGameData(format!("{CONTENT_PACK}: {e}")))?
        }
    };
    let Some((r, f)) = bgm_in else { return Ok(()) };
    let tracks = fmt_tracks(&content)?;
    drop(content);
    let out = work.join(BGM_PACK);
    let bytes = if r == Role::BgmDat {
        let enc = vorbis(o.vorbis_q, cancel);
        let built = bgm::build(&tracks, &mut BufReader::new(f), &enc, o.jobs, false, ctl)?;
        let bad = built.loop_failures(&tracks);
        if !bad.is_empty() {
            return Err(Error::Failed(format!("loop self-check failed: {bad:?}")));
        }
        built.bytes
    } else {
        ctl.report(Stage::Bgm, 0, 1);
        read_all(f)?
    };
    write_synced(&out, &bytes)?;
    drop(bytes);
    let back = fs::read(&out)?;
    bgm::check_pack(&back, &tracks)?;
    ctl.report(Stage::Bgm, 1, 1);
    Ok(())
}

// Deletes the imported files (and a leftover work folder).
pub fn remove(game: &Path) -> std::io::Result<()> {
    for n in [CONTENT_PACK, BGM_PACK] {
        match fs::remove_file(game.join(n)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
    }
    let _ = fs::remove_dir_all(game.join(WORK_DIR));
    Ok(())
}

pub mod bridge {
    //! One JSON call for the Android launcher: start / status / cancel / remove of the import job.
    use super::*;
    use serde_json::{Value, json};

    #[derive(Default)]
    struct State {
        busy: bool,
        stage: Option<Stage>,
        done: u64,
        total: u64,
        // (kind, message): ok | cancelled | wrong_file | version | no_space | failed.
        outcome: Option<(&'static str, String)>,
        cancel: Option<Arc<AtomicBool>>,
        secs: f64,
        // The job's thread, joined by `wait` (tests).
        thread: Option<std::thread::JoinHandle<()>>,
    }

    static STATE: Mutex<State> = Mutex::new(State {
        busy: false,
        stage: None,
        done: 0,
        total: 0,
        outcome: None,
        cancel: None,
        secs: 0.0,
        thread: None,
    });

    fn kind(e: &Error) -> &'static str {
        match e {
            Error::NotGameData(_) => "wrong_file",
            Error::Version(_) => "version",
            Error::NoSpace => "no_space",
            Error::Cancelled => "cancelled",
            Error::Io(_) | Error::Failed(_) => "failed",
        }
    }

    fn status(s: &State) -> Value {
        json!({
            "busy": s.busy,
            "stage": s.stage.map(Stage::name),
            "done": s.done,
            "total": s.total,
            "outcome": s.outcome.as_ref().map(|(k, m)| json!({"kind": k, "message": m, "secs": s.secs})),
        })
    }

    #[cfg(unix)]
    fn file_of(v: &Value) -> Option<(String, File)> {
        use std::os::fd::FromRawFd;
        let name = v["name"].as_str()?.to_owned();
        let fd = i32::try_from(v["fd"].as_i64()?).ok()?;
        // SAFETY: the caller hands over ownership of a detached, open descriptor.
        Some((name, unsafe { File::from_raw_fd(fd) }))
    }

    #[cfg(not(unix))]
    fn file_of(v: &Value) -> Option<(String, File)> {
        Some((
            v["name"].as_str()?.to_owned(),
            File::open(v["path"].as_str()?).ok()?,
        ))
    }

    // `low_ram` (the launcher's ActivityManager check): at most 2 tracks encode at once.
    fn start(game: PathBuf, files: Vec<(String, File)>, low_ram: bool, s: &mut State) -> bool {
        let mut o = Options::default();
        if low_ram {
            o.jobs = o.jobs.min(2);
        }
        spawn(s, move |cancel, progress| {
            let r = fs::create_dir_all(&game)
                .map_err(Error::from)
                .and_then(|()| {
                    run(
                        files,
                        &game,
                        &o,
                        Some(cancel.clone()),
                        Ctl::new(cancel, progress),
                    )
                });
            if r.is_err() {
                let _ = fs::remove_dir_all(game.join(WORK_DIR));
            }
            r
        })
    }

    fn lock() -> std::sync::MutexGuard<'static, State> {
        STATE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    // Runs `job` on a thread unless one is running; a panic in it ends the job as failed.
    fn spawn(
        s: &mut State,
        job: impl FnOnce(&Arc<AtomicBool>, &(dyn Fn(Stage, u64, u64) + Sync)) -> Result<()>
        + Send
        + 'static,
    ) -> bool {
        if s.busy {
            return false;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        *s = State {
            busy: true,
            cancel: Some(cancel.clone()),
            ..State::default()
        };
        s.thread = Some(std::thread::spawn(move || {
            let clock = std::time::Instant::now();
            let progress = |st: Stage, done: u64, total: u64| {
                let mut s = lock();
                (s.stage, s.done, s.total) = (Some(st), done, total);
            };
            let r =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| job(&cancel, &progress)));
            let outcome = match r {
                Ok(Ok(())) => ("ok", String::new()),
                // An encoder stopped by the flag reports an io error; the flag decides.
                Ok(Err(_)) if cancel.load(Ordering::Relaxed) => {
                    (kind(&Error::Cancelled), Error::Cancelled.to_string())
                }
                Ok(Err(e)) => (kind(&e), e.to_string()),
                Err(p) => {
                    let m = p
                        .downcast_ref::<&str>()
                        .map(|s| (*s).to_owned())
                        .or_else(|| p.downcast_ref::<String>().cloned())
                        .unwrap_or_else(|| "panic".into());
                    ("failed", format!("internal error: {m}"))
                }
            };
            let mut s = lock();
            s.busy = false;
            s.secs = clock.elapsed().as_secs_f64();
            s.outcome = Some(outcome);
        }));
        true
    }

    pub fn handle(req: &str) -> String {
        let r: Value = match serde_json::from_str(req) {
            Ok(v) => v,
            Err(e) => return json!({"error": format!("bad request: {e}")}).to_string(),
        };
        let game = r["game"].as_str().map(PathBuf::from);
        let mut s = lock();
        let accepted = match r["op"].as_str().unwrap_or("status") {
            "start" => {
                let files: Vec<(String, File)> = r["files"]
                    .as_array()
                    .map(|a| a.iter().filter_map(file_of).collect())
                    .unwrap_or_default();
                match game {
                    Some(g) if !files.is_empty() => {
                        start(g, files, r["low_ram"].as_bool() == Some(true), &mut s)
                    }
                    _ => false,
                }
            }
            "cancel" => {
                if let Some(c) = &s.cancel {
                    c.store(true, Ordering::Relaxed);
                }
                true
            }
            "remove" => !s.busy && game.is_some_and(|g| remove(&g).is_ok()),
            _ => true,
        };
        let mut v = status(&s);
        v["accepted"] = json!(accepted);
        v.to_string()
    }

    #[cfg(test)]
    pub(super) fn spawn_test(
        job: impl FnOnce(&Arc<AtomicBool>, &(dyn Fn(Stage, u64, u64) + Sync)) -> Result<()>
        + Send
        + 'static,
    ) -> bool {
        spawn(&mut lock(), job)
    }

    // Blocks until the running job ended (tests and tools).
    pub fn wait() {
        let t = lock().thread.take();
        if let Some(t) = t {
            let _ = t.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::TAU;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("th08-import-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    // Two tracks (s16 stereo, 44.1 kHz) of tones plus noise repeating every loop length, so the loop seam is
    // continuous, at offsets 16.. of a ZWAV-headed thbgm.dat; and a th08.dat whose thbgm.fmt lists them.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::disallowed_methods,
        reason = "test signal: samples fit i16, bits need not match across targets"
    )]
    fn fixture() -> (Vec<u8>, Vec<u8>) {
        let mut bgm = b"ZWAV\x01\0\0\0\0\0\0\0\0\0\0\0".to_vec();
        let mut fmt = Vec::new();
        for (k, (len, ls)) in [(66_150usize, 22_050usize), (88_200, 10_000)]
            .iter()
            .enumerate()
        {
            let off = bgm.len() as u32;
            let period = len - ls;
            for i in 0..*len {
                let n = (i % period) as u64;
                let t = n as f64 / 44100.0;
                let noise = (n.wrapping_mul(2_862_933_555_777_941_757) >> 52) as f64 - 2048.0;
                let v = 6000.0 * (TAU * 313.7 * (k + 1) as f64 * t).sin()
                    + 3000.0 * (TAU * 1234.5 * t).sin()
                    + noise;
                bgm.extend_from_slice(&(v as i16).to_le_bytes());
                bgm.extend_from_slice(&((v / 2.0) as i16).to_le_bytes());
            }
            let mut rec = [0u8; 52];
            let name = format!("th08_0{k}.wav");
            rec[..name.len()].copy_from_slice(name.as_bytes());
            rec[16..20].copy_from_slice(&off.to_le_bytes());
            rec[24..28].copy_from_slice(&((*ls as u32) * 4).to_le_bytes());
            rec[28..32].copy_from_slice(&((*len as u32) * 4).to_le_bytes());
            fmt.extend_from_slice(&rec);
        }
        fmt.extend_from_slice(&[0; 52]);
        let ecl = b"ECL".repeat(400);
        let dat = th08_formats::archive::pack(&[("thbgm.fmt", &fmt), ("ecldata1.ecl", &ecl)]);
        (dat, bgm)
    }

    fn opts() -> Options {
        Options {
            level: 3,
            ..Options::default()
        }
    }

    #[test]
    fn fixture_tracks_parse() {
        let (dat, _) = fixture();
        let ar = content::archive(dat).unwrap();
        let t = parse_fmt(&ar.read("thbgm.fmt").unwrap());
        assert_eq!(t.len(), 2, "{t:?}");
        assert_eq!(
            (t[0].offset, t[0].loop_start, t[0].length),
            (16, 22_050, 66_150)
        );
    }

    // The fixture is not the vanilla th08.dat: refused as another version, nothing left behind.
    #[test]
    fn foreign_dat_refused_and_clean() {
        let d = tmp("foreign");
        let (dat, bgm) = fixture();
        fs::write(d.join("TH08.DAT"), &dat).unwrap();
        fs::write(d.join("thbgm.dat"), &bgm).unwrap();
        let game = d.join("game");
        fs::create_dir_all(&game).unwrap();
        let inputs = vec![
            (
                "TH08.DAT".to_owned(),
                File::open(d.join("TH08.DAT")).unwrap(),
            ),
            (
                "thbgm.dat".to_owned(),
                File::open(d.join("thbgm.dat")).unwrap(),
            ),
        ];
        let r = run(inputs, &game, &opts(), None, Ctl::default());
        assert!(matches!(r, Err(Error::Version(_))), "{r:?}");
        assert_eq!(fs::read_dir(&game).unwrap().count(), 0);
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn wrong_files_refused() {
        let d = tmp("wrong");
        fs::write(d.join("th08.dat"), b"PK\x03\x04 not an archive").unwrap();
        fs::write(d.join("notes.txt"), b"x").unwrap();
        let game = d.join("game");
        fs::create_dir_all(&game).unwrap();
        let r = run(
            vec![("th08.dat".into(), File::open(d.join("th08.dat")).unwrap())],
            &game,
            &opts(),
            None,
            Ctl::default(),
        );
        assert!(matches!(r, Err(Error::NotGameData(_))), "{r:?}");
        let r = run(
            vec![("notes.txt".into(), File::open(d.join("notes.txt")).unwrap())],
            &game,
            &opts(),
            None,
            Ctl::default(),
        );
        assert!(matches!(r, Err(Error::NotGameData(_))), "{r:?}");
        assert_eq!(fs::read_dir(&game).unwrap().count(), 0);
        fs::remove_dir_all(&d).unwrap();
    }

    // The BGM converter on the fixture: both tracks loop-checked, pack readable through the game's BgmSource index.
    #[test]
    fn bgm_converts() {
        let (dat, bgm) = fixture();
        let ar = content::archive(dat).unwrap();
        let tracks = parse_fmt(&ar.read("thbgm.fmt").unwrap());
        let enc = vorbis(0.4, None);
        let b = bgm::build(&tracks, &mut &bgm[..], &enc, 2, true, Ctl::default()).unwrap();
        assert!(b.loop_failures(&tracks).is_empty());
        assert_eq!(b.info.dat_len, bgm.len() as u64);
        assert_eq!(&b.info.header[..4], b"ZWAV");
        let idx = bgm::check_pack(&b.bytes, &tracks).unwrap();
        assert_eq!(idx.entries.len(), 2);
        assert!(b.tracks.iter().all(|t| t.snr > 10.0));
        // thbgm.dat cut short of its table: refused.
        let short = &bgm[..bgm.len() - 100];
        let r = bgm::build(&tracks, &mut &short[..], &enc, 2, false, Ctl::default());
        assert!(matches!(r, Err(Error::NotGameData(_))));
    }

    // A .pak pair taken as it is: thbgm.pak alone joins an imported th08.pak; a cancelled run leaves nothing new.
    #[test]
    fn pak_import_and_cancel() {
        let d = tmp("pak");
        let (dat, bgm) = fixture();
        let ar = content::archive(dat).unwrap();
        let c = content::build(
            &ar,
            1 << 20,
            &mut |b| content::zstd_frame(b, 3),
            Ctl::default(),
        )
        .unwrap();
        let tracks = parse_fmt(&ar.read("thbgm.fmt").unwrap());
        let b = bgm::build(
            &tracks,
            &mut &bgm[..],
            &vorbis(0.4, None),
            2,
            false,
            Ctl::default(),
        )
        .unwrap();
        fs::write(d.join("ThBgm.PAK"), &b.bytes).unwrap();
        fs::write(d.join("thbgm.dat"), &bgm).unwrap();
        let game = d.join("game");
        fs::create_dir_all(&game).unwrap();
        // No th08 content yet: a thbgm file alone is refused.
        let r = run(
            vec![("ThBgm.PAK".into(), File::open(d.join("ThBgm.PAK")).unwrap())],
            &game,
            &opts(),
            None,
            Ctl::default(),
        );
        assert!(matches!(r, Err(Error::NotGameData(_))), "{r:?}");
        fs::write(game.join(CONTENT_PACK), &c.bytes).unwrap();
        run(
            vec![("ThBgm.PAK".into(), File::open(d.join("ThBgm.PAK")).unwrap())],
            &game,
            &opts(),
            None,
            Ctl::default(),
        )
        .unwrap();
        assert_eq!(fs::read(game.join(BGM_PACK)).unwrap(), b.bytes);
        assert!(!game.join(WORK_DIR).exists());

        fs::remove_file(game.join(BGM_PACK)).unwrap();
        let flag = Arc::new(AtomicBool::new(false));
        let f2 = flag.clone();
        let stop = move |s: Stage, done: u64, _| {
            if s == Stage::Bgm && done > 0 {
                f2.store(true, Ordering::Relaxed);
            }
        };
        let r = run(
            vec![("thbgm.dat".into(), File::open(d.join("thbgm.dat")).unwrap())],
            &game,
            &Options { jobs: 1, ..opts() },
            Some(flag.clone()),
            Ctl::new(&flag, &stop),
        );
        assert!(r.is_err(), "{r:?}");
        let left: Vec<_> = fs::read_dir(&game)
            .unwrap()
            .flatten()
            .map(|e| e.file_name())
            .collect();
        assert_eq!(left, [std::ffi::OsString::from(CONTENT_PACK)]);
        remove(&game).unwrap();
        assert_eq!(fs::read_dir(&game).unwrap().count(), 0);
        fs::remove_dir_all(&d).unwrap();
    }

    // The bridge's job state is global: its tests take turns.
    static BRIDGE: Mutex<()> = Mutex::new(());

    // A panicking job ends as failed, not busy, and the next start is accepted.
    #[test]
    fn bridge_job_panic() {
        let _t = BRIDGE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert!(bridge::spawn_test(|_, _| panic!("boom")));
        bridge::wait();
        let v: serde_json::Value =
            serde_json::from_str(&bridge::handle(r#"{"op": "status"}"#)).unwrap();
        assert_eq!(v["busy"], false);
        assert_eq!(v["outcome"]["kind"], "failed", "{v}");
        assert_eq!(v["outcome"]["message"], "internal error: boom");
        assert!(bridge::spawn_test(|_, _| Ok(())));
        bridge::wait();
    }

    #[cfg(unix)]
    #[test]
    fn bridge_job() {
        use std::os::fd::IntoRawFd;
        let _t = BRIDGE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let d = tmp("bridge");
        let (dat, _) = fixture();
        fs::write(d.join("th08.dat"), &dat).unwrap();
        let fd = File::open(d.join("th08.dat")).unwrap().into_raw_fd();
        let game = d.join("game");
        let req = serde_json::json!({"op": "start", "game": game, "files": [{"name": "th08.dat", "fd": fd}]});
        let v: serde_json::Value = serde_json::from_str(&bridge::handle(&req.to_string())).unwrap();
        assert_eq!(v["accepted"], true);
        bridge::wait();
        let v: serde_json::Value =
            serde_json::from_str(&bridge::handle(r#"{"op": "status"}"#)).unwrap();
        assert_eq!(v["busy"], false);
        assert_eq!(v["outcome"]["kind"], "version", "{v}");
        assert!(!game.join(CONTENT_PACK).exists());
        assert!(bridge::handle("nope").contains("bad request"));
        fs::remove_dir_all(&d).unwrap();
    }

    // The real game files (TH08_DIR): the full device import, then removal.
    #[test]
    fn real_files_import() {
        let Some(dir) = std::env::var_os("TH08_DIR").map(PathBuf::from) else {
            return;
        };
        let find = |n: &str| {
            fs::read_dir(&dir)
                .unwrap()
                .flatten()
                .map(|e| e.path())
                .find(|p| {
                    p.file_name()
                        .is_some_and(|f| f.to_string_lossy().eq_ignore_ascii_case(n))
                })
                .unwrap()
        };
        let d = tmp("real");
        let game = d.join("game");
        fs::create_dir_all(&game).unwrap();
        let t = std::time::Instant::now();
        run(
            vec![
                ("th08.dat".into(), File::open(find("th08.dat")).unwrap()),
                ("thbgm.dat".into(), File::open(find("thbgm.dat")).unwrap()),
            ],
            &game,
            &Options::default(),
            None,
            Ctl::default(),
        )
        .unwrap();
        eprintln!(
            "real import {:.1} s: th08.pak {} B, thbgm.pak {} B",
            t.elapsed().as_secs_f64(),
            fs::metadata(game.join(CONTENT_PACK)).unwrap().len(),
            fs::metadata(game.join(BGM_PACK)).unwrap().len()
        );
        let p = Pack::from_bytes(fs::read(game.join(CONTENT_PACK)).unwrap()).unwrap();
        assert!(th08_core::content::vanilla_mismatch(&p).is_empty());
        remove(&game).unwrap();
        fs::remove_dir_all(&d).unwrap();
    }
}
