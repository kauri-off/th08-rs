//! `pack-check <pack dir|zip> [--dat th08.dat] [--max N]` — missing glyphs, per-site overflow,
//! unknown / unmapped keys of a presentation pack. Exit 1 on unknown keys, mount warnings or glyphs no font has.

use anyhow::{Context, Result, bail};

fn main() -> Result<()> {
    let mut a = std::env::args().skip(1);
    let (mut pack, mut dat, mut max) = (None, None, 10usize);
    while let Some(x) = a.next() {
        match x.as_str() {
            "--dat" => dat = Some(a.next().context("--dat FILE")?),
            "--max" => max = a.next().context("--max N")?.parse()?,
            s if !s.starts_with('-') && pack.is_none() => pack = Some(s.to_string()),
            s => bail!(
                "unknown argument {s}\nusage: pack-check <pack dir|zip> [--dat th08.dat] [--max N]"
            ),
        }
    }
    let pack = pack.context("usage: pack-check <pack dir|zip> [--dat th08.dat] [--max N]")?;
    let files = th08::packs::read_files(std::path::Path::new(&pack))?;
    let vanilla = dat
        .map(|d| th08_thcrap::thcrap::Vanilla::from_dat(std::path::Path::new(&d)))
        .transpose()?;
    let s = th08_tools::packcheck::check(&files, vanilla.as_ref(), max)?;
    print!("{}", s.text);
    println!("errors: {}", s.errors);
    if s.errors > 0 {
        std::process::exit(1);
    }
    Ok(())
}
