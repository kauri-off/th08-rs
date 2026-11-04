//! BGM sample source (docs/arch/ASSETS_FORMAT.md §3.3, AS2): a thbgm.fmt track's stored PCM `[0, loop_end)` (02 §1–§2)
//! from thbgm.dat by range, or decoded from its Ogg Vorbis stream in thbgm.pak (lewton) and cut to `loop_end`.

use anyhow::{Context, Result, ensure};
use std::sync::Arc;
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

    // The track's `[0, loop_end)` frames (`t.length`, 02 §2); the loop points always come from thbgm.fmt.
    pub async fn read(&self, source: &Source, t: &Track) -> Result<Vec<Frame>> {
        match self {
            Self::Raw => Ok(pcm_frames(
                &source
                    .read_range("thbgm.dat", t.offset, t.length * 4)
                    .await?,
            )),
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
                let mut s = decode_vorbis(&ogg, t.length)?;
                fit(&mut s, t.length);
                Ok(s)
            }
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
    let mut r = lewton::inside_ogg::OggStreamReader::new(std::io::Cursor::new(ogg))?;
    let h = &r.ident_hdr;
    ensure!(
        h.audio_channels == 2 && h.audio_sample_rate == 44100,
        "Vorbis stream is {} ch / {} Hz, not 2 / 44100",
        h.audio_channels,
        h.audio_sample_rate
    );
    let mut out: Vec<Frame> = Vec::with_capacity(max.min(1 << 24));
    while out.len() < max {
        let Some(p) = r.read_dec_packet_itl()? else {
            break;
        };
        out.extend(p.chunks_exact(2).map(|f| [f[0], f[1]]));
    }
    Ok(out)
}

// Cut or zero-pad to `len` frames.
pub fn fit(s: &mut Vec<Frame>, len: usize) {
    if s.len() < len {
        log::warn!("bgm: decoded {} frames, padding to {len}", s.len());
    }
    s.resize(len, [0, 0]);
}
