//! `th08-replay`: plays a recorded session or a demo headless and prints its per-row trace hash stream.

use std::io::Write;

const USAGE: &str =
    "usage: th08-replay --game DIR (SESSION_DIR | --demo N) [--out FILE] [--trace FILE]
  Prints `row hash` per trace row and a final `digest D rows N` line (stdout, or --out FILE).
  --trace FILE also writes the rows as the session tool's trace.txt lines (without its header).
  The game directory defaults to $TH08_DIR.";

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let (mut game, mut session, mut demo, mut out, mut trace) = (
        std::env::var_os("TH08_DIR").map(Into::into),
        None,
        None,
        None,
        None,
    );
    while let Some(a) = args.next() {
        match a.as_str() {
            "--game" => game = args.next().map(Into::into),
            "--demo" => demo = args.next().and_then(|v| v.parse::<u8>().ok()),
            "--out" => out = args.next(),
            "--trace" => trace = args.next(),
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            _ if session.is_none() && !a.starts_with('-') => {
                session = Some(std::path::PathBuf::from(a));
            }
            _ => anyhow::bail!("unknown argument {a}\n{USAGE}"),
        }
    }
    let game = game.ok_or_else(|| anyhow::anyhow!("no game directory\n{USAGE}"))?;
    anyhow::ensure!(session.is_some() != demo.is_some(), "{USAGE}");
    let out: Box<dyn Write> = match &out {
        Some(p) => Box::new(std::fs::File::create(p)?),
        None => Box::new(std::io::stdout().lock()),
    };
    let mut s = th08_replay::run::Stream::new(std::io::BufWriter::new(out));
    if let Some(p) = &trace {
        s.text = Some(Box::new(std::io::BufWriter::new(std::fs::File::create(p)?)));
    }
    let mut ctx = th08_replay::run::load(game)?;
    match (&session, demo) {
        (Some(dir), _) => {
            let rec = th08_replay::session::Recording::load(dir)?;
            th08_replay::run::session(&mut ctx, &rec, |r| s.row(r))?;
        }
        (None, Some(n)) => th08_replay::run::demo(&mut ctx, n, |r| s.row(r))?,
        (None, None) => unreachable!(),
    }
    let (digest, rows) = s.finish()?;
    eprintln!("digest {digest:016x} rows {rows}");
    Ok(())
}
