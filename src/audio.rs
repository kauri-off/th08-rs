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
                name: String::from_utf8_lossy(&e[..16]).trim_end_matches('\0').to_string(),
                offset: u(0x10) as u64,
                loop_start: u(0x18) as usize / 4,
                length: u(0x1c) as usize / 4,
            }
        })
        .collect()
}

#[derive(Default)]
struct Music {
    samples: Vec<[f32; 2]>,
    loop_start: usize,
    pos: f64,
}

#[derive(Default)]
struct Mixer {
    music: Option<Music>,
    generation: u32,
    out_rate: f64,
    volume: f32,
}

impl Mixer {
    fn render(&mut self, out: &mut [f32], channels: usize) {
        let step = BGM_RATE / self.out_rate;
        for fr in out.chunks_exact_mut(channels) {
            let (l, r) = match &mut self.music {
                Some(m) if !m.samples.is_empty() => {
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
            fr[0] = l * self.volume;
            if channels > 1 {
                fr[1] = r * self.volume;
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
        let mixer = Arc::new(Mutex::new(Mixer { volume: 1.0, out_rate: BGM_RATE, ..Default::default() }));
        Self { source, tracks: parse_fmt(fmt), mixer, stream: None }
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

    pub fn play_bgm(&mut self, name: &str) {
        let Some(t) = self.tracks.iter().find(|t| t.name == name).cloned() else {
            log::warn!("no bgm {name}");
            return;
        };
        let generation = {
            let mut m = self.mixer.lock().unwrap();
            m.generation += 1;
            m.music = None;
            m.generation
        };
        let (source, mixer) = (self.source.clone(), self.mixer.clone());
        spawn(async move {
            match source.read_range("thbgm.dat", t.offset, t.length * 4).await {
                Ok(buf) => {
                    let samples = buf
                        .chunks_exact(4)
                        .map(|c| {
                            let s = |o: usize| i16::from_le_bytes([c[o], c[o + 1]]) as f32 / 32768.0;
                            [s(0), s(2)]
                        })
                        .collect();
                    let mut m = mixer.lock().unwrap();
                    if m.generation == generation {
                        m.music = Some(Music { samples, loop_start: t.loop_start, pos: 0.0 });
                    }
                }
                Err(e) => log::warn!("bgm {}: {e:#}", t.name),
            }
        });
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
        host.output_devices()
            .ok()?
            .find(|d| d.id().map(|i| format!("{i:?}").contains(want)).unwrap_or(false))
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
