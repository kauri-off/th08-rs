//! thcrap-import: a thpatch language → a local presentation pack, downloaded on demand from srv.thpatch.net or read
//! from a local thcrap `lang_xx` folder (CLI over the th08-thcrap library).

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use th08_thcrap::build::{self, Options};
use th08_thcrap::thcrap::Vanilla;
use th08_thcrap::thpatch::{self, Client};
use th08_thcrap::{Stdout, manage};

const USAGE: &str = "usage: thcrap-import --list [--coverage --dat th08.dat] [--cache DIR]
       thcrap-import --lang <code|lang_xx> --dat th08.dat [--cache DIR] [-o PACK_DIR] [--no-images] [--force]
       thcrap-import <local thcrap lang_xx dir> --dat th08.dat [-o PACK_DIR] [--no-images] [--force]
--dat takes th08.dat or th08.pak. Cache and packs default to <target>/thcrap and <target>/packs (never commit them).";

#[derive(Default)]
struct Args {
    // `--list`; Some(true) = with `--coverage`.
    list: Option<bool>,
    lang: Option<String>,
    local: Option<PathBuf>,
    dat: Option<PathBuf>,
    cache: PathBuf,
    out: Option<PathBuf>,
    opts: Options,
}

fn target() -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| PathBuf::from("target"), PathBuf::from)
}

fn args() -> Result<Args> {
    let mut it = std::env::args().skip(1);
    let mut a = Args {
        cache: target().join("thcrap"),
        ..Args::default()
    };
    while let Some(x) = it.next() {
        let mut val = || {
            it.next()
                .with_context(|| format!("{x} needs a value\n{USAGE}"))
        };
        match x.as_str() {
            "--list" => a.list = Some(a.list.unwrap_or(false)),
            "--coverage" => a.list = Some(true),
            "--lang" => a.lang = Some(val()?),
            "--dat" => a.dat = Some(PathBuf::from(val()?)),
            "--cache" => a.cache = PathBuf::from(val()?),
            "-o" | "--out" => a.out = Some(PathBuf::from(val()?)),
            "--no-images" => a.opts.no_images = true,
            "--force" => a.opts.force = true,
            "-h" | "--help" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            s if !s.starts_with('-') && a.local.is_none() => a.local = Some(PathBuf::from(s)),
            s => bail!("unknown argument {s}\n{USAGE}"),
        }
    }
    let modes = [a.list.is_some(), a.lang.is_some(), a.local.is_some()];
    if modes.iter().filter(|&&b| b).count() != 1 {
        bail!("give one of --list, --lang, a lang_xx folder\n{USAGE}");
    }
    if a.list != Some(false) && a.dat.is_none() {
        bail!("--dat is required\n{USAGE}");
    }
    Ok(a)
}

fn vanilla(a: &Args) -> Result<Vanilla> {
    Vanilla::from_dat(a.dat.as_deref().context("--dat")?)
}

fn build(
    a: &Args,
    patch: &str,
    title: &str,
    layers: &[PathBuf],
    fonts: &thpatch::Fonts,
) -> Result<()> {
    let code = patch.strip_prefix("lang_").unwrap_or(patch);
    let out = a
        .out
        .clone()
        .unwrap_or_else(|| target().join("packs").join(format!("lang_{code}")));
    let s = build::build_pack(
        patch,
        title,
        layers,
        fonts,
        &vanilla(a)?,
        &out,
        a.opts,
        &Stdout,
    )?;
    println!(
        "{}: {} texts, {} images, {} pack fonts\ncoverage:\n{}{}",
        s.out.display(),
        s.texts,
        s.images,
        s.fonts,
        s.coverage,
        s.font_text
    );
    print!("{}", s.report.render(3));
    Ok(())
}

// `--list`: every server language with th08 files; `--coverage` imports each (text only, cached) for its share.
fn list(a: &Args) -> Result<()> {
    let c = Client::new();
    let rows = manage::server_languages(&c)?;
    let v = if a.list == Some(true) {
        Some(vanilla(a)?)
    } else {
        None
    };
    println!(
        "{:<20} {:<40} {:<14} own  +base  img  coverage",
        "language", "title", "on top of"
    );
    let quiet = Quiet;
    for r in rows {
        let cov = match &v {
            Some(v) => match manage::coverage(&c, &r.id, &a.cache, v, &quiet) {
                Ok((pct, per)) => {
                    let kinds: Vec<String> = per
                        .iter()
                        .map(|(k, (h, n))| {
                            format!(
                                "{k} {:.0}%",
                                if *n == 0 {
                                    0.0
                                } else {
                                    100.0 * *h as f64 / *n as f64
                                }
                            )
                        })
                        .collect();
                    format!("{pct:5.1}%  ({})", kinds.join(", "))
                }
                Err(e) => format!("error: {e:#}"),
            },
            None => format!("~{:.0}%", r.estimate()),
        };
        let title: String = r.title.chars().take(40).collect();
        println!(
            "{:<20} {title:<40} {:<14} {:>2}/47 {:>2}/47 {:>4}  {cov}",
            r.id,
            r.base.join(","),
            r.text_files,
            r.chain_files,
            r.images
        );
    }
    Ok(())
}

struct Quiet;

impl th08_thcrap::Progress for Quiet {
    fn line(&self, _: &str) {}
}

fn main() -> Result<()> {
    let a = args()?;
    if a.list.is_some() {
        return list(&a);
    }
    if let Some(l) = &a.lang {
        let id = if l.starts_with("lang_") {
            l.clone()
        } else {
            format!("lang_{l}")
        };
        anyhow::ensure!(thpatch::valid_lang(&id), "{l}: not a language code");
        let c = Client::new();
        let f = thpatch::fetch(&c, &id, &a.cache, true, &Stdout)?;
        return build(&a, &id, &f.title, &f.layers, &f.fonts);
    }
    let dir = a.local.clone().context("lang dir")?;
    let patch = dir
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    anyhow::ensure!(
        thpatch::valid_lang(&patch),
        "{}: not a lang_xx folder",
        dir.display()
    );
    let fonts = thpatch::local_fonts(&dir);
    let title = std::fs::read(dir.join("patch.js"))
        .ok()
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        .and_then(|v| v.get("title")?.as_str().map(str::to_string))
        .unwrap_or_default();
    build(&a, &patch, &title, &thpatch::local_layers(&dir), &fonts)
}
