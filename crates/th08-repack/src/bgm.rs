//! thbgm.pak from thbgm.dat: per track the PCM `[0, loop_end)` plus 1 s of loop continuation as one Ogg Vorbis stream.

// The decode is cut to `loop_end`. Self-check per track: length ≥ loop_end, lag 0 in ±64 frames, junction RMS ≤ 1.5×
// global RMS error (`Encoded::loop_ok`; seam SNR only reported). thbgm.dat is read once, front to back.
use std::io::Read;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, mpsc};
use std::time::Instant;

use sha2::{Digest, Sha256};
use th08_audio::bgm::{Frame, decode_vorbis, fit};
use th08_formats::pack::{self, BgmInfo, BgmTrack, Index, Kind};
use th08_formats::thbgm::Track;

use crate::{Ctl, Error, Result, Stage};

// Encoder look-ahead past loop_end: the musical continuation, 1 s.
const PAD: usize = 44100;
// Lag search range and windows (frames), seam half-width and junction half-width.
const LAG: i64 = 64;
const LAG_WIN: usize = 65536;
const SEAM: usize = 4096;
const JUNCTION: usize = 32;
// thbgm.dat read size.
const CHUNK: usize = 1 << 20;

// s16 LE stereo PCM at 44.1 kHz → one Ogg Vorbis stream.
pub type Encoder<'a> = &'a (dyn Fn(&[u8]) -> std::io::Result<Vec<u8>> + Sync);

pub struct Encoded {
    pub ogg: Vec<u8>,
    // The decode cut to loop_end; empty unless `keep_decoded`.
    pub dec: Vec<Frame>,
    pub dec_len: usize,
    pub lag: i64,
    pub snr: f64,
    pub seam_snr: f64,
    pub local_snr: f64,
    pub junction: f64,
    pub step: (i32, i32),
    pub secs: f64,
    pub pcm_sha256: [u8; 32],
}

impl Encoded {
    pub fn loop_ok(&self) -> bool {
        self.lag == 0 && self.junction <= 1.5
    }
}

pub struct Built {
    pub bytes: Vec<u8>,
    pub info: BgmInfo,
    // In thbgm.fmt order.
    pub tracks: Vec<Encoded>,
    pub secs: f64,
}

impl Built {
    pub fn loop_failures<'a>(&self, tracks: &'a [Track]) -> Vec<&'a str> {
        tracks
            .iter()
            .zip(&self.tracks)
            .filter(|(_, d)| !d.loop_ok())
            .map(|(t, _)| t.name.as_str())
            .collect()
    }
}

// Frame access shared by the decode and the thbgm.dat bytes, which are read in place (s16 LE stereo).
trait Frames {
    fn n(&self) -> usize;
    fn at(&self, i: usize) -> Frame;
}

impl Frames for [Frame] {
    fn n(&self) -> usize {
        self.len()
    }
    fn at(&self, i: usize) -> Frame {
        self[i]
    }
}

struct Pcm<'a>(&'a [[u8; 4]]);

impl Frames for Pcm<'_> {
    fn n(&self) -> usize {
        self.0.len()
    }
    fn at(&self, i: usize) -> Frame {
        let c = self.0[i];
        [
            i16::from_le_bytes([c[0], c[1]]),
            i16::from_le_bytes([c[2], c[3]]),
        ]
    }
}

fn sq(f: Frame) -> f64 {
    f64::from(f[0]).powi(2) + f64::from(f[1]).powi(2)
}

fn err(a: Frame, b: Frame) -> f64 {
    (f64::from(a[0]) - f64::from(b[0])).powi(2) + (f64::from(a[1]) - f64::from(b[1])).powi(2)
}

fn sse(o: &(impl Frames + ?Sized), d: &[Frame]) -> f64 {
    (0..o.n().min(d.len())).map(|i| err(o.at(i), d[i])).sum()
}

fn snr(o: &(impl Frames + ?Sized), d: &[Frame]) -> f64 {
    let s: f64 = (0..o.n()).map(|i| sq(o.at(i))).sum();
    10.0 * (s / sse(o, d).max(1e-9)).log10()
}

// Cross-correlation peak of `d` against `o` over three windows; 0 = sample-aligned.
#[expect(
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    reason = "start ≥ LAG keeps i + l ≥ 0; frame indices < 2^63"
)]
fn lag(o: &(impl Frames + ?Sized), d: &[Frame]) -> i64 {
    let n = o.n();
    let mut best = (i128::MIN, 0);
    for l in -LAG..=LAG {
        let mut acc: i128 = 0;
        for w in [n / 4, n / 2, 3 * n / 4] {
            let start = w.max(LAG as usize);
            let end = (start + LAG_WIN).min(n.saturating_sub(LAG as usize));
            for i in start..end.min(n) {
                let (oi, j) = (o.at(i), (i as i64 + l) as usize);
                acc += i128::from(i32::from(oi[0]) * i32::from(d[j][0]))
                    + i128::from(i32::from(oi[1]) * i32::from(d[j][1]));
            }
        }
        if acc > best.0 {
            best = (acc, l);
        }
    }
    best.1
}

fn splice(s: &(impl Frames + ?Sized), le: usize, ls: usize, w: usize) -> Vec<Frame> {
    (le - w.min(le)..s.n())
        .chain(ls..(ls + w).min(le))
        .map(|i| s.at(i))
        .collect()
}

fn step(s: &(impl Frames + ?Sized), le: usize, ls: usize) -> i32 {
    (i32::from(s.at(le - 1)[0]) - i32::from(s.at(ls)[0])).abs()
}

fn rms(o: &(impl Frames + ?Sized), d: &[Frame]) -> f64 {
    (sse(o, d) / o.n().max(1) as f64).sqrt()
}

// `pcm` = the track's bytes `[offset, offset + 4·loop_end)`; the encoder's pad is appended to it in place.
fn encode_track(t: &Track, mut pcm: Vec<u8>, encode: Encoder, keep: bool) -> Result<Encoded> {
    let clock = Instant::now();
    let (le, ls) = (t.length, t.loop_start);
    pcm.extend_from_within(4 * ls..4 * (ls + PAD).min(le));
    let ogg = encode(&pcm)?;
    pcm.truncate(4 * le);
    let mut dec = decode_vorbis(&ogg, usize::MAX)
        .map_err(|e| Error::Failed(format!("{}: decode of the encoded track: {e:#}", t.name)))?;
    let dec_len = dec.len();
    if dec_len < le {
        return Err(Error::Failed(format!(
            "{}: decoded {dec_len} < loop_end {le} frames",
            t.name
        )));
    }
    fit(&mut dec, le);
    let o = Pcm(pcm.as_chunks::<4>().0);
    let (os, ds) = (splice(&o, le, ls, SEAM), splice(&*dec, le, ls, SEAM));
    let j = SEAM.min(le);
    let jr = j.saturating_sub(JUNCTION)..(j + JUNCTION).min(os.len());
    let junction = rms(&os[jr.clone()], &ds[jr]) / rms(&o, &dec).max(1e-9);
    Ok(Encoded {
        lag: lag(&o, &dec),
        snr: snr(&o, &dec),
        seam_snr: snr(&*os, &ds),
        local_snr: snr(
            &*splice(&o, le, ls, 8 * SEAM),
            &splice(&*dec, le, ls, 8 * SEAM),
        ),
        junction,
        step: (step(&o, le, ls), step(&*dec, le, ls)),
        ogg,
        dec: if keep { dec } else { Vec::new() },
        dec_len,
        secs: clock.elapsed().as_secs_f64(),
        pcm_sha256: pack::sha256(&pcm),
    })
}

// thbgm.dat bytes in order, hashed as they pass; `need(end)` keeps reading until `end` bytes were seen.
struct Stream<'a> {
    r: &'a mut dyn Read,
    buf: Vec<u8>,
    base: u64,
    seen: u64,
    head: Vec<u8>,
    sha: Sha256,
}

impl Stream<'_> {
    fn more(&mut self) -> Result<bool> {
        let at = self.buf.len();
        self.buf.resize(at + CHUNK, 0);
        let n = loop {
            match self.r.read(&mut self.buf[at..]) {
                Ok(n) => break n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e.into()),
            }
        };
        self.buf.truncate(at + n);
        self.sha.update(&self.buf[at..]);
        if self.head.len() < 16 {
            let k = (16 - self.head.len()).min(n);
            self.head.extend_from_slice(&self.buf[at..at + k]);
        }
        self.seen += n as u64;
        Ok(n > 0)
    }

    // Reads until `end` bytes were seen; the buffer is sized once for them plus `spare` bytes.
    fn need(&mut self, end: u64, spare: usize, ctl: &Ctl) -> Result<()> {
        let want = (end.max(self.seen) - self.base) as usize + CHUNK + spare;
        self.buf.reserve_exact(want.saturating_sub(self.buf.len()));
        while self.seen < end {
            ctl.check()?;
            if !self.more()? {
                return Err(Error::NotGameData(
                    "thbgm.dat is shorter than its track table (thbgm.fmt) says".into(),
                ));
            }
        }
        Ok(())
    }

    fn slice(&self, start: u64, end: u64) -> &[u8] {
        &self.buf[(start - self.base) as usize..(end - self.base) as usize]
    }

    // Moves `[start, end)` out of the buffer without copying it; everything before `end` is dropped.
    fn take(&mut self, start: u64, end: u64) -> Vec<u8> {
        self.drop_before(start);
        let rest = self.buf.split_off((end - self.base) as usize);
        self.base = end;
        std::mem::replace(&mut self.buf, rest)
    }

    fn drop_before(&mut self, at: u64) {
        let k = (at.min(self.seen) - self.base) as usize;
        self.buf.drain(..k);
        self.base += k as u64;
    }
}

// thbgm.pak bytes for `tracks` (thbgm.fmt) from the thbgm.dat stream `dat`; `jobs` tracks encode at once.
// `keep_decoded` keeps every track's checked decode (the repack read-back compare; ~4 B per frame).
pub fn build(
    tracks: &[Track],
    dat: &mut dyn Read,
    encode: Encoder,
    jobs: usize,
    keep_decoded: bool,
    ctl: Ctl,
) -> Result<Built> {
    if tracks.is_empty() {
        return Err(Error::NotGameData("thbgm.fmt lists no tracks".into()));
    }
    for t in tracks {
        if t.loop_start >= t.length {
            return Err(Error::NotGameData(format!(
                "{}: loop_start {} ≥ loop_end {}",
                t.name, t.loop_start, t.length
            )));
        }
    }
    let clock = Instant::now();
    let total: u64 = tracks.iter().map(|t| t.length as u64 * 4).sum();
    let finished = AtomicU64::new(0);
    ctl.report(Stage::Bgm, 0, total);
    let mut order: Vec<usize> = (0..tracks.len()).collect();
    order.sort_by_key(|&i| tracks[i].offset);
    let done: Mutex<Vec<Option<Result<Encoded>>>> =
        Mutex::new((0..tracks.len()).map(|_| None).collect());
    // One read-ahead track: every queued track is a whole track's PCM in memory.
    let (tx, rx) = mpsc::sync_channel::<(usize, Vec<u8>)>(1);
    let rx = Mutex::new(rx);
    let mut s = Stream {
        r: dat,
        buf: Vec::new(),
        base: 0,
        seen: 0,
        head: Vec::new(),
        sha: Sha256::new(),
    };
    let fed = std::thread::scope(|sc| {
        for _ in 0..jobs.max(1) {
            sc.spawn(|| {
                loop {
                    let job = rx.lock().unwrap().recv();
                    let Ok((i, pcm)) = job else { break };
                    let t = &tracks[i];
                    let r = ctl
                        .check()
                        .and_then(|()| encode_track(t, pcm, encode, keep_decoded));
                    let n = finished.fetch_add(t.length as u64 * 4, Ordering::Relaxed);
                    ctl.report(Stage::Bgm, n + t.length as u64 * 4, total);
                    done.lock().unwrap()[i] = Some(r);
                }
            });
        }
        let r = (|| -> Result<()> {
            for (k, &i) in order.iter().enumerate() {
                let t = &tracks[i];
                let end = t.offset + t.length as u64 * 4;
                let pad = 4 * PAD.min(t.length - t.loop_start);
                s.need(end, pad, &ctl)?;
                let next = order.get(k + 1).map(|&n| tracks[n].offset);
                let pcm = if next.is_some_and(|n| n < end) {
                    let mut v = Vec::with_capacity(t.length * 4 + pad);
                    v.extend_from_slice(s.slice(t.offset, end));
                    v
                } else {
                    s.take(t.offset, end)
                };
                if tx.send((i, pcm)).is_err() {
                    break;
                }
                if let Some(&next) = order.get(k + 1) {
                    s.drop_before(tracks[next].offset);
                }
            }
            s.drop_before(u64::MAX);
            while s.more()? {
                ctl.check()?;
                s.drop_before(u64::MAX);
            }
            Ok(())
        })();
        drop(tx);
        r
    });
    fed?;
    ctl.check()?;
    if s.seen < 16 {
        return Err(Error::NotGameData("thbgm.dat too short".into()));
    }
    let info = BgmInfo {
        header: s.head[..16].try_into().expect("16 bytes"),
        dat_len: s.seen,
        dat_sha256: s.sha.finalize().into(),
    };
    let done: Vec<Encoded> = done
        .into_inner()
        .unwrap()
        .into_iter()
        .zip(tracks)
        .map(|(d, t)| d.unwrap_or_else(|| Err(Error::Failed(format!("{}: not encoded", t.name)))))
        .collect::<Result<_>>()?;
    let entries: Vec<BgmTrack> = tracks
        .iter()
        .zip(&done)
        .map(|(t, d)| BgmTrack {
            name: &t.name,
            ogg: &d.ogg,
            decoded_size: d.dec_len as u64 * 4,
            pcm_size: t.length as u64 * 4,
            pcm_sha256: d.pcm_sha256,
        })
        .collect();
    let bytes = pack::write_bgm(&entries, &info);
    Ok(Built {
        bytes,
        info,
        tracks: done,
        secs: clock.elapsed().as_secs_f64(),
    })
}

// A thbgm.pak's index: a BGM pack holding every thbgm.fmt track, each stream an Ogg Vorbis 44.1 kHz stereo header.
pub fn check_pack(bytes: &[u8], tracks: &[Track]) -> Result<Index> {
    let bad = |m: String| Error::NotGameData(format!("thbgm.pak: {m}"));
    let (off, len) = Index::manifest_range(bytes).map_err(|e| bad(e.to_string()))?;
    let m = usize::try_from(off)
        .ok()
        .and_then(|o| bytes.get(o..o.checked_add(len)?))
        .ok_or_else(|| bad("manifest range".into()))?;
    let idx = Index::parse(bytes, m).map_err(|e| bad(e.to_string()))?;
    if idx.kind != Kind::Bgm || idx.bgm.is_none() {
        return Err(bad("not a BGM pack".into()));
    }
    for t in tracks {
        let e = idx
            .entry(&t.name)
            .ok_or_else(|| bad(format!("{} missing", t.name)))?;
        let f = &idx.frames[e.frame as usize];
        let ogg = &bytes[f.offset as usize..(f.offset + f.len) as usize];
        let r = lewton::inside_ogg::OggStreamReader::new(std::io::Cursor::new(ogg))
            .map_err(|e| bad(format!("{}: {e}", t.name)))?;
        if r.ident_hdr.audio_channels != 2 || r.ident_hdr.audio_sample_rate != 44100 {
            return Err(bad(format!("{}: not 44.1 kHz stereo", t.name)));
        }
    }
    Ok(idx)
}
