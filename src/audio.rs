use crate::assets::{Source, spawn};
use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};

const BGM_RATE: f64 = 44100.0;

#[derive(Clone, Debug)]
pub struct Track {
    pub name: String,
    pub offset: u64,
    pub loop_start: usize,
    pub length: usize,
}

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

fn find_track<'a>(tracks: &'a [Track], name: &str) -> Option<&'a Track> {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let wav = match base.rfind('.') {
        Some(i) => format!("{}.wav", &base[..i]),
        None => base.to_string(),
    };
    tracks.iter().find(|t| t.name == wav).or(tracks.first())
}

#[derive(Default)]
struct Music {
    samples: Vec<[f32; 2]>,
    loop_start: usize,
    pos: f64,
}

// One PCM wave file (RIFF, 8/16-bit, mono/stereo).
pub struct Wave {
    samples: Vec<[f32; 2]>,
    rate: f64,
}

pub fn parse_wav(data: &[u8]) -> Result<Wave> {
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
    let (fmt, pcm) = (fmt.context("no fmt chunk")?, pcm.context("no data chunk")?);
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

#[derive(Default, Clone)]
struct Voice {
    wave: Option<Arc<Wave>>,
    pos: f64,
    gain: [f32; 2],
    playing: bool,
}

// DirectSound attenuation (1/100 dB) → amplitude.
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

    // Sound_LoadSE (13 §1.1): one buffer per SE id, each a copy of its wave file.
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

    // Stop, rewind, set pan/volume (1/100 dB) and play once (13 §1.3).
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

    // Music_Fade: linear fade-out over `secs`, then stop.
    pub fn fade_bgm(&mut self, secs: f32) {
        let mut m = self.mixer.lock().unwrap();
        m.fade_step = 1.0 / (secs.max(0.01) * m.out_rate as f32);
    }

    // BGM volume through the 13 §1.3 curve (table volume 0); SE volumes are applied per play.
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
            let w = super::parse_wav(&a.read(&format!("{name}.wav")).unwrap()).unwrap();
            assert!(!w.samples.is_empty() && w.rate > 8000.0, "{name}");
        }
    }
}
