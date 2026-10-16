use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

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
struct Playing {
    samples: Vec<[i16; 2]>,
    loop_start: usize,
    pos: usize,
    volume: f32,
}

pub struct Bgm {
    path: PathBuf,
    tracks: Vec<Track>,
    state: Arc<Mutex<Playing>>,
    _stream: Option<cpal::Stream>,
}

impl Bgm {
    pub fn new(path: &Path, fmt: &[u8]) -> Self {
        let state = Arc::new(Mutex::new(Playing::default()));
        let stream = open_stream(state.clone()).map_err(|e| eprintln!("audio disabled: {e}")).ok();
        Self { path: path.to_path_buf(), tracks: parse_fmt(fmt), state, _stream: stream }
    }

    pub fn play(&mut self, name: &str) -> Result<()> {
        let t = self.tracks.iter().find(|t| t.name == name).with_context(|| format!("no bgm {name}"))?;
        let mut f = std::fs::File::open(&self.path)?;
        f.seek(SeekFrom::Start(t.offset))?;
        let mut buf = vec![0u8; t.length * 4];
        f.read_exact(&mut buf)?;
        let samples = buf
            .chunks_exact(4)
            .map(|c| [i16::from_le_bytes([c[0], c[1]]), i16::from_le_bytes([c[2], c[3]])])
            .collect();
        *self.state.lock().unwrap() = Playing { samples, loop_start: t.loop_start, pos: 0, volume: 1.0 };
        Ok(())
    }

    pub fn stop(&mut self) {
        *self.state.lock().unwrap() = Playing::default();
    }
}

fn open_stream(state: Arc<Mutex<Playing>>) -> Result<cpal::Stream> {
    let host = cpal::default_host();
    let pick = |want: &str| {
        host.output_devices().ok()?.find(|d| d.id().map(|i| format!("{i:?}").contains(want)).unwrap_or(false))
    };
    let dev = pick("pipewire")
        .or_else(|| pick("pulse"))
        .or_else(|| host.default_output_device())
        .context("no output device")?;
    let cfg = cpal::StreamConfig {
        channels: 2,
        sample_rate: 44100,
        buffer_size: cpal::BufferSize::Default,
    };
    let stream = dev.build_output_stream(
        cfg,
        move |out: &mut [f32], _| {
            let mut p = state.lock().unwrap();
            for fr in out.chunks_exact_mut(2) {
                if p.samples.is_empty() {
                    fr.fill(0.0);
                    continue;
                }
                if p.pos >= p.samples.len() {
                    p.pos = p.loop_start;
                }
                let s = p.samples[p.pos];
                fr[0] = s[0] as f32 / 32768.0 * p.volume;
                fr[1] = s[1] as f32 / 32768.0 * p.volume;
                p.pos += 1;
            }
        },
        |e| eprintln!("audio error: {e}"),
        None,
    )?;
    stream.play()?;
    Ok(stream)
}
