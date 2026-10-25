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
}

// Streaming-object fade (02 §4): `mode` 1 `Music_Fade`, 2 `Bgm_StartFadeIn`, 3 `Bgm_StartFadeInFromDip`,
// 4 `Bgm_StartDip`; `n` counts down from N = trunc(secs·60) once per Supervisor calc.
struct Fade {
    mode: u8,
    n: i32,
    total: i32,
}

// One fade tick: a new attenuation (1/100 dB), the end (mode 0, last value stays), or stop the buffer (mode 1).
#[derive(Debug, PartialEq)]
enum FadeTick {
    Volume(i32),
    End,
    Stop,
}

impl Fade {
    fn new(mode: u8, secs: f32) -> Self {
        let total = (secs * 60.0) as i32;
        Self {
            mode,
            n: total,
            total,
        }
    }

    // `Sound_TickBgmFades` 0x447764: n −= 1, n < 1 ends; p is linear in dB (integer division).
    fn tick(&mut self) -> FadeTick {
        self.n -= 1;
        let (n, t) = (self.n, self.total);
        match self.mode {
            _ if n >= 1 => FadeTick::Volume(match self.mode {
                1 => n * 5000 / t - 5000,
                2 => -(n * 5000 / t),
                3 => -(n * 1000 / t),
                _ => n * 1000 / t - 1000,
            }),
            1 => FadeTick::Stop,
            _ => FadeTick::End,
        }
    }
}

impl Mixer {
    fn render(&mut self, out: &mut [f32], channels: usize) {
        let step = BGM_RATE / self.out_rate;
        for fr in out.chunks_exact_mut(channels) {
            // Port mixer: linear resampling to the device rate; WaveStream_* chunking (02 §3) is platform detail.
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
            let (mut l, mut r) = (l * self.volume, r * self.volume);
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
    // BGM volume cfg +0x27 (0..100) and the current fade attenuation p (1/100 dB, 0 = none), 02 §4.
    bgm_volume: u8,
    fade_p: i32,
    // Sound command 5 queued by Music_Fade (whole seconds), started by Sound_Update.
    fade_req: Option<i32>,
    fade: Option<Fade>,
    // Music slots 0..15 (02 §3 preload): the name queued by sound command 1 (`Music_Play`), started by `play_slot`.
    slots: [Option<String>; 16],
    // Music mode cfg +0x1F (02 §4): 0 off, 1 WAV, 2 MIDI.
    music: u8,
}

impl Audio {
    pub fn new(source: Source, fmt: &[u8]) -> Self {
        let mixer = Arc::new(Mutex::new(Mixer {
            volume: 1.0,
            out_rate: BGM_RATE,
            ..Default::default()
        }));
        Self {
            source,
            tracks: parse_fmt(fmt),
            mixer,
            stream: None,
            bgm_volume: 100,
            fade_p: 0,
            fade_req: None,
            fade: None,
            slots: Default::default(),
            music: 1,
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

    // Music_Play + Music_PlaySlot (02 §4) in one step: loads the stored part (never past loop_end) and plays; music
    // mode 0 plays nothing. The music-room heard flags are set by callers.
    // SPEC-GAP: MIDI mode (cfg +0x1F = 2) is not implemented; it plays the WAV track.
    // Accepts script names like "bgm/th08_00.mid"; unknown names fall back to track 0.
    pub fn play_bgm(&mut self, name: &str) {
        if self.music == 0 {
            return;
        }
        let Some(t) = find_track(&self.tracks, name).cloned() else {
            log::warn!("no bgm {name}");
            return;
        };
        // Sound_Update play stages 0 (stop: fade mode 0) and 4 (`Bgm_SetVolume(0)`), 02 §4 (RE-Q30); done at once here.
        (self.fade_req, self.fade, self.fade_p) = (None, None, 0);
        self.apply_volume();
        let generation = {
            let mut m = self.mixer.lock().unwrap();
            m.generation += 1;
            m.music = None;
            m.paused = false;
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

    // Music_Play(slot, name) 0x447D56, WAV mode (02 §4): sound command 1 only prepares the slot; nothing is heard.
    pub fn load_bgm(&mut self, slot: usize, name: &str) {
        if self.music == 0 {
            return;
        }
        if let Some(s) = self.slots.get_mut(slot) {
            *s = Some(name.to_owned());
        }
    }

    // Music_PlaySlot 0x447E47 (02 §4): command 2 starts a loaded slot; the music-room heard flags are set by callers.
    pub fn play_slot(&mut self, slot: usize) {
        match self.slots.get(slot).cloned().flatten() {
            Some(name) => self.play_bgm(&name),
            None => log::warn!("bgm slot {slot} not loaded"),
        }
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

    // Music_Fade 0x4480F8 (10 §5 op 12, 11 §4, 16 §1.3 `@M`), WAV mode: queues command 5 with trunc(secs); callers
    // have already divided by the game speed. Linear in dB from 0 to −50 dB over trunc(secs)·60 frames, then stop.
    // Music mode 0 queues nothing.
    pub fn fade_bgm(&mut self, secs: f32) {
        if self.music == 0 {
            return;
        }
        self.fade_req = Some(secs as i32);
    }

    // Sound_Update (13 §1.3) handling command 5: mode 1, N = n = trunc(secs·60); no volume write yet.
    pub fn start_fade(&mut self) {
        if let Some(secs) = self.fade_req.take() {
            self.fade = Some(Fade::new(1, secs as f32));
        }
    }

    // `Bgm_StartFadeIn` 0x406AC0 (02 §4): immediate, mode 2, volume −10000 at once (−50 dB → ≈ 0).
    pub fn start_fade_in(&mut self, secs: f32) {
        self.fade = Some(Fade::new(2, secs));
        self.set_fade_p(-10000);
    }

    // `Bgm_StartDip` 0x406B40 (02 §4): mode 4, no initial write (0 → −10 dB); the music keeps playing.
    pub fn start_dip(&mut self, secs: f32) {
        self.fade = Some(Fade::new(4, secs));
    }

    // `Bgm_StartFadeInFromDip` 0x406BB0 (02 §4): mode 3, volume −1000 first (−10 dB → ≈ 0).
    pub fn start_fade_in_from_dip(&mut self, secs: f32) {
        self.fade = Some(Fade::new(3, secs));
        self.set_fade_p(-1000);
    }

    // 0x447764, every Supervisor calc before the input poll (02 §4): fade tick; mode 1 stops the buffer at the end.
    pub fn tick_fade(&mut self) {
        let Some(f) = &mut self.fade else { return };
        match f.tick() {
            FadeTick::Volume(p) => self.set_fade_p(p),
            FadeTick::End => self.fade = None,
            FadeTick::Stop => {
                self.fade = None;
                self.stop_bgm();
            }
        }
    }

    // `Bgm_SetVolume` 0x472840: every write goes through the BGM-volume scaling (02 §4).
    fn set_fade_p(&mut self, p: i32) {
        self.fade_p = p;
        self.apply_volume();
    }

    // Music mode cfg +0x1F, and the BGM volume through the square curve of 0x472840 (02 §4) with cfg +0x27; SE
    // volumes are applied per play.
    pub fn set_volumes(&mut self, cfg: &crate::config::Config) {
        self.music = cfg.music;
        self.bgm_volume = cfg.bgm_volume;
        self.apply_volume();
    }

    fn apply_volume(&mut self) {
        let db = crate::config::bgm_volume_curve(self.fade_p, self.bgm_volume);
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
    fn music_off_plays_nothing() {
        // cfg +0x1F = 0 (02 §4): no slot is loaded and no fade is queued; mode 1 loads.
        let mut a = Audio::new(Source::native(std::env::temp_dir()), &[]);
        let mut cfg = crate::config::Config::default();
        cfg.music = 0;
        a.set_volumes(&cfg);
        a.load_bgm(0, "bgm/th08_00.mid");
        a.fade_bgm(4.0);
        assert!(a.slots[0].is_none() && a.fade_req.is_none());
        cfg.music = 1;
        a.set_volumes(&cfg);
        a.load_bgm(0, "bgm/th08_00.mid");
        a.fade_bgm(4.0);
        assert!(a.slots[0].is_some() && a.fade_req == Some(4));
    }

    #[test]
    fn fade_linear_in_db() {
        // Music_Fade(4): N = 240; first tick n = 239, middle n = 120, last audible n = 1, then stop.
        let mut f = Fade::new(1, 4.0);
        let p = ticks(&mut f);
        assert_eq!(p.len(), 239);
        assert_eq!([p[0], p[119], p[238]], [-21, -2500, -4980]);
        let bgm = |p, v| crate::config::bgm_volume_curve(p, v);
        assert_eq!(
            [bgm(p[0], 100), bgm(p[119], 100), bgm(p[238], 100)],
            [-21, -2500, -4980]
        );
        // v = 50 (k = 0.75): trunc((p + 5000)·0.75) − 5000.
        assert_eq!(
            [bgm(p[0], 50), bgm(p[119], 50), bgm(p[238], 50)],
            [-1266, -3125, -4985]
        );
        assert_eq!(Fade::new(1, 0.0).tick(), FadeTick::Stop);
    }

    fn ticks(f: &mut Fade) -> Vec<i32> {
        std::iter::from_fn(|| match f.tick() {
            FadeTick::Volume(p) => Some(p),
            _ => None,
        })
        .collect()
    }

    // 02 §4 modes 2/3/4: last values −41 (N 120), −16 and −984 (N 60), then mode 0 with no write.
    #[test]
    fn fade_in_and_dip() {
        let mut f = Fade::new(2, 2.0);
        let p = ticks(&mut f);
        assert_eq!((p.len(), p[0], p[118]), (119, -4958, -41));
        assert_eq!(f.tick(), FadeTick::End);
        let p = ticks(&mut Fade::new(3, 1.0));
        assert_eq!((p.len(), p[0], p[58]), (59, -983, -16));
        let p = ticks(&mut Fade::new(4, 1.0));
        assert_eq!((p.len(), p[0], p[58]), (59, -17, -984));

        let mut a = Audio::new(Source::native(std::env::temp_dir()), &[]);
        a.start_fade_in(2.0);
        assert_eq!(a.fade_p, -10000);
        (0..119).for_each(|_| a.tick_fade());
        assert_eq!(a.fade_p, -41);
        a.tick_fade();
        assert!(a.fade.is_none() && a.fade_p == -41);
        a.start_dip(1.0);
        assert_eq!(a.fade_p, -41);
        a.tick_fade();
        assert_eq!(a.fade_p, -17);
        a.start_fade_in_from_dip(1.0);
        assert_eq!(a.fade_p, -1000);
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
