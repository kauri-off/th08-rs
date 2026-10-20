//! Port audio back end (cpal): thbgm.dat BGM streaming (02 §1–§4) and the SE voices DirectSound plays (13 §1.1, §1.3).

use crate::assets::{Source, spawn};
use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};

// thbgm.fmt WAVEFORMATEX: 44100 Hz, stereo, 16-bit for every track (02 §2).
const BGM_RATE: f64 = 44100.0;

// thbgm.fmt record (0x34 bytes, 02 §2); positions are in stereo frames (bytes / 4).
#[derive(Clone, Debug)]
pub struct Track {
    pub name: String,
    pub offset: u64,
    pub loop_start: usize,
    // Loop end +0x1C = frames stored in thbgm.dat; +0x14 (full length) is larger and never read (02 §2).
    pub length: usize,
}

// 52-byte records up to the one whose name starts with NUL (02 §2).
pub fn parse_fmt(data: &[u8]) -> Vec<Track> {
    data.chunks_exact(52)
        .take_while(|e| e[0] != 0)
        .map(|e| {
            let u = |o: usize| u32::from_le_bytes(e[o..o + 4].try_into().unwrap());
            Track {
                name: String::from_utf8_lossy(&e[..16])
                    .trim_end_matches('\0')
                    .to_string(),
                offset: u(0x10) as u64,
                loop_start: u(0x18) as usize / 4,
                length: u(0x1c) as usize / 4,
            }
        })
        .collect()
}

// Music_Play's .mid → .wav rename plus Bgm_FindTrack 0x45C6B0 (02 §4): basename, case-sensitive compare,
// no match → track 0 (th08_01.wav). So the std typos th08_02 / th08_13_b play the title theme.
fn find_track<'a>(tracks: &'a [Track], name: &str) -> Option<&'a Track> {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let wav = match base.rfind('.') {
        Some(i) => format!("{}.wav", &base[..i]),
        None => base.to_string(),
    };
    tracks.iter().find(|t| t.name == wav).or(tracks.first())
}

// The playing BGM: whole stored track in memory; after the end it loops from loop_start (02 §3).
#[derive(Default)]
struct Music {
    samples: Vec<[f32; 2]>,
    loop_start: usize,
    pos: f64,
}

// One se_*.wav PCM wave file (RIFF, 8/16-bit, mono/stereo; 04 file index, 13 §1.1).
pub struct Wave {
    samples: Vec<[f32; 2]>,
    rate: f64,
}

// PCM WAV (8/16-bit, mono/stereo) via hound; 8-bit is unsigned on disk, hound yields it centred on 0.
pub fn parse_wav(data: &[u8]) -> Result<Wave> {
    let r = hound::WavReader::new(std::io::Cursor::new(data))?;
    let spec = r.spec();
    let (channels, bits) = (spec.channels as usize, spec.bits_per_sample);
    anyhow::ensure!(
        spec.sample_format == hound::SampleFormat::Int
            && (1..=2).contains(&channels)
            && (bits == 8 || bits == 16),
        "unsupported wav format {:?}/{channels}/{bits}",
        spec.sample_format
    );
    let scale = if bits == 8 { 128.0 } else { 32768.0 };
    let pcm = r
        .into_samples::<i16>()
        .map(|v| v.map(|v| v as f32 / scale))
        .collect::<Result<Vec<f32>, _>>()?;
    let samples = pcm
        .chunks_exact(channels)
        .map(|f| [f[0], f[channels - 1]])
        .collect();
    Ok(Wave {
        samples,
        rate: spec.sample_rate as f64,
    })
}

// One SE id's DirectSound buffer (13 §1.1): a non-looping copy of its wave; gain holds volume and pan per channel.
#[derive(Default, Clone)]
struct Voice {
    wave: Option<Arc<Wave>>,
    pos: f64,
    gain: [f32; 2],
    playing: bool,
}

// DirectSound attenuation (1/100 dB) → amplitude; −10000 is silence.
fn db_gain(db: i32) -> f32 {
    if db <= -10000 {
        0.0
    } else {
        10f32.powf(db as f32 / 2000.0)
    }
}

#[derive(Default)]
struct Mixer {
    voices: Vec<Voice>,
    music: Option<Music>,
    generation: u32,
    out_rate: f64,
    volume: f32,
    paused: bool,
    fade: f32,
    fade_step: f32,
}

impl Mixer {
    fn render(&mut self, out: &mut [f32], channels: usize) {
        let step = BGM_RATE / self.out_rate;
        for fr in out.chunks_exact_mut(channels) {
            // Port mixer: linear resampling to the device rate; WaveStream_* chunking (02 §3) is platform detail.
            if self.fade_step > 0.0 {
                self.fade = (self.fade - self.fade_step).max(0.0);
                if self.fade == 0.0 {
                    self.music = None;
                    self.fade_step = 0.0;
                }
            }
            let (l, r) = match &mut self.music {
                Some(m) if !m.samples.is_empty() && !self.paused => {
                    let n = m.samples.len();
                    let i = m.pos as usize;
                    let t = (m.pos - i as f64) as f32;
                    let a = m.samples[i];
                    let j = if i + 1 >= n { m.loop_start } else { i + 1 };
                    let b = m.samples[j];
                    m.pos += step;
                    // 02 §3: after loop_end, continue from loop_start (track start when loop_start is 0).
                    if m.pos >= n as f64 {
                        m.pos -= (n - m.loop_start) as f64;
                    }
                    (a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t)
                }
                _ => (0.0, 0.0),
            };
            let v = self.volume * self.fade;
            let (mut l, mut r) = (l * v, r * v);
            for vc in self.voices.iter_mut().filter(|v| v.playing) {
                let Some(w) = &vc.wave else { continue };
                let i = vc.pos as usize;
                if i >= w.samples.len() {
                    vc.playing = false;
                    continue;
                }
                let s = w.samples[i];
                l += s[0] * vc.gain[0];
                r += s[1] * vc.gain[1];
                vc.pos += w.rate / self.out_rate;
            }
            if channels > 1 {
                fr[0] = l.clamp(-1.0, 1.0);
                fr[1] = r.clamp(-1.0, 1.0);
            } else {
                fr[0] = ((l + r) * 0.5).clamp(-1.0, 1.0);
            }
        }
    }
}

pub struct Audio {
    source: Source,
    tracks: Vec<Track>,
    mixer: Arc<Mutex<Mixer>>,
    stream: Option<cpal::Stream>,
}

impl Audio {
    pub fn new(source: Source, fmt: &[u8]) -> Self {
        let mixer = Arc::new(Mutex::new(Mixer {
            volume: 1.0,
            fade: 1.0,
            out_rate: BGM_RATE,
            ..Default::default()
        }));
        Self {
            source,
            tracks: parse_fmt(fmt),
            mixer,
            stream: None,
        }
    }

    pub fn is_started(&self) -> bool {
        self.stream.is_some()
    }

    // Must be called from a user gesture on the web (AudioContext policy).
    pub fn ensure_started(&mut self) {
        if self.stream.is_none() {
            match open_stream(self.mixer.clone()) {
                Ok(s) => self.stream = Some(s),
                Err(e) => log::warn!("audio disabled: {e:#}"),
            }
        }
    }

    // Music_Play + Music_PlaySlot (02 §4) in one step, WAV mode only: loads the stored part (never past loop_end) and plays.
    // SPEC-GAP: MIDI mode (cfg +0x1F = 2) and music off (0) are not implemented; the music-room heard flags are set by callers.
    // Accepts script names like "bgm/th08_00.mid"; unknown names fall back to track 0.
    pub fn play_bgm(&mut self, name: &str) {
        let Some(t) = find_track(&self.tracks, name).cloned() else {
            log::warn!("no bgm {name}");
            return;
        };
        let generation = {
            let mut m = self.mixer.lock().unwrap();
            m.generation += 1;
            m.music = None;
            (m.paused, m.fade, m.fade_step) = (false, 1.0, 0.0);
            m.generation
        };
        let (source, mixer) = (self.source.clone(), self.mixer.clone());
        spawn(async move {
            match source.read_range("thbgm.dat", t.offset, t.length * 4).await {
                Ok(buf) => {
                    let samples = buf
                        .chunks_exact(4)
                        .map(|c| {
                            let s =
                                |o: usize| i16::from_le_bytes([c[o], c[o + 1]]) as f32 / 32768.0;
                            [s(0), s(2)]
                        })
                        .collect();
                    let mut m = mixer.lock().unwrap();
                    if m.generation == generation {
                        m.music = Some(Music {
                            samples,
                            loop_start: t.loop_start,
                            pos: 0.0,
                        });
                    }
                }
                Err(e) => log::warn!("bgm {}: {e:#}", t.name),
            }
        });
    }

    // Sound_LoadSE 0x45D3F0 (13 §1.1): one buffer per SE id, each a copy of its wave file; the table volume is not
    // stored because Sound_Update overwrites it on every play (13 §1.3).
    pub fn load_se(&mut self, archive: &crate::archive::Archive) {
        let mut waves: Vec<(&str, Option<Arc<Wave>>)> = Vec::new();
        let mut voices = Vec::new();
        for (name, _) in crate::sound::TABLE {
            let w = match waves.iter().find(|(n, _)| *n == name) {
                Some((_, w)) => w.clone(),
                None => {
                    let w = archive
                        .read(&format!("{name}.wav"))
                        .and_then(|d| parse_wav(&d))
                        .map(Arc::new);
                    if let Err(e) = &w {
                        log::warn!("{name}.wav: {e:#}");
                    }
                    waves.push((name, w.ok()));
                    waves.last().unwrap().1.clone()
                }
            };
            voices.push(Voice {
                wave: w,
                ..Voice::default()
            });
        }
        self.mixer.lock().unwrap().voices = voices;
    }

    // Sound_Update per queued id (13 §1.3): stop, rewind, set pan/volume (1/100 dB) and play once.
    // Pan uses the DirectSound rule: the opposite channel is attenuated by |pan| (1/100 dB).
    pub fn play_se(&mut self, id: usize, pan: i32, volume: i32) {
        let mut m = self.mixer.lock().unwrap();
        let Some(v) = m.voices.get_mut(id) else {
            return;
        };
        let g = db_gain(volume);
        let pan = pan.clamp(-10000, 10000);
        v.gain = [g * db_gain(-pan.max(0)), g * db_gain(pan.min(0))];
        v.pos = 0.0;
        v.playing = v.wave.is_some();
    }

    pub fn pause_bgm(&mut self, on: bool) {
        self.mixer.lock().unwrap().paused = on;
    }

    // Music_Fade 0x4480F8 (10 §5 op 12, 11 §4, 16 §1.3 `@M`): fade out over `secs`, then stop.
    // SPEC-GAP: the fade curve is not specified (linear in amplitude here).
    pub fn fade_bgm(&mut self, secs: f32) {
        let mut m = self.mixer.lock().unwrap();
        m.fade_step = 1.0 / (secs.max(0.01) * m.out_rate as f32);
    }

    // BGM volume through the 13 §1.3 curve with cfg +0x27 (table volume 0); SE volumes are applied per play.
    pub fn set_volumes(&mut self, cfg: &crate::config::Config) {
        let db = crate::config::volume_curve(0, cfg.bgm_volume);
        self.mixer.lock().unwrap().volume = db_gain(db);
    }

    pub fn stop_bgm(&mut self) {
        let mut m = self.mixer.lock().unwrap();
        m.generation += 1;
        m.music = None;
    }
}

// Port: output device setup (prefers 44.1 kHz stereo so the BGM needs no resampling).
fn open_stream(mixer: Arc<Mutex<Mixer>>) -> Result<cpal::Stream> {
    let host = cpal::default_host();
    let pick = |want: &str| {
        host.output_devices().ok()?.find(|d| {
            d.id()
                .map(|i| format!("{i:?}").contains(want))
                .unwrap_or(false)
        })
    };
    let dev = pick("pipewire")
        .or_else(|| pick("pulse"))
        .or_else(|| host.default_output_device())
        .context("no output device")?;
    let def = dev.default_output_config()?;
    let channels = def.channels() as usize;
    let mut cfg: cpal::StreamConfig = def.config();
    if dev
        .supported_output_configs()?
        .any(|c| c.channels() == 2 && c.min_sample_rate() <= 44100 && c.max_sample_rate() >= 44100)
    {
        cfg.sample_rate = 44100;
        cfg.channels = 2;
    }
    let channels = if cfg.channels == 2 { 2 } else { channels };
    mixer.lock().unwrap().out_rate = cfg.sample_rate as f64;
    let stream = dev.build_output_stream(
        cfg,
        move |out: &mut [f32], _| mixer.lock().unwrap().render(out, channels),
        |e| log::warn!("audio error: {e}"),
        None,
    )?;
    stream.play()?;
    Ok(stream)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn se_mix_pan_and_volume() {
        let wave = Arc::new(Wave {
            samples: vec![[0.5, 0.5]; 100],
            rate: 44100.0,
        });
        let mut m = Mixer {
            volume: 1.0,
            fade: 1.0,
            out_rate: 44100.0,
            voices: vec![Voice {
                wave: Some(wave),
                ..Voice::default()
            }],
            ..Default::default()
        };
        let g = db_gain(-600);
        let v = &mut m.voices[0];
        (v.gain, v.playing) = ([g * db_gain(-1000), g], true);
        let mut out = vec![0.0; 2 * 120];
        m.render(&mut out, 2);
        assert!(
            (out[1] - 0.5 * g).abs() < 1e-6 && (out[0] - 0.5 * g * db_gain(-1000)).abs() < 1e-6
        );
        assert_eq!(out[2 * 110], 0.0);
        assert!(!m.voices[0].playing);
        assert!((db_gain(-2000) - 0.1).abs() < 1e-6 && db_gain(-10000) == 0.0);
    }
    #[test]
    fn se_waves_parse() {
        let Some(a) = crate::archive::test_archive() else {
            return;
        };
        for (name, _) in crate::sound::TABLE {
            let data = a.read(&format!("{name}.wav")).unwrap();
            let w = super::parse_wav(&data).unwrap();
            assert!(!w.samples.is_empty() && w.rate > 8000.0, "{name}");
            let r = parse_wav_reference(&data).unwrap();
            assert_eq!(w.rate, r.rate, "{name}");
            assert!(
                w.samples.len() == r.samples.len()
                    && w.samples.iter().zip(&r.samples).all(|(a, b)| {
                        a[0].to_bits() == b[0].to_bits() && a[1].to_bits() == b[1].to_bits()
                    }),
                "{name}: hound samples differ from the RIFF parser"
            );
        }
    }

    // The pre-hound RIFF/WAVE parser, kept as the reference for se_waves_parse.
    fn parse_wav_reference(data: &[u8]) -> Result<Wave> {
        anyhow::ensure!(
            data.len() >= 12 && &data[..4] == b"RIFF" && &data[8..12] == b"WAVE",
            "not a RIFF/WAVE file"
        );
        let (mut fmt, mut pcm) = (None, None);
        let mut p = 12;
        while p + 8 <= data.len() {
            let len = u32::from_le_bytes(data[p + 4..p + 8].try_into().unwrap()) as usize;
            let body = &data[p + 8..(p + 8 + len).min(data.len())];
            match &data[p..p + 4] {
                b"fmt " if body.len() >= 16 => fmt = Some(body),
                b"data" => pcm = Some(body),
                _ => {}
            }
            p += 8 + len + (len & 1);
        }
        let (fmt, pcm) = (
            anyhow::Context::context(fmt, "no fmt chunk")?,
            anyhow::Context::context(pcm, "no data chunk")?,
        );
        let u16_at = |o: usize| u16::from_le_bytes([fmt[o], fmt[o + 1]]);
        let (tag, channels, rate, bits) = (
            u16_at(0),
            u16_at(2) as usize,
            u32::from_le_bytes(fmt[4..8].try_into().unwrap()),
            u16_at(14),
        );
        anyhow::ensure!(
            tag == 1 && (1..=2).contains(&channels) && (bits == 8 || bits == 16),
            "unsupported wav format {tag}/{channels}/{bits}"
        );
        let bytes = bits as usize / 8;
        let sample = |c: &[u8]| {
            if bytes == 1 {
                (c[0] as f32 - 128.0) / 128.0
            } else {
                i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0
            }
        };
        let samples = pcm
            .chunks_exact(bytes * channels)
            .map(|f| {
                let l = sample(f);
                [
                    l,
                    if channels == 2 {
                        sample(&f[bytes..])
                    } else {
                        l
                    },
                ]
            })
            .collect();
        Ok(Wave {
            samples,
            rate: rate as f64,
        })
    }
}
