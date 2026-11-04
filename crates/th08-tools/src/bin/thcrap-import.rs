//! thcrap-import (I18N G3): a thpatch language → a local presentation pack (I18N §2.5). Sources: `--lang <code>`
//! (downloaded on demand from srv.thpatch.net, cached per revision) or a local thcrap `lang_xx` folder; th08.dat maps
//! the keys. `--list` shows the server's languages with th08 files (`--coverage`: per-language coverage). Nothing it
//! writes belongs in git: cache and packs default to `<target>/thcrap` and `<target>/packs`.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use th08_tools::fontchain::{self, FontInfo};
use th08_tools::thcrap::{self, Lang, Report, Tables, Vanilla};
use th08_tools::thpatch::{self, Client, Fonts, LICENCE, LICENCE_URL};

const USAGE: &str = "usage: thcrap-import --list [--coverage --dat th08.dat] [--cache DIR]
       thcrap-import --lang <code|lang_xx> --dat th08.dat [--cache DIR] [-o PACK_DIR] [--no-images] [--force]
       thcrap-import <local thcrap lang_xx dir> --dat th08.dat [-o PACK_DIR] [--no-images] [--force]";

#[derive(Default)]
struct Args {
    // `--list`; Some(true) = with `--coverage`.
    list: Option<bool>,
    lang: Option<String>,
    local: Option<PathBuf>,
    dat: Option<PathBuf>,
    cache: PathBuf,
    out: Option<PathBuf>,
    no_images: bool,
    force: bool,
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
            "--no-images" => a.no_images = true,
            "--force" => a.force = true,
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

// Where a patch came from (ATTRIBUTION.md, pack.toml).
struct Source {
    patch: String,
    url: String,
    date: String,
    revision: String,
}

// SOURCE.txt of a cache folder, else (a local thcrap folder) the newest file time; revision = hash of files.js.
fn source_of(dir: &Path, patch: &str) -> Source {
    let revision = std::fs::read(dir.join("files.js"))
        .map_or_else(|_| "unknown".to_string(), |b| thpatch::revision(&b));
    if let Ok(s) = std::fs::read_to_string(dir.join("SOURCE.txt")) {
        let field = |k: &str| {
            s.lines()
                .find_map(|l| l.strip_prefix(k).map(|v| v.trim().to_string()))
                .unwrap_or_default()
        };
        return Source {
            patch: patch.to_string(),
            url: field("url "),
            date: format!("retrieved {}", field("retrieved ")),
            revision,
        };
    }
    let mut newest = std::time::SystemTime::UNIX_EPOCH;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if let Ok(t) = e.metadata().and_then(|m| m.modified()) {
                newest = newest.max(t);
            }
        }
    }
    let date = jiff::Timestamp::try_from(newest).map_or_else(
        |_| "unknown".to_string(),
        |t| t.strftime("%Y-%m-%d").to_string(),
    );
    Source {
        patch: patch.to_string(),
        url: format!(
            "{}{patch}/ (local thcrap copy {})",
            thpatch::SERVER,
            dir.display()
        ),
        date: format!("files dated up to {date}"),
        revision,
    }
}

// The font part of the report and of ATTRIBUTION.md.
struct FontReport {
    text: String,
    attribution: String,
}

fn font_report(
    fonts: &Fonts,
    infos: &[FontInfo],
    bad: &[String],
    chain: &fontchain::Chain,
    chars: &[char],
) -> Result<FontReport> {
    let mut t = String::new();
    let mut at = String::new();
    match &fonts.face {
        Some((p, f)) => writeln!(t, "th08 face: {f:?} (th08.js of {p})")?,
        None => writeln!(t, "th08 face: none declared (th08.js)")?,
    }
    writeln!(t, "declared fonts: {}", fonts.decls.len())?;
    for d in &fonts.decls {
        let info = infos
            .iter()
            .position(|i| i.decl.name == d.name && i.decl.patch == d.patch);
        let chain_pos = info.and_then(|i| chain.fonts.iter().position(|&c| c == i));
        let lic = info.map_or(String::new(), |i| {
            let f = &infos[i];
            format!(
                " | {} | {}{}",
                f.copyright,
                if f.licence.is_empty() {
                    "no licence field"
                } else {
                    &f.licence
                },
                if f.open_licence() {
                    ""
                } else {
                    " | no open licence named in the font"
                }
            )
        });
        let pos = chain_pos.map_or(String::new(), |k| {
            format!(" | chain #{k}: {}", chain.reason[k])
        });
        writeln!(t, "  {} {} : {}{pos}{lic}", d.patch, d.name, d.status)?;
    }
    for b in bad {
        writeln!(t, "  {b}")?;
    }
    // Per script: characters from the chain, from the system font only, from no font.
    let sys = th08_render::text::Raster::new(None, Vec::new());
    let mut per: std::collections::BTreeMap<&str, (usize, usize, usize)> =
        std::collections::BTreeMap::new();
    let mut none = Vec::new();
    for &c in chars {
        let s = fontchain::script_of(c);
        let e = per.entry(s).or_default();
        if chain.covered.get(&c).copied().flatten().is_some() {
            e.0 += 1;
        } else if sys.has_glyph(c).1 == Some(true) {
            e.1 += 1;
        } else {
            e.2 += 1;
            none.push(c);
        }
    }
    writeln!(
        t,
        "scripts (characters: pack fonts / system font only / no font):"
    )?;
    for (s, (a, b, c)) in &per {
        let note = if fontchain::rtl(s) {
            "  (right-to-left: not laid out, out of scope)"
        } else if *c > 0 {
            "  (lacks a font)"
        } else {
            ""
        };
        writeln!(t, "  {s:<10} {a:>4} / {b:>4} / {c:>4}{note}")?;
    }
    if !none.is_empty() {
        writeln!(t, "  no font: {}", none.iter().collect::<String>())?;
    }
    writeln!(at, "## Fonts\n")?;
    writeln!(
        at,
        "The pack's fonts are the translation's own, as declared by the patch (`patch.js` `fonts`, `th08.js` `font`);\n\
         they were downloaded by you from the patch server and are not distributed by the th08 port.\n"
    )?;
    for (k, &i) in chain.fonts.iter().enumerate() {
        let f = &infos[i];
        writeln!(
            at,
            "- {} — {} (`{}`): {}; {}.{}",
            k,
            f.decl.name,
            f.decl.patch,
            if f.copyright.is_empty() {
                "no copyright field"
            } else {
                &f.copyright
            },
            if f.licence.is_empty() {
                "no licence field"
            } else {
                &f.licence
            },
            if f.open_licence() {
                ""
            } else {
                " **No open licence named in the font**: usable from your own download only, never bundled into a \
                 public build."
            }
        )?;
    }
    if chain.fonts.is_empty() {
        writeln!(at, "- none: the system font is used.")?;
    }
    let rest: Vec<&FontInfo> = (0..infos.len())
        .filter(|i| !chain.fonts.contains(i))
        .map(|i| &infos[i])
        .collect();
    if !rest.is_empty() {
        writeln!(
            at,
            "\nAlso declared by the patch and downloaded into your cache, not used by this pack (no glyph the chain lacks):\n"
        )?;
        for f in rest {
            writeln!(
                at,
                "- {} (`{}`): {}; {}.{}",
                f.decl.name,
                f.decl.patch,
                if f.copyright.is_empty() {
                    "no copyright field"
                } else {
                    &f.copyright
                },
                if f.licence.is_empty() {
                    "no licence field"
                } else {
                    &f.licence
                },
                if f.open_licence() {
                    ""
                } else {
                    " No open licence named in the font (MyriadPro-Cond, for one, is a commercial Adobe font): \
                     personal use from your own download, never bundled into a public build."
                }
            )?;
        }
    }
    Ok(FontReport {
        text: t,
        attribution: at,
    })
}

fn attribution(s: &Source, fonts: &str) -> String {
    let today = jiff::Timestamp::now().strftime("%Y-%m-%d");
    format!(
        "# Attribution\n\n\
         The texts and edited images in this pack are translations by **Touhou Patch Center**\n\
         (<https://www.thpatch.net>) and its contributors. The contributors of each text are listed in the history of\n\
         its wiki page under <https://www.thpatch.net/wiki/Th08> and of the wiki's shared string definition pages.\n\n\
         - Patch: `{patch}`, {url}\n\
         - Source date: {date}\n\
         - Source revision: `{rev}` (SHA-256 prefix of the patch's `files.js`, thcrap's per-file CRC32 list)\n\
         - Pack built: {today}, by `th08-tools thcrap-import`, for personal use\n\
         - Licence: {LICENCE}, <{LICENCE_URL}>\n\n\
         Changes made: converted from thcrap patch files into flat JSON tables keyed by the port's text keys; thcrap\n\
         layout markup flattened; lines refitted to the original line slots (extra lines joined into the last slot,\n\
         missing lines blanked); `%%` unescaped outside printf formats; printf formats whose conversions differ from\n\
         the original left out (the original is shown there). Untranslated slots show the original Japanese text.\n\
         This adaptation is under the same licence. The underlying game text and images are by Team Shanghai Alice\n\
         (ZUN); this is a fan translation.\n\n{fonts}",
        patch = s.patch,
        url = s.url,
        date = s.date,
        rev = s.revision,
    )
}

// Empty, or a previous pack (pack.toml present) that `--force` may replace.
fn prepare_dir(dir: &Path, force: bool) -> Result<()> {
    if dir.exists() && std::fs::read_dir(dir)?.next().is_some() {
        if !(force && dir.join("pack.toml").is_file()) {
            bail!(
                "{}: not empty (--force replaces an existing pack)",
                dir.display()
            );
        }
        std::fs::remove_dir_all(dir)?;
    }
    std::fs::create_dir_all(dir)?;
    Ok(())
}

fn write(dir: &Path, rel: &str, b: &[u8]) -> Result<()> {
    let p = dir.join(rel);
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(&p, b).with_context(|| format!("write {}", p.display()))
}

// `fonts/<k>-<name>` with [A-Za-z0-9._-] only.
fn font_file(k: usize, name: &str) -> String {
    let base = name.rsplit('/').next().unwrap_or(name);
    let clean: String = base
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "._-".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("fonts/{k}-{clean}")
}

struct Built {
    tables: Tables,
    report: Report,
    coverage: String,
}

fn import_tables(lang_dir: &Path, v: &Vanilla) -> Result<Built> {
    let lang = Lang::read(lang_dir)?;
    let mut report = Report::default();
    let tables = thcrap::import(&lang, v, &mut report);
    let cov = thcrap::coverage(|k| tables.values().any(|t| t.contains_key(k)), v);
    Ok(Built {
        tables,
        report,
        coverage: thcrap::coverage_text(&cov),
    })
}

fn build_pack(a: &Args, patch: &str, lang_dir: &Path, fonts: &Fonts) -> Result<()> {
    let code = patch.strip_prefix("lang_").unwrap_or(patch);
    let out = a
        .out
        .clone()
        .unwrap_or_else(|| target().join("packs").join(format!("lang_{code}")));
    let src = source_of(lang_dir, patch);
    println!(
        "source {} ({}, revision {}); {LICENCE}, {LICENCE_URL}",
        src.url, src.date, src.revision
    );
    let v = Vanilla::from_dat(a.dat.as_deref().context("--dat")?)?;
    let mut b = import_tables(lang_dir, &v)?;
    let images = if a.no_images {
        Vec::new()
    } else {
        thcrap::images(lang_dir, &mut b.report)
    };
    let chars: Vec<char> = b
        .tables
        .values()
        .flat_map(|t| t.values())
        .flat_map(|s| s.chars())
        .filter(|c| !c.is_whitespace() && !c.is_control())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let (infos, bad) = fontchain::load(&fonts.decls);
    let chain = fontchain::choose(&infos, fonts.face.as_ref().map(|f| f.1.as_str()), &chars);
    let fr = font_report(fonts, &infos, &bad, &chain, &chars)?;

    prepare_dir(&out, a.force)?;
    for (name, t) in &b.tables {
        let mut s = serde_json::to_string_pretty(t)?;
        s.push('\n');
        write(&out, &format!("text/{name}"), s.as_bytes())?;
    }
    for (rel, bytes) in &images {
        write(&out, rel, bytes)?;
    }
    let mut names = Vec::new();
    for (k, &i) in chain.fonts.iter().enumerate() {
        let f = font_file(k, &infos[i].decl.name);
        write(&out, &f, &infos[i].bytes)?;
        names.push(f);
    }
    let fonts_toml = match names.split_first() {
        Some((first, rest)) => format!(
            "\n[fonts]\ntext = \"{first}\"\nfallback = [{}]\n",
            rest.iter()
                .map(|n| format!("\"{n}\""))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        None => String::new(),
    };
    let toml = format!(
        "# Presentation pack (I18N §2.5), built by th08-tools thcrap-import for personal use. See ATTRIBUTION.md.\n\
         id = \"lang_{code}\"\nclass = \"presentation\"\nlanguage = \"{code}\"\nsource = \"thpatch {patch} {date} {rev}\"\n\
         \n[targets]\ndialogue = \"squeeze\"\nspell = \"squeeze\"\nending = \"squeeze\"\nmusic_room = \"squeeze\"\n\
         menu_list = \"squeeze\"\nmenu_desc = \"shrink\"\n{fonts_toml}",
        date = src.date,
        rev = src.revision,
    );
    write(&out, "pack.toml", toml.as_bytes())?;
    write(
        &out,
        "ATTRIBUTION.md",
        attribution(&src, &fr.attribution).as_bytes(),
    )?;
    let full = format!(
        "coverage of the vanilla slots:\n{}\n{}\n{}",
        b.coverage,
        fr.text,
        b.report.render(usize::MAX)
    );
    write(&out, "README.import.txt", full.as_bytes())?;
    let n: usize = b.tables.values().map(std::collections::BTreeMap::len).sum();
    println!(
        "{}: {n} texts, {} images, {} pack fonts\ncoverage:\n{}{}",
        out.display(),
        images.len(),
        names.len(),
        b.coverage,
        fr.text
    );
    print!("{}", b.report.render(3));
    Ok(())
}

// `--list`: every server language with its th08 files; `--coverage` imports each (text only, cached) for its share.
fn list(a: &Args) -> Result<()> {
    let c = Client::new();
    let langs = c.languages()?;
    let rows = std::sync::Mutex::new(Vec::new());
    let next = std::sync::atomic::AtomicUsize::new(0);
    std::thread::scope(|s| {
        for _ in 0..8 {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some((id, title)) = langs.get(i) else {
                        break;
                    };
                    let files = thpatch::files_js(&c, id).and_then(|b| thpatch::file_list(&b));
                    rows.lock()
                        .unwrap()
                        .push((id.clone(), title.clone(), files));
                }
            });
        }
    });
    let mut rows = rows.into_inner().unwrap();
    rows.sort_by(|x, y| x.0.cmp(&y.0));
    let v = if a.list == Some(true) {
        Some(Vanilla::from_dat(a.dat.as_deref().context("--dat")?)?)
    } else {
        None
    };
    let mut without = Vec::new();
    println!(
        "{:<20} {:<40} msg   end  sp cm mu img  coverage",
        "language", "title"
    );
    for (id, title, files) in rows {
        let Ok(files) = files else {
            without.push(format!("{id} (no files.js)"));
            continue;
        };
        let has = |n: &str| files.contains_key(&format!("th08/{n}"));
        let msg = thcrap_count(&files, ".dat.jdiff");
        let end = thcrap_count(&files, ".end.jdiff");
        let img = files.keys().filter(|n| thpatch::is_image(n)).count();
        let flags = [
            has("spells.js"),
            has("spellcomments.js"),
            has("musiccmt.js"),
        ];
        if msg + end + img == 0 && !flags.iter().any(|&f| f) {
            without.push(id);
            continue;
        }
        let cov = match &v {
            Some(v) => {
                coverage_of(&c, &id, &a.cache, v).unwrap_or_else(|e| format!("error: {e:#}"))
            }
            None => String::new(),
        };
        let f = |b: bool| if b { " y" } else { " -" };
        let title: String = title.chars().take(40).collect();
        println!(
            "{id:<20} {title:<40} {msg:>2}/32 {end:>2}/12 {}{}{} {img:>3}  {cov}",
            f(flags[0]),
            f(flags[1]),
            f(flags[2])
        );
    }
    println!("no th08 files: {}", without.join(", "));
    Ok(())
}

fn thcrap_count(files: &std::collections::BTreeMap<String, u32>, suffix: &str) -> usize {
    files
        .keys()
        .filter(|n| n.starts_with("th08/") && n.ends_with(suffix))
        .count()
}

// Overall and per-kind coverage of one language, from a text-only cached download.
fn coverage_of(c: &Client, id: &str, cache: &Path, v: &Vanilla) -> Result<String> {
    let f = thpatch::fetch(c, id, cache, false)?;
    let lang = Lang::read(&f.dir)?;
    let mut report = Report::default();
    let tables = thcrap::import(&lang, v, &mut report);
    let per = thcrap::coverage(|k| tables.values().any(|t| t.contains_key(k)), v);
    let (have, n) = per.values().fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
    let pct = |a: usize, b: usize| {
        if b == 0 {
            0.0
        } else {
            100.0 * a as f64 / b as f64
        }
    };
    let kinds: Vec<String> = per
        .iter()
        .map(|(k, (a, b))| format!("{k} {:.0}%", pct(*a, *b)))
        .collect();
    Ok(format!("{:5.1}%  ({})", pct(have, n), kinds.join(", ")))
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
        let f = thpatch::fetch(&c, &id, &a.cache, true)?;
        return build_pack(&a, &id, &f.dir, &f.fonts);
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
    build_pack(&a, &patch, &dir, &fonts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_file_names() {
        assert_eq!(
            font_file(1, "th08/PT Sans-Narrow.ttf"),
            "fonts/1-PT_Sans-Narrow.ttf"
        );
    }
}
