//! BGM sample source (docs/arch/ASSETS_FORMAT.md §3.3, AS2): a thbgm.fmt track's stored PCM `[0, loop_end)` (02 §1–§2)
//! from thbgm.dat by range, or decoded from its Ogg Vorbis stream in thbgm.pak (lewton) and cut to `loop_end`;
//! published progressively into a `Pcm` block by block, so playback starts after the first ring (AS3, §3.4).

use anyhow::{Context, Result, ensure};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use th08_core::assets::Source;
use th08_core::content::{BGM_PACK, bgm_pack_index};
use th08_formats::pack::Index;
use th08_formats::thbgm::Track;

// One stereo frame of thbgm.dat: s16 LE left, right (02 §2 WAVEFORMATEX).
pub type Frame = [i16; 2];

pub enum BgmSource {
    Raw,
    Vorbis(Arc<Index>),
}

impl BgmSource {
    // thbgm.pak when the source has a readable one (as `content::bgm_header` decides), else thbgm.dat.
    pub async fn resolve(source: &Source) -> Self {
        match bgm_pack_index(source).await {
            Some(Ok(i)) => Self::Vorbis(Arc::new(i)),
            Some(Err(e)) => {
                log::warn!("{BGM_PACK}: {e:#}; using thbgm.dat");
                Self::Raw
            }
            None => Self::Raw,
        }
    }

    // The track's `[0, loop_end)` frames (`t.length`, 02 §2) at once (repack's read-back check).
    pub async fn read(&self, source: &Source, t: &Track) -> Result<Vec<Frame>> {
        let pcm = Pcm::new(t.length);
        self.stream(source, t, &mut PcmWriter::new(pcm.clone()))
            .await?;
        Ok((0..pcm.len()).map(|i| pcm.get(i)).collect())
    }

    // Writes the track's `[0, loop_end)` frames into `w` in order; the loop points always come from thbgm.fmt.
    // Stops early (Ok) when `w` reports that nobody plays the track any more.
    pub async fn stream(&self, source: &Source, t: &Track, w: &mut PcmWriter) -> Result<()> {
        match self {
            Self::Raw => {
                // First ring, then 1 MiB pieces: playback can start after the first range read.
                let mut at = 0;
                while at < t.length {
                    let n = if at == 0 { RING_FRAMES } else { 1 << 18 }.min(t.length - at);
                    let b = source
                        .read_range("thbgm.dat", t.offset + at as u64 * 4, n * 4)
                        .await?;
                    if !w.push(&pcm_frames(&b)) {
                        return Ok(());
                    }
                    at += n;
                }
                w.finish();
                Ok(())
            }
            Self::Vorbis(index) => {
                let e = index
                    .entry(&t.name)
                    .with_context(|| format!("{} not in {BGM_PACK}", t.name))?;
                if e.size != t.length as u64 * 4 {
                    log::warn!(
                        "{BGM_PACK} {}: {} PCM bytes, thbgm.fmt loop_end·4 = {}",
                        t.name,
                        e.size,
                        t.length * 4
                    );
                }
                let f = &index.frames[e.frame as usize];
                let ogg = source
                    .read_range(BGM_PACK, f.offset, f.len as usize)
                    .await?;
                if decode_vorbis_into(&ogg, w)? {
                    w.finish();
                }
                Ok(())
            }
        }
    }
}

// Frames per publication block: one ring notification chunk (02 §3, 0.25 s).
pub const BLOCK: usize = 11025;
// The streaming ring (02 §3): 16 chunks; the play command's fill stage needs these first.
pub const RING_FRAMES: usize = 16 * BLOCK;

// A track's `[0, len)` frames, filled block by block while it plays (ASSETS_FORMAT §3.4). Readers never block: a
// block not decoded yet reads as silence and sets `underrun`.
pub struct Pcm {
    len: usize,
    blocks: Box<[OnceLock<Box<[Frame]>>]>,
    ready: AtomicUsize,
    underrun: AtomicBool,
}

impl Pcm {
    pub fn new(len: usize) -> Arc<Self> {
        Arc::new(Self {
            len,
            blocks: (0..len.div_ceil(BLOCK)).map(|_| OnceLock::new()).collect(),
            ready: AtomicUsize::new(0),
            underrun: AtomicBool::new(false),
        })
    }

    // A fully decoded track (tests, callers with all frames in hand).
    pub fn from_frames(s: &[Frame]) -> Arc<Self> {
        let p = Self::new(s.len());
        let mut w = PcmWriter::new(p.clone());
        w.push(s);
        w.finish();
        p
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    // Frames published so far, a prefix `[0, ready)`.
    pub fn ready(&self) -> usize {
        self.ready.load(Ordering::Acquire)
    }

    pub fn underrun(&self) -> bool {
        self.underrun.load(Ordering::Relaxed)
    }

    pub fn get(&self, i: usize) -> Frame {
        match self.blocks[i / BLOCK].get() {
            Some(b) => b[i % BLOCK],
            None => {
                self.underrun.store(true, Ordering::Relaxed);
                [0, 0]
            }
        }
    }
}

type OnBlock = Box<dyn FnMut(&Arc<Pcm>, usize) -> bool + Send>;

// The one writer of a `Pcm`: appends frames, publishes each full block, cuts at `len`, pads the rest on `finish`.
pub struct PcmWriter {
    pcm: Arc<Pcm>,
    cur: Vec<Frame>,
    // Called after each published block with the ready count; false = stop decoding (track no longer wanted).
    on_block: OnBlock,
}

impl PcmWriter {
    pub fn new(pcm: Arc<Pcm>) -> Self {
        Self::with(pcm, |_, _| true)
    }

    pub fn with(pcm: Arc<Pcm>, f: impl FnMut(&Arc<Pcm>, usize) -> bool + Send + 'static) -> Self {
        Self {
            pcm,
            cur: Vec::with_capacity(BLOCK),
            on_block: Box::new(f),
        }
    }

    // Frames still wanted before the cut at `len`.
    pub fn missing(&self) -> usize {
        self.pcm.len - self.pcm.ready() - self.cur.len()
    }

    // Appends frames (beyond `len` dropped); false once `on_block` asked to stop.
    pub fn push(&mut self, mut s: &[Frame]) -> bool {
        while !s.is_empty() && self.missing() > 0 {
            let room = (BLOCK - self.cur.len()).min(self.missing());
            let n = room.min(s.len());
            self.cur.extend_from_slice(&s[..n]);
            s = &s[n..];
            if (self.cur.len() == BLOCK || self.missing() == 0) && !self.publish() {
                return false;
            }
        }
        true
    }

    fn publish(&mut self) -> bool {
        let p = &self.pcm;
        let ready = p.ready();
        let b = std::mem::replace(&mut self.cur, Vec::with_capacity(BLOCK));
        let n = b.len();
        let _ = p.blocks[ready / BLOCK].set(b.into_boxed_slice());
        p.ready.store(ready + n, Ordering::Release);
        (self.on_block)(p, ready + n)
    }

    // End of the decoded data: zero-pad to `len` (as `fit`), publishing the remaining blocks.
    pub fn finish(&mut self) {
        let m = self.missing();
        if m > 0 {
            log::warn!(
                "bgm: decoded {} frames, padding to {}",
                self.pcm.len - m,
                self.pcm.len
            );
            let z = vec![[0, 0]; m];
            self.push(&z);
        } else if !self.cur.is_empty() {
            self.publish();
        }
    }
}

// s16 LE stereo bytes → frames.
pub fn pcm_frames(bytes: &[u8]) -> Vec<Frame> {
    bytes
        .chunks_exact(4)
        .map(|c| {
            [
                i16::from_le_bytes([c[0], c[1]]),
                i16::from_le_bytes([c[2], c[3]]),
            ]
        })
        .collect()
}

// Ogg Vorbis (44.1 kHz stereo) → frames, decoding stops once `max` frames exist. lewton starts at sample 0 of the
// encoder input (lag 0 measured, ASSETS_FORMAT §2.1) and applies no end trim, so the result may run past `max`.
pub fn decode_vorbis(ogg: &[u8], max: usize) -> Result<Vec<Frame>> {
    let mut out: Vec<Frame> = Vec::with_capacity(max.min(1 << 24));
    vorbis_packets(ogg, |p| {
        out.extend(p.as_chunks::<2>().0.iter().copied());
        out.len() < max
    })?;
    Ok(out)
}

// The same decode into a `PcmWriter` (cut at its `len`); false when the writer asked to stop.
pub fn decode_vorbis_into(ogg: &[u8], w: &mut PcmWriter) -> Result<bool> {
    let mut frames = Vec::new();
    let mut go = true;
    vorbis_packets(ogg, |p| {
        frames.clear();
        frames.extend(p.as_chunks::<2>().0.iter().copied());
        go = w.push(&frames);
        go && w.missing() > 0
    })?;
    Ok(go)
}

// Interleaved s16 packets to `f` until it returns false or the stream ends.
fn vorbis_packets(ogg: &[u8], mut f: impl FnMut(&[i16]) -> bool) -> Result<()> {
    let mut r = lewton::inside_ogg::OggStreamReader::new(std::io::Cursor::new(ogg))?;
    let h = &r.ident_hdr;
    ensure!(
        h.audio_channels == 2 && h.audio_sample_rate == 44100,
        "Vorbis stream is {} ch / {} Hz, not 2 / 44100",
        h.audio_channels,
        h.audio_sample_rate
    );
    while let Some(p) = r.read_dec_packet_itl()? {
        if !f(&p) {
            break;
        }
    }
    Ok(())
}

// Cut or zero-pad to `len` frames.
pub fn fit(s: &mut Vec<Frame>, len: usize) {
    if s.len() < len {
        log::warn!("bgm: decoded {} frames, padding to {len}", s.len());
    }
    s.resize(len, [0, 0]);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(i: usize) -> Frame {
        [i as i16, (i >> 15) as i16]
    }

    // Blocks publish in order, input past `len` is cut, `finish` zero-pads, unpublished frames read as silence.
    #[test]
    fn pcm_writer_blocks_cut_and_pad() {
        let len = 3 * BLOCK + 100;
        let p = Pcm::new(len);
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let s2 = seen.clone();
        let mut w = PcmWriter::with(p.clone(), move |_, r| {
            s2.lock().unwrap().push(r);
            true
        });
        let a: Vec<Frame> = (0..BLOCK + 7).map(f).collect();
        assert!(w.push(&a));
        assert_eq!(p.ready(), BLOCK);
        assert_eq!(p.get(BLOCK + 1), [0, 0]);
        assert!(p.underrun());
        let b: Vec<Frame> = (BLOCK + 7..len + 500).map(f).collect();
        assert!(w.push(&b));
        w.finish();
        assert_eq!(p.ready(), len);
        assert!((0..len).all(|i| p.get(i) == f(i)));
        assert_eq!(
            *seen.lock().unwrap(),
            vec![BLOCK, 2 * BLOCK, 3 * BLOCK, len]
        );

        let p = Pcm::new(BLOCK + 3);
        let mut w = PcmWriter::new(p.clone());
        w.push(&[[5, 5]; 10]);
        w.finish();
        assert_eq!(p.ready(), BLOCK + 3);
        assert_eq!(
            (p.get(9), p.get(10), p.get(BLOCK + 2)),
            ([5, 5], [0, 0], [0, 0])
        );
    }

    // `on_block` returning false stops the writer (track replaced while decoding).
    #[test]
    fn pcm_writer_stops() {
        let p = Pcm::new(4 * BLOCK);
        let mut w = PcmWriter::with(p.clone(), |_, r| r < 2 * BLOCK);
        assert!(!w.push(&vec![[1, 1]; 4 * BLOCK]));
        assert_eq!(p.ready(), 2 * BLOCK);
    }
}
