//! pack-check: a presentation pack against its font, the translated text layout and the vanilla key set — glyphs
//! the pack font lacks, per-site overflow (`Raster::render_layout` rules), unknown and unmapped keys.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use anyhow::Result;
use th08_core::drawlist::TextSite;
use th08_render::text::{MIN_FIT_SCALE, Overflow, Raster};
use th08_render::textdb::PackText;

use th08_thcrap::thcrap::{self, Vanilla};

// Strip font size of every text caller (10 §6: font byte 0 → 15; dialogue font_size = 15).
const SIZE: usize = 15;

#[derive(Default)]
pub struct Summary {
    pub text: String,
    // Unknown keys, mount warnings or glyphs no font has.
    pub errors: usize,
}

// A builtin printf format with sample arguments (`%d` → zeros of its width, `%s` → a 12-letter word), for widths.
fn sample(s: &str) -> String {
    let Ok(specs) = thcrap::printf_specs(s) else {
        return s.to_string();
    };
    let mut out = s.replace("%%", "%");
    for sp in specs {
        let w: usize = sp
            .trim_start_matches('%')
            .trim_start_matches(['-', '0'])
            .split(['.', 'd', 's'])
            .next()
            .and_then(|n| n.parse().ok())
            .unwrap_or(0);
        let r = if sp.ends_with('d') {
            "0".repeat(w.max(3))
        } else {
            "Mmmmmmmmmmmm".to_string()
        };
        out = out.replacen(&sp, &r, 1);
    }
    out
}

// The texts of a pack as the mount reads them: every `text/*.json` object, later files overriding earlier ones.
fn texts(files: &[(String, Vec<u8>)]) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for (p, b) in files {
        let l = p.to_ascii_lowercase();
        if !(l.starts_with("text/") && l.ends_with(".json")) {
            continue;
        }
        if let Ok(m) = serde_json::from_slice::<serde_json::Map<String, serde_json::Value>>(b) {
            for (k, v) in m {
                if let Some(s) = v.as_str().filter(|s| !s.is_empty()) {
                    out.insert(k, s.to_string());
                }
            }
        }
    }
    out
}

// Capacity of a mode in units of the strip room: shrink / squeeze down to MIN_FIT, wrap = two such lines.
fn capacity(m: Overflow) -> f32 {
    match m {
        Overflow::Clip => 1.0,
        Overflow::Shrink | Overflow::Squeeze => 1.0 / MIN_FIT_SCALE,
        Overflow::Wrap => 2.0 / MIN_FIT_SCALE,
    }
}

// Overflow counts of one site.
#[derive(Default)]
struct Site {
    n: usize,
    over: usize,
    cut: usize,
    worst: Vec<(f32, String)>,
}

pub fn check(
    files: &[(String, Vec<u8>)],
    vanilla: Option<&Vanilla>,
    max: usize,
) -> Result<Summary> {
    let m = th08::packs::mount(files, "pack")?;
    let mut sum = Summary::default();
    let o = &mut sum.text;
    let tx = texts(files);
    writeln!(
        o,
        "pack {} ({}): {} texts, {} images, {} pack fonts",
        m.id,
        m.language,
        tx.len(),
        m.overlay.len(),
        m.text.fonts.len()
    )?;
    writeln!(o, "mount warnings: {}", m.warnings.len())?;
    for w in m.warnings.iter().take(max) {
        writeln!(o, "  {w}")?;
    }
    sum.errors += m.warnings.len();
    let mut r = Raster::new(None, m.text.fonts.clone());
    sum.errors += glyphs(o, &r, &tx, max)?;
    sum.errors += overflow(o, &mut r, &m.text, &tx, max)?;
    if let Some(v) = vanilla {
        sum.errors += coverage(o, v, &tx, max)?;
    }
    Ok(sum)
}

// Characters the pack font lacks (drawn from the system font) and those no font has; returns the latter count.
fn glyphs(o: &mut String, r: &Raster, tx: &BTreeMap<String, String>, max: usize) -> Result<usize> {
    // Per character, the number of texts using it.
    let mut uses: BTreeMap<char, usize> = BTreeMap::new();
    for s in tx.values() {
        for c in s.chars().collect::<BTreeSet<_>>() {
            if !c.is_whitespace() && !c.is_control() {
                *uses.entry(c).or_default() += 1;
            }
        }
    }
    // Against the chain: pack fonts in order, then the system font.
    let (mut no_pack, mut no_any) = (Vec::new(), Vec::new());
    let mut per_font = vec![0usize; r.pack_fonts()];
    for (&c, &n) in &uses {
        match r.has_glyph(c) {
            (Some(k), _) => per_font[k] += 1,
            (None, Some(true)) => no_pack.push((c, n)),
            (None, _) => no_any.push((c, n)),
        }
    }
    let fmt = |v: &[(char, usize)]| -> String {
        v.iter()
            .take(max.max(40))
            .map(|(c, n)| format!("{c} U+{:04X}×{n}", u32::from(*c)))
            .collect::<Vec<_>>()
            .join(", ")
    };
    writeln!(
        o,
        "glyphs: {} distinct; first chain font with the glyph: {per_font:?} (pack fonts in [fonts] order)",
        uses.len()
    )?;
    writeln!(
        o,
        "  missing in every pack font (system fallback): {} {}",
        no_pack.len(),
        fmt(&no_pack)
    )?;
    writeln!(
        o,
        "  missing in every font (.notdef): {} {}",
        no_any.len(),
        fmt(&no_any)
    )?;
    Ok(no_any.len())
}

// Overflow per site against the visible sprite (the room `render_layout` fits to); unknown keys. Returns the
// unknown-key count.
fn overflow(
    o: &mut String,
    r: &mut Raster,
    pack: &PackText,
    tx: &BTreeMap<String, String>,
    max: usize,
) -> Result<usize> {
    let mut sites: BTreeMap<&'static str, Site> = BTreeMap::new();
    let mut unknown = Vec::new();
    for (k, s) in tx {
        if !thcrap::key_known(k) {
            unknown.push(k.clone());
            continue;
        }
        let Some((site, align, edge)) = thcrap::site_of(k) else {
            continue;
        };
        let s = if k.starts_with("builtin/") {
            sample(s)
        } else {
            s.clone()
        };
        let ratio = r.fit_ratio(&s, SIZE, (align, 0.0, edge));
        let e = sites.entry(site.name()).or_default();
        e.n += 1;
        if ratio > 1.0 {
            e.over += 1;
            e.worst.push((ratio, format!("{k} {ratio:.2}: {s}")));
        }
        if ratio > capacity(pack.overflow(site)) {
            e.cut += 1;
        }
    }
    writeln!(
        o,
        "overflow (size {SIZE}, room = visible sprite: 320 dialogue / spell, 512 ending / lists / music / menu):"
    )?;
    for (name, e) in &mut sites {
        let mode = TextSite::ALL
            .into_iter()
            .find(|s| s.name() == *name)
            .map(|s| pack.overflow(s));
        writeln!(
            o,
            "  {name:<10} {:>5} texts, {:>4} over room ({mode:?}), {:>4} cut after fitting",
            e.n, e.over, e.cut
        )?;
        e.worst.sort_by(|a, b| b.0.total_cmp(&a.0));
        for (_, s) in e.worst.iter().take(max) {
            writeln!(o, "      {s}")?;
        }
    }
    writeln!(o, "unknown keys: {}", unknown.len())?;
    for k in unknown.iter().take(max) {
        writeln!(o, "  {k}")?;
    }
    Ok(unknown.len())
}

// Vanilla slots (th08.dat) the pack leaves untranslated (they show the original), with coverage %, and pack keys
// with no vanilla slot (returned count).
fn coverage(
    o: &mut String,
    v: &Vanilla,
    tx: &BTreeMap<String, String>,
    max: usize,
) -> Result<usize> {
    let per = thcrap::coverage(|k| tx.contains_key(k), v);
    let mut all = v.keys();
    for k in thcrap::spell_keys() {
        all.insert(k, String::new());
    }
    let missing: Vec<&String> = all.keys().filter(|k| !tx.contains_key(*k)).collect();
    let stale: Vec<&String> = tx
        .keys()
        .filter(|k| thcrap::key_known(k) && !all.contains_key(*k))
        .collect();
    writeln!(
        o,
        "coverage of the vanilla slots (th08.dat; untranslated slots show the original):"
    )?;
    o.push_str(&thcrap::coverage_text(&per));
    for k in missing.iter().take(max) {
        writeln!(o, "    unmapped {k}: {}", all[*k])?;
    }
    writeln!(o, "keys with no vanilla slot: {}", stale.len())?;
    for k in stale.iter().take(max) {
        writeln!(o, "  {k}")?;
    }
    Ok(stale.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples() {
        assert_eq!(sample("%3d/%3d x"), "000/000 x");
        assert_eq!(sample("No.%s %6d 100%%"), "No.Mmmmmmmmmmmm 000000 100%");
    }

    #[test]
    fn unknown_keys_counted() {
        let files = vec![
            (
                "pack.toml".to_string(),
                b"id = \"t\"\nlanguage = \"en\"\n".to_vec(),
            ),
            (
                "text/a.json".to_string(),
                br#"{"msg/msg1a.dat/0/0": "Hi", "bogus/1": "x", "spell/9/name": "Card"}"#.to_vec(),
            ),
        ];
        let s = check(&files, None, 5).unwrap();
        assert!(s.text.contains("unknown keys: 1"), "{}", s.text);
        assert!(s.text.contains("dialogue"), "{}", s.text);
        assert!(s.errors >= 1);
    }
}
