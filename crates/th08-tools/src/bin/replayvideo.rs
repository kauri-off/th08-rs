//! Replay to video: renders a .rpy or attract demo headless and encodes it with its sound (ffmpeg, or PNG + WAV).

use clap::{Arg, ArgAction, Command, value_parser};
use th08::platform::hitbox::Hitboxes;
use th08::platform::overlay::Corner;
use th08::videoexport::{self, Options, Output, Source};

fn cli() -> Command {
    Command::new("replayvideo")
        .about("Renders a replay headless at render scale 1–3 and 60 fps, mixes its BGM and sound effects offline \
                and encodes both through ffmpeg (H.264 / AAC in .mp4 or .mkv), or writes a PNG sequence + WAV. \
                Needs TH08_DIR; nothing is played.")
        .arg(Arg::new("source").required(true).help("a .rpy file, or demo:N (attract demo N of the archive)"))
        .arg(Arg::new("out").short('o').long("out").help("output video (.mp4 or .mkv)"))
        .arg(Arg::new("png_wav").long("png-wav").value_name("DIR")
            .help("write DIR/NNNNNN.png per frame and DIR/audio.wav instead of a video"))
        .arg(Arg::new("scale").long("scale").default_value("1").value_parser(value_parser!(u32).range(1..=3))
            .help("render scale: frames are 640k × 480k"))
        .arg(Arg::new("stage").long("stage").value_parser(value_parser!(u64).range(1..=9))
            .help("play only this stage record (1-based)"))
        .arg(Arg::new("from").long("from").default_value("0").value_parser(value_parser!(u64))
            .help("first video frame (0 = the first frame of the playback)"))
        .arg(Arg::new("to").long("to").value_parser(value_parser!(u64)).help("last video frame"))
        .arg(Arg::new("hitboxes").long("hitboxes").default_value("off")
            .value_parser(["off", "player", "all"]).help("hitbox overlay"))
        .arg(Arg::new("inputs").long("inputs").default_value("off").value_parser(Corner::IDS)
            .help("input display corner"))
        .arg(Arg::new("rate").long("rate").default_value("44100").value_parser(value_parser!(u32))
            .help("audio sample rate"))
        .arg(Arg::new("crf").long("crf").default_value("18").value_parser(value_parser!(u32).range(0..=51))
            .help("x264 quality"))
        .arg(Arg::new("preset").long("preset").default_value("fast").help("x264 preset"))
        .arg(Arg::new("ffmpeg").long("ffmpeg").default_value("ffmpeg").help("ffmpeg executable"))
        .arg(Arg::new("quiet").short('q').long("quiet").action(ArgAction::SetTrue).help("no progress line"))
}

fn main() -> anyhow::Result<()> {
    let _ = env_logger::try_init();
    let m = cli().get_matches();
    let get = |k: &str| m.get_one::<String>(k).cloned().unwrap_or_default();
    let output = match (m.get_one::<String>("out"), m.get_one::<String>("png_wav")) {
        (Some(o), None) => Output::Video(o.into()),
        (None, Some(d)) => Output::PngWav(d.into()),
        _ => anyhow::bail!("give exactly one of --out FILE and --png-wav DIR"),
    };
    let mut o = Options::new(Source::parse(&get("source"))?, output);
    o.scale = *m.get_one("scale").unwrap();
    o.stage = m.get_one::<u64>("stage").map(|&s| s as usize - 1);
    o.from = *m.get_one("from").unwrap();
    o.to = m.get_one("to").copied();
    o.hitboxes = Hitboxes::parse(&get("hitboxes")).unwrap_or_default();
    o.inputs = Corner::parse(&get("inputs"));
    o.rate = *m.get_one("rate").unwrap();
    o.crf = *m.get_one("crf").unwrap();
    o.preset = get("preset");
    o.ffmpeg = get("ffmpeg").into();
    let quiet = m.get_flag("quiet");
    let s = th08::settings::get();
    let t0 = std::time::Instant::now();
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let r = videoexport::export(s, &o, &cancel, |p| {
        if !quiet && p.frames % 600 == 0 {
            eprint!(
                "\rframe {} / ~{} (stage {}), {:.0} fps   ",
                p.frames,
                p.total,
                p.stage + 1,
                p.frames as f64 / t0.elapsed().as_secs_f64()
            );
        }
    })?;
    if !quiet {
        eprintln!();
    }
    println!(
        "{} frames {}x{} ({:.3} s video), {} sample frames at {} Hz ({:.6} s audio), drift {:+.6} s, \
         {} logic frames, {:.1} s",
        r.frames,
        r.size.0,
        r.size.1,
        r.frames as f64 / 60.0,
        r.samples,
        r.rate,
        r.samples as f64 / f64::from(r.rate),
        r.drift_secs(),
        r.run_frames,
        t0.elapsed().as_secs_f64()
    );
    Ok(())
}
