//! Presentation packs: `packs/<id>` (a folder or a .zip) chosen by `game_text` or the `pack`
//! setting, read into the renderer's `PackText` (texts, FileFont, overflow per site) and core's image `Overlay`.
//! Port-only; nothing here reaches logic (logic keys are refused by `Overlay::add`, I18N §2.1).

use std::collections::HashMap;
use std::sync::Arc;

use th08_core::content::Overlay;
use th08_core::drawlist::TextSite;
use th08_render::text::Overflow;
use th08_render::textdb::{PackText, TextDb};

use crate::settings::Settings;

// A pack read and checked: what the renderer and core get, plus the warnings printed at mount.
pub struct Mounted {
    pub id: String,
    pub language: String,
    pub text: PackText,
    pub overlay: Arc<Overlay>,
    pub warnings: Vec<String>,
}

impl Mounted {
    // `Io::overlay`: None when the pack has no images, so a text-only pack leaves `PresentView` untouched.
    pub fn overlay(&self) -> Option<Arc<Overlay>> {
        (!self.overlay.is_empty()).then(|| self.overlay.clone())
    }
}

// The pack this run shows: the `pack` setting (a path, or an id in the pack folders), else the first pack whose
// `language` is the game text language (`game_text`, `follow` = `language`); none for Japanese, the original.
#[cfg(not(target_arch = "wasm32"))]
pub fn select(s: &Settings) -> Option<Mounted> {
    let dirs = pack_dirs(s);
    let path = if let Some(p) = &s.pack {
        let path = std::path::PathBuf::from(p);
        let found = std::iter::once(path)
            .chain(dirs.iter().map(|d| d.join(p)))
            .chain(dirs.iter().map(|d| d.join(format!("{p}.zip"))))
            .find(|d| d.exists());
        if found.is_none() {
            log::warn!("pack {p}: not found in {dirs:?}");
        }
        found?
    } else {
        // Tool runs never follow the system locale: a pack only by an explicit `pack`, `game_text` or `language`.
        if s.hermetic && s.game_text.is_none() && s.language.is_none() {
            return None;
        }
        let lang = game_text(s);
        if lang == "ja" {
            return None;
        }
        let found = find_language(&dirs, &lang);
        if found.is_none() && s.game_text.is_some() {
            log::warn!("game_text {lang}: no such pack installed in {dirs:?}; original text");
        } else if found.is_none() {
            log::info!("no {lang} presentation pack in {dirs:?}; vanilla text");
        }
        found?
    };
    let files = match read_files(&path) {
        Ok(f) => f,
        Err(e) => {
            log::warn!("pack {}: {e:#}", path.display());
            return None;
        }
    };
    match mount(&files, &stem(&path)) {
        Ok(m) => {
            for w in &m.warnings {
                log::warn!("pack {}: {w}", m.id);
            }
            log::info!(
                "pack {} ({}) mounted: {} texts, {} images, {} pack fonts",
                m.id,
                m.language,
                m.text.db.len(),
                m.overlay.len(),
                m.text.fonts.len()
            );
            Some(m)
        }
        Err(e) => {
            log::warn!("pack {}: {e:#}", path.display());
            None
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub fn select(_: &Settings) -> Option<Mounted> {
    None
}

// The game text language code: `game_text`, `follow` = the screen language.
pub fn game_text(s: &Settings) -> String {
    s.game_text
        .clone()
        .unwrap_or_else(|| crate::i18n::resolve(s.language).code().to_string())
}

// An installed pack as the settings screens list it.
#[derive(Clone, Debug, PartialEq)]
pub struct Installed {
    pub id: String,
    pub language: String,
    pub title: String,
    pub revision: String,
    // Translated share of the vanilla slots (0–100) recorded at import.
    pub coverage: Option<f64>,
    pub path: std::path::PathBuf,
    // In `<config folder>/packs`: installed by a settings screen, so it can be updated and removed there.
    pub managed: bool,
}

// Every pack of the pack folders (both, name order); `config` = the config folder.
#[cfg(not(target_arch = "wasm32"))]
pub fn installed(
    game_dir: Option<&std::path::Path>,
    config: Option<&std::path::Path>,
) -> Vec<Installed> {
    let mut out = Vec::new();
    for (d, managed) in [(config, true), (game_dir, false)] {
        let Some(d) = d else { continue };
        for p in pack_entries(&d.join("packs")) {
            let Some(m) = read_manifest(&p) else {
                continue;
            };
            let get = |k: &str| {
                m.get(k)
                    .and_then(toml::Value::as_str)
                    .unwrap_or("")
                    .to_string()
            };
            let language = get("language");
            if language.is_empty() {
                continue;
            }
            out.push(Installed {
                id: if get("id").is_empty() {
                    stem(&p)
                } else {
                    get("id")
                },
                language,
                title: get("title"),
                revision: get("revision"),
                coverage: m
                    .get("coverage")
                    .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64))),
                managed: managed && p.is_dir(),
                path: p,
            });
        }
    }
    out
}

// Pack folders and zips of a folder in name order; `.`-names (an install in progress) are skipped.
#[cfg(not(target_arch = "wasm32"))]
fn pack_entries(d: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut entries: Vec<_> = std::fs::read_dir(d)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .map(|e| e.path())
        .collect();
    entries.sort();
    entries
}

// `<config folder>/packs` (downloaded by a settings screen), then `<game folder>/packs` (bundled): the order that
// breaks a revision tie in `find_language`.
#[cfg(not(target_arch = "wasm32"))]
fn pack_dirs(s: &Settings) -> Vec<std::path::PathBuf> {
    s.dirs
        .config
        .iter()
        .chain(s.game_dir.iter())
        .map(|d| d.join("packs"))
        .collect()
}

#[cfg(not(target_arch = "wasm32"))]
fn stem(p: &std::path::Path) -> String {
    p.file_stem()
        .map_or_else(String::new, |s| s.to_string_lossy().into_owned())
}

// The newest pack (`Freshness`) whose pack.toml says `language = <code>`; on a tie the earlier folder, then name order.
#[cfg(not(target_arch = "wasm32"))]
fn find_language(dirs: &[std::path::PathBuf], code: &str) -> Option<std::path::PathBuf> {
    let mut best: Option<(Freshness, std::path::PathBuf)> = None;
    for d in dirs {
        for p in pack_entries(d) {
            let Some(manifest) = read_manifest(&p) else {
                continue;
            };
            if manifest.get("language").and_then(toml::Value::as_str) != Some(code) {
                continue;
            }
            let f = Freshness::of(&manifest);
            if best.as_ref().is_none_or(|(b, _)| f.newer_than(b)) {
                best = Some((f, p));
            }
        }
    }
    best.map(|b| b.1)
}

// How new a pack is: `revision` (a thpatch revision is a hash, so it only tells equal from different) and the date
// in `source` (`thpatch lang_en retrieved 2026-11-04 16:32 UTC <rev>`).
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Debug, Default, PartialEq)]
struct Freshness {
    revision: String,
    date: String,
}

#[cfg(not(target_arch = "wasm32"))]
impl Freshness {
    fn of(m: &toml::Table) -> Self {
        let get = |k: &str| m.get(k).and_then(toml::Value::as_str).unwrap_or("");
        Self {
            revision: get("revision").to_string(),
            date: source_date(get("source")),
        }
    }

    // Strictly newer: a different revision with a later date, or with the same date a higher numeric revision.
    fn newer_than(&self, o: &Self) -> bool {
        if self.revision == o.revision {
            return false;
        }
        match self.date.cmp(&o.date) {
            std::cmp::Ordering::Equal => {
                matches!((self.revision.parse::<u64>(), o.revision.parse::<u64>()), (Ok(a), Ok(b)) if a > b)
            }
            c => c.is_gt(),
        }
    }
}

// The first `YYYY-MM-DD` (with a following ` HH:MM`) in a `source` line; "" = none.
#[cfg(not(target_arch = "wasm32"))]
fn source_date(s: &str) -> String {
    let b = s.as_bytes();
    let fits = |i: usize, pat: &[u8]| {
        b.len() >= i + pat.len()
            && pat.iter().zip(&b[i..]).all(|(&p, &c)| {
                if p == b'9' {
                    c.is_ascii_digit()
                } else {
                    p == c
                }
            })
    };
    (0..b.len())
        .find(|&i| fits(i, b"9999-99-99"))
        .map_or_else(String::new, |i| {
            let end = if fits(i + 10, b" 99:99") {
                i + 16
            } else {
                i + 10
            };
            s[i..end].to_string()
        })
}

#[cfg(not(target_arch = "wasm32"))]
fn read_manifest(p: &std::path::Path) -> Option<toml::Table> {
    let text = if p.is_dir() {
        std::fs::read_to_string(p.join("pack.toml")).ok()?
    } else {
        let mut z = zip::ZipArchive::new(std::fs::File::open(p).ok()?).ok()?;
        let mut f = z.by_name("pack.toml").ok()?;
        let mut s = String::new();
        std::io::Read::read_to_string(&mut f, &mut s).ok()?;
        s
    };
    text.parse().ok()
}

// Every file of a pack folder (recursively) or zip, as (relative path with '/', bytes).
#[cfg(not(target_arch = "wasm32"))]
pub fn read_files(p: &std::path::Path) -> anyhow::Result<Vec<(String, Vec<u8>)>> {
    use std::io::Read;
    let mut out = Vec::new();
    if p.is_dir() {
        let mut stack = vec![p.to_path_buf()];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d)? {
                let path = e?.path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    let rel = path.strip_prefix(p)?.to_string_lossy().replace('\\', "/");
                    out.push((rel, std::fs::read(&path)?));
                }
            }
        }
    } else {
        let mut z = zip::ZipArchive::new(std::fs::File::open(p)?)?;
        for i in 0..z.len() {
            let mut f = z.by_index(i)?;
            if f.is_dir() {
                continue;
            }
            let name = f.name()?.replace('\\', "/");
            let mut b = Vec::new();
            f.read_to_end(&mut b)?;
            out.push((name, b));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

// Pack layout (I18N §2.5): `pack.toml`; `text/*.json` = flat `{"<TextRef key>": "<text>"}` objects; `fonts/*.ttf|otf`;
// `images/<chunk image name>` and `files/<name>` textures; ATTRIBUTION / LICENSE / README files are skipped.
pub fn mount(files: &[(String, Vec<u8>)], fallback_id: &str) -> anyhow::Result<Mounted> {
    let manifest: toml::Table = match files.iter().find(|f| f.0.eq_ignore_ascii_case("pack.toml")) {
        Some((_, b)) => std::str::from_utf8(b)?.parse()?,
        None => anyhow::bail!("no pack.toml"),
    };
    let get = |k: &str| manifest.get(k).and_then(toml::Value::as_str);
    let class = get("class").unwrap_or("presentation");
    anyhow::ensure!(
        class == "presentation",
        "class = {class:?}: only presentation packs are mounted"
    );
    let mut warnings = Vec::new();
    let mut overflow = HashMap::new();
    if let Some(t) = manifest.get("targets").and_then(toml::Value::as_table) {
        for (k, v) in t {
            let site = TextSite::ALL.into_iter().find(|s| s.name() == k);
            let mode = v.as_str().and_then(Overflow::from_name);
            match (site, mode) {
                (Some(s), Some(m)) => {
                    overflow.insert(s, m);
                }
                _ => warnings.push(format!("[targets] {k} = {v}: unknown site or mode")),
            }
        }
    }
    // `[fonts] text` = the first font of the chain, `fallback` = the next ones in order (I18N G3).
    let fonts_t = manifest.get("fonts").and_then(toml::Value::as_table);
    let mut chain_names: Vec<String> = fonts_t
        .and_then(|t| t.get("text"))
        .and_then(toml::Value::as_str)
        .map(str::to_ascii_lowercase)
        .into_iter()
        .collect();
    chain_names.extend(
        fonts_t
            .and_then(|t| t.get("fallback"))
            .and_then(toml::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(toml::Value::as_str)
            .map(str::to_ascii_lowercase),
    );
    let mut db = TextDb::new();
    let mut overlay = Overlay::default();
    let mut fonts: Vec<(String, Vec<u8>)> = Vec::new();
    for (path, bytes) in files {
        let lower = path.to_ascii_lowercase();
        let base = lower.rsplit('/').next().unwrap_or(&lower);
        if lower == "pack.toml"
            || ["attribution", "license", "readme", "copying"]
                .iter()
                .any(|m| base.starts_with(m))
        {
            continue;
        }
        if let Some(rest) = lower.strip_prefix("text/") {
            if !rest.ends_with(".json") {
                warnings.push(format!("{path}: not a .json text table, ignored"));
                continue;
            }
            match serde_json::from_slice::<serde_json::Map<String, serde_json::Value>>(bytes) {
                Ok(map) => {
                    for (k, v) in map {
                        match v {
                            serde_json::Value::String(t) => db.insert(k, t),
                            _ => warnings.push(format!("{path}: {k}: not a string")),
                        }
                    }
                }
                Err(e) => warnings.push(format!("{path}: {e}")),
            }
        } else if lower.starts_with("fonts/") {
            if base.ends_with(".ttf") || base.ends_with(".otf") || base.ends_with(".ttc") {
                fonts.push((lower.clone(), bytes.clone()));
            } else {
                warnings.push(format!("{path}: not a font file, ignored"));
            }
        } else if let Err(e) = overlay.add(path, bytes.clone()) {
            warnings.push(e.to_string());
        }
    }
    let font_list = if chain_names.is_empty() {
        fonts.first().map(|f| f.1.clone()).into_iter().collect()
    } else {
        let mut v = Vec::new();
        for n in &chain_names {
            match fonts.iter().find(|f| &f.0 == n) {
                Some(f) => v.push(f.1.clone()),
                None => warnings.push(format!("[fonts] {n:?}: no such file")),
            }
        }
        v
    };
    Ok(Mounted {
        id: get("id").unwrap_or(fallback_id).to_string(),
        language: get("language").unwrap_or("").to_string(),
        text: PackText {
            db,
            fonts: font_list,
            overflow,
        },
        overlay: Arc::new(overlay),
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(p: &str, b: &[u8]) -> (String, Vec<u8>) {
        (p.to_string(), b.to_vec())
    }

    // Same language in both pack folders: the newer one wins, a tie goes to the config folder.
    #[test]
    fn newest_pack_wins() {
        let root = std::env::temp_dir().join(format!("th08-packs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let pack = |dir: &str, name: &str, rev: &str, date: &str| {
            let d = root.join(dir).join("packs").join(name);
            std::fs::create_dir_all(&d).unwrap();
            let toml = format!(
                "language = \"en\"\nsource = \"thpatch lang_en retrieved {date} UTC {rev}\"\nrevision = \"{rev}\"\n"
            );
            std::fs::write(d.join("pack.toml"), toml).unwrap();
            root.join(dir).join("packs").join(name)
        };
        let dirs = [root.join("cfg/packs"), root.join("game/packs")];
        let old_cfg = pack("cfg", "lang_en", "aaaa", "2026-01-02 10:00");
        let new_game = pack("game", "lang_en", "bbbb", "2026-11-04 16:32");
        assert_eq!(find_language(&dirs, "en"), Some(new_game));
        pack("game", "lang_en", "aaaa", "2026-01-02 10:00");
        assert_eq!(find_language(&dirs, "en"), Some(old_cfg.clone()));
        pack("game", "lang_en", "cccc", "2026-01-02 10:00");
        assert_eq!(find_language(&dirs, "en"), Some(old_cfg));
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(
            source_date("thpatch x retrieved 2026-11-04 16:32 UTC 84e8"),
            "2026-11-04 16:32"
        );
        assert_eq!(source_date("files dated up to 2026-11-04"), "2026-11-04");
        assert_eq!(source_date("none"), "");
        let num = |r: &str| Freshness {
            revision: r.into(),
            date: String::new(),
        };
        assert!(num("3").newer_than(&num("2")) && !num("2").newer_than(&num("3")));
    }

    // Texts and targets are read; logic keys and non-textures are refused with a warning; nothing else breaks.
    #[test]
    fn mount_rejects_logic_keys() {
        let files = vec![
            f(
                "pack.toml",
                b"id = \"t\"\nlanguage = \"en\"\n[targets]\ndialogue = \"wrap\"\nbogus = \"clip\"\n",
            ),
            f("ATTRIBUTION.txt", b"x"),
            f("text/msg.json", br#"{"msg/msg1a.dat/0/0": "Hello", "spell/0/name": 3}"#),
            f("files/ecldata1.ecl", b"evil"),
            f("msg1a.dat", b"evil"),
            f("images/text/stg1txt.anm", b"evil"),
            f("files/se_ok00.wav", b"RIFF"),
            f("images/data/text/stg1txt.png", b"png"),
            f("files/th08logo.jpg", b"jpg"),
        ];
        let m = mount(&files, "fallback").unwrap();
        assert_eq!((m.id.as_str(), m.language.as_str()), ("t", "en"));
        assert_eq!(m.text.db.len(), 1);
        assert_eq!(m.text.overflow(TextSite::Dialogue), Overflow::Wrap);
        assert_eq!(m.text.overflow(TextSite::Spell), Overflow::Shrink);
        assert_eq!(m.overlay.len(), 2);
        let w = m.warnings.join("\n");
        for k in ["ecldata1.ecl", "msg1a.dat", "stg1txt.anm"] {
            assert!(w.contains(&format!("{k}: logic key")), "{w}");
        }
        assert!(w.contains("se_ok00.wav: not a texture"), "{w}");
        assert!(w.contains("bogus"), "{w}");
        assert!(w.contains("spell/0/name: not a string"), "{w}");
    }

    // `[fonts] text` then `fallback` in order; a missing file is a warning.
    #[test]
    fn mount_font_chain() {
        let files = vec![
            f(
                "pack.toml",
                b"[fonts]\ntext = \"fonts/B.ttf\"\nfallback = [\"fonts/a.otf\", \"fonts/none.ttf\"]\n",
            ),
            f("fonts/a.otf", b"A"),
            f("fonts/B.ttf", b"B"),
        ];
        let m = mount(&files, "x").unwrap();
        assert_eq!(m.text.fonts, [b"B".to_vec(), b"A".to_vec()]);
        assert!(
            m.warnings.join("\n").contains("none.ttf"),
            "{:?}",
            m.warnings
        );
    }

    #[test]
    fn mount_refuses_logic_class() {
        let files = vec![f("pack.toml", b"class = \"logic\"\n")];
        assert!(mount(&files, "x").is_err());
        assert!(mount(&[], "x").is_err());
    }
}
