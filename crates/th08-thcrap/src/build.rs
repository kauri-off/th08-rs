//! Builds a presentation pack from a fetched or local thpatch language: text tables, images, font chain,
//! pack.toml, ATTRIBUTION.md and the import report.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::Progress;
use crate::fontchain::{self, FontInfo};
use crate::thcrap::{self, Lang, Report, Vanilla};
use crate::thpatch::{self, Fonts, LICENCE, LICENCE_URL};

// Where a patch came from (ATTRIBUTION.md, pack.toml).
pub struct Source {
    pub patch: String,
    pub url: String,
    pub date: String,
    pub revision: String,
}

// SOURCE.txt of a cache folder, else (a local thcrap folder) the newest file time; revision = hash of files.js.
pub fn source_of(dir: &Path, patch: &str) -> Source {
    let revision = crate::vfs::read(dir.join("files.js"))
        .map_or_else(|_| "unknown".to_string(), |b| thpatch::revision(&b));
    if let Ok(s) = crate::vfs::read_to_string(dir.join("SOURCE.txt")) {
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
        for p in crate::vfs::list(&d).into_iter().flatten() {
            if crate::vfs::is_dir(&p) {
                stack.push(p);
            } else if let Some(t) = crate::vfs::modified(&p) {
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
    #[cfg(not(target_arch = "wasm32"))]
    let sys = th08_render::text::Raster::new(None, Vec::new());
    #[cfg(not(target_arch = "wasm32"))]
    let system_has = |c: char| sys.has_glyph(c).1 == Some(true);
    // The browser draws what the pack fonts lack with its own fonts, which cannot be asked here.
    #[cfg(target_arch = "wasm32")]
    let system_has = |_: char| true;
    let mut per: std::collections::BTreeMap<&str, (usize, usize, usize)> =
        std::collections::BTreeMap::new();
    let mut none = Vec::new();
    for &c in chars {
        let s = fontchain::script_of(c);
        let e = per.entry(s).or_default();
        if chain.covered.get(&c).copied().flatten().is_some() {
            e.0 += 1;
        } else if system_has(c) {
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
         - Pack built: {today}, by the th08 port's thcrap importer, for personal use\n\
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
    if crate::vfs::exists(dir) && !crate::vfs::list(dir)?.is_empty() {
        if !(force && crate::vfs::is_file(&dir.join("pack.toml"))) {
            bail!(
                "{}: not empty (--force replaces an existing pack)",
                dir.display()
            );
        }
        crate::vfs::remove_dir_all(dir)?;
    }
    crate::vfs::create_dir_all(dir)?;
    Ok(())
}

fn write(dir: &Path, rel: &str, b: &[u8]) -> Result<()> {
    let p = dir.join(rel);
    if let Some(d) = p.parent() {
        crate::vfs::create_dir_all(d)?;
    }
    crate::vfs::write(&p, b).with_context(|| format!("write {}", p.display()))
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

#[derive(Clone, Copy, Debug, Default)]
pub struct Options {
    pub no_images: bool,
    // Replace an existing pack (a folder with pack.toml) at `out`.
    pub force: bool,
}

// What a build wrote.
pub struct Summary {
    pub out: PathBuf,
    pub texts: usize,
    pub images: usize,
    pub fonts: usize,
    // Translated share of all vanilla slots, 0–100.
    pub percent: f64,
    pub coverage: String,
    pub font_text: String,
    pub report: Report,
}

// Pack fonts without an open licence named in the font (see `FontInfo::open_licence`): personal use only.
fn personal_fonts(names: &[String], infos: &[&FontInfo]) -> Vec<String> {
    names
        .iter()
        .zip(infos)
        .filter(|(_, f)| !f.open_licence())
        .map(|(n, _)| n.clone())
        .collect()
}

fn quoted(v: &[String]) -> String {
    v.iter()
        .map(|n| format!("\"{n}\""))
        .collect::<Vec<_>>()
        .join(", ")
}

// `patch` = `lang_xx`; `layers` = its text folders, base languages first (the last one is the language's own).
#[expect(clippy::too_many_arguments, reason = "one build, every input named")]
pub fn build_pack(
    patch: &str,
    title: &str,
    layers: &[PathBuf],
    fonts: &Fonts,
    v: &Vanilla,
    out: &Path,
    o: Options,
    p: &dyn Progress,
) -> Result<Summary> {
    let code = patch.strip_prefix("lang_").unwrap_or(patch);
    let lang_dir = layers.last().context("no text layer")?;
    let src = source_of(lang_dir, patch);
    p.line(&format!(
        "source {} ({}, revision {}); {LICENCE}, {LICENCE_URL}",
        src.url, src.date, src.revision
    ));
    if layers.len() > 1 {
        let base: Vec<String> = layers[..layers.len() - 1]
            .iter()
            .map(|l| {
                l.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        p.line(&format!("on top of {}", base.join(", ")));
    }
    let lang = Lang::read_layers(layers)?;
    let mut report = Report::default();
    let tables = thcrap::import(&lang, v, &mut report);
    let per = thcrap::coverage(|k| tables.values().any(|t| t.contains_key(k)), v);
    let (have, all) = per.values().fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
    let percent = if all == 0 {
        0.0
    } else {
        100.0 * have as f64 / all as f64
    };
    let coverage = thcrap::coverage_text(&per);
    let images = if o.no_images {
        Vec::new()
    } else {
        lang.images(&mut report)
    };
    let chars: Vec<char> = tables
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

    prepare_dir(out, o.force)?;
    for (name, t) in &tables {
        let mut s = serde_json::to_string_pretty(t)?;
        s.push('\n');
        write(out, &format!("text/{name}"), s.as_bytes())?;
    }
    for (rel, bytes) in &images {
        write(out, rel, bytes)?;
    }
    let mut names = Vec::new();
    let mut used = Vec::new();
    for (k, &i) in chain.fonts.iter().enumerate() {
        let f = font_file(k, &infos[i].decl.name);
        write(out, &f, &infos[i].bytes)?;
        names.push(f);
        used.push(&infos[i]);
    }
    let personal = personal_fonts(&names, &used);
    let fonts_toml = match names.split_first() {
        Some((first, rest)) => format!(
            "\n[fonts]\ntext = \"{first}\"\nfallback = [{}]\n# No open licence named in the font: personal use only, never in a public build.\npersonal = [{}]\n",
            quoted(rest),
            quoted(&personal)
        ),
        None => String::new(),
    };
    let title = title.replace(['"', '\\', '\n'], " ");
    let toml = format!(
        "# Presentation pack, built by th08 thcrap-import for personal use. See ATTRIBUTION.md.\n\
         id = \"lang_{code}\"\nclass = \"presentation\"\nlanguage = \"{code}\"\ntitle = \"{title}\"\n\
         source = \"thpatch {patch} {date} {rev}\"\nrevision = \"{rev}\"\ncoverage = {percent:.1}\n\
         \n[targets]\ndialogue = \"squeeze\"\nspell = \"squeeze\"\nending = \"squeeze\"\nmusic_room = \"squeeze\"\n\
         menu_list = \"squeeze\"\nmenu_desc = \"shrink\"\n{fonts_toml}",
        date = src.date,
        rev = src.revision,
    );
    write(out, "pack.toml", toml.as_bytes())?;
    write(
        out,
        "ATTRIBUTION.md",
        attribution(&src, &fr.attribution).as_bytes(),
    )?;
    let full = format!(
        "coverage of the vanilla slots:\n{coverage}\n{}\n{}",
        fr.text,
        report.render(usize::MAX)
    );
    write(out, "README.import.txt", full.as_bytes())?;
    Ok(Summary {
        out: out.to_path_buf(),
        texts: tables.values().map(std::collections::BTreeMap::len).sum(),
        images: images.len(),
        fonts: names.len(),
        percent,
        coverage,
        font_text: fr.text,
        report,
    })
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
