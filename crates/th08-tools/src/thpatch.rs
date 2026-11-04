//! thpatch patches for th08 (I18N G3, I18N §1.2 server file list): the server's language list, a language patch and
//! its dependency chain (`patch.js` `dependencies`, repos found through `repo.js` neighbours), and the cached download
//! `<cache>/<lang>/<revision>/` of its th08 text, images and declared fonts. Downloads are untrusted data: names are
//! checked, sizes capped, CRC32 checked against `files.js`, full `.end` scripts never fetched, only JSON is parsed.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context, Result};
use serde_json::{Map, Value};

pub const SERVER: &str = "https://srv.thpatch.net/";
pub const LICENCE: &str =
    "Creative Commons Attribution-ShareAlike 4.0 International (CC BY-SA 4.0)";
pub const LICENCE_URL: &str = "https://creativecommons.org/licenses/by-sa/4.0/";
const MAX_FILE: u64 = 32 << 20;
// Parallel requests of one download.
const WORKERS: usize = 8;

// Text inputs of the importer (I18N §1.2): stringdefs + themes at the patch root; jdiff and the three spell/music
// tables under th08/. Full `th08/*.end` replacements are logic keys (I18N §2.1) and are skipped.
pub fn is_text(name: &str) -> bool {
    if !safe_name(name) {
        return false;
    }
    if name == "stringdefs.js" || name == "themes.js" {
        return true;
    }
    let Some(r) = name.strip_prefix("th08/") else {
        return false;
    };
    let l = r.to_ascii_lowercase();
    l.ends_with(".jdiff") || matches!(l.as_str(), "spells.js" | "spellcomments.js" | "musiccmt.js")
}

// Texture replacements under th08/ (I18N §2.4).
pub fn is_image(name: &str) -> bool {
    let l = name.to_ascii_lowercase();
    safe_name(name) && l.starts_with("th08/") && (l.ends_with(".png") || l.ends_with(".jpg"))
}

// A declared font that can serve th08: at the patch root or under th08/ (others belong to other games).
pub fn is_th08_font(name: &str) -> bool {
    let l = name.to_ascii_lowercase();
    let font = l.ends_with(".ttf") || l.ends_with(".otf") || l.ends_with(".ttc");
    font && safe_name(name) && (!name.contains('/') || l.starts_with("th08/"))
}

// Relative path of [A-Za-z0-9 _.-/] with no empty, `.` or `..` component (font names carry spaces).
fn safe_name(n: &str) -> bool {
    !n.is_empty()
        && n.chars()
            .all(|c| c.is_ascii_alphanumeric() || " _.-/".contains(c))
        && n.split('/')
            .all(|c| !c.trim().is_empty() && c != "." && c != "..")
}

// `lang_xx` patch id: `lang_` + [a-z0-9-].
pub fn valid_lang(id: &str) -> bool {
    id.strip_prefix("lang_").is_some_and(|c| {
        !c.is_empty()
            && c.chars()
                .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
    })
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_-".contains(c))
}

// Source revision of a patch: SHA-256 (first 16 hex digits) of its `files.js` (thcrap's per-file CRC32 list), so
// a cache folder and a pack name the exact server state they came from.
pub fn revision(files_js: &[u8]) -> String {
    use sha2::Digest;
    let h = sha2::Sha256::digest(files_js);
    h.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

fn json_object(b: &[u8], what: &str) -> Result<Map<String, Value>> {
    let b = b.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(b);
    match serde_json::from_slice(b).with_context(|| what.to_string())? {
        Value::Object(m) => Ok(m),
        _ => anyhow::bail!("{what}: not a JSON object"),
    }
}

// `files.js`: live file → CRC32 (deleted files are `null` and dropped).
pub fn file_list(files_js: &[u8]) -> Result<BTreeMap<String, u32>> {
    Ok(json_object(files_js, "files.js")?
        .into_iter()
        .filter_map(|(k, v)| Some((k, u32::try_from(v.as_u64()?).ok()?)))
        .collect())
}

// `files.js` of a server language (for `--list`).
pub fn files_js(c: &Client, lang: &str) -> Result<Vec<u8>> {
    anyhow::ensure!(valid_lang(lang), "{lang}: not a lang_xx patch id");
    c.get(&format!("{SERVER}{lang}/files.js"))
}

// One thcrap patch: where it lives, its patch.js and file list.
pub struct Patch {
    pub repo: String,
    pub id: String,
    pub base: String,
    pub meta: Map<String, Value>,
    pub files_raw: Vec<u8>,
    pub files: BTreeMap<String, u32>,
}

impl Patch {
    pub fn key(&self) -> String {
        format!("{}/{}", self.repo, self.id)
    }

    // `dependencies`: (repo, patch); a bare name is in this patch's repo.
    pub fn dependencies(&self) -> Vec<(String, String)> {
        self.meta
            .get("dependencies")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(|d| match d.split_once('/') {
                Some((r, p)) => (r.to_string(), p.to_string()),
                None => (self.repo.clone(), d.to_string()),
            })
            .filter(|(r, p)| valid_id(r) && valid_id(p))
            .collect()
    }

    // `fonts` of patch.js (file names relative to the patch).
    pub fn declared_fonts(&self) -> Vec<String> {
        self.meta
            .get("fonts")
            .and_then(Value::as_object)
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default()
    }
}

pub struct Client {
    agent: ureq::Agent,
    // Repo id → base URL; `thpatch` is the server itself, others come from `repo.js` neighbours.
    repos: Mutex<HashMap<String, String>>,
    neighbours: Mutex<Option<Vec<String>>>,
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

impl Client {
    pub fn new() -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_secs(60)))
            .build()
            .into();
        let repos = HashMap::from([("thpatch".to_string(), SERVER.to_string())]);
        Self {
            agent,
            repos: Mutex::new(repos),
            neighbours: Mutex::new(None),
        }
    }

    pub fn get(&self, url: &str) -> Result<Vec<u8>> {
        let mut r = self
            .agent
            .get(url)
            .call()
            .with_context(|| format!("GET {url}"))?;
        Ok(r.body_mut().with_config().limit(MAX_FILE).read_to_vec()?)
    }

    // Languages of the server (`repo.js` patches named `lang_…`): (id, title).
    pub fn languages(&self) -> Result<Vec<(String, String)>> {
        let repo = json_object(&self.get(&format!("{SERVER}repo.js"))?, "repo.js")?;
        let patches = repo
            .get("patches")
            .and_then(Value::as_object)
            .context("repo.js: no patches")?;
        Ok(patches
            .iter()
            .filter(|(k, _)| valid_lang(k))
            .map(|(k, t)| (k.clone(), t.as_str().unwrap_or("").to_string()))
            .collect())
    }

    // Base URL of repo `id`: the server's `repo.js` neighbours are asked for their `repo.js` id until it matches.
    fn repo_base(&self, id: &str) -> Result<String> {
        if let Some(b) = self.repos.lock().unwrap().get(id) {
            return Ok(b.clone());
        }
        let list = {
            let mut n = self.neighbours.lock().unwrap();
            if n.is_none() {
                let repo = json_object(&self.get(&format!("{SERVER}repo.js"))?, "repo.js")?;
                *n = Some(
                    repo.get("neighbors")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .filter(|u| u.starts_with("https://"))
                        .map(|u| format!("{}/", u.trim_end_matches('/')))
                        .collect(),
                );
            }
            n.clone().unwrap_or_default()
        };
        // thpatch's own mirror first: the base patches (nmlgc) live there.
        let mut list = list;
        list.sort_by_key(|u| !u.contains(".thpatch.net/"));
        for u in list {
            let Ok(b) = self.get(&format!("{u}repo.js")) else {
                continue;
            };
            let Ok(r) = json_object(&b, "repo.js") else {
                continue;
            };
            if let Some(rid) = r.get("id").and_then(Value::as_str) {
                self.repos
                    .lock()
                    .unwrap()
                    .insert(rid.to_string(), u.clone());
                if rid == id {
                    return Ok(u);
                }
            }
        }
        anyhow::bail!("repo {id}: not among the thpatch neighbours")
    }

    pub fn patch(&self, repo: &str, id: &str) -> Result<Patch> {
        let base = format!("{}{id}/", self.repo_base(repo)?);
        let meta = json_object(&self.get(&format!("{base}patch.js"))?, "patch.js")?;
        let files_raw = self.get(&format!("{base}files.js"))?;
        let files = file_list(&files_raw)?;
        Ok(Patch {
            repo: repo.to_string(),
            id: id.to_string(),
            base,
            meta,
            files_raw,
            files,
        })
    }

    // `lang` and every patch it depends on, dependencies first (thcrap's stacking order, the language last).
    pub fn chain(&self, lang: &str) -> Result<Vec<Patch>> {
        anyhow::ensure!(valid_lang(lang), "{lang}: not a lang_xx patch id");
        let mut out: Vec<Patch> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        self.visit("thpatch", lang, &mut out, &mut seen, 0)?;
        Ok(out)
    }

    fn visit(
        &self,
        repo: &str,
        id: &str,
        out: &mut Vec<Patch>,
        seen: &mut std::collections::HashSet<String>,
        depth: usize,
    ) -> Result<()> {
        if depth > 8 || !seen.insert(format!("{repo}/{id}")) {
            return Ok(());
        }
        let p = match self.patch(repo, id) {
            Ok(p) => p,
            // A missing dependency only costs its fonts; the language itself must exist.
            Err(e) if depth > 0 => {
                println!("  dependency {repo}/{id}: {e:#}");
                return Ok(());
            }
            Err(e) => return Err(e),
        };
        for (r, d) in p.dependencies() {
            self.visit(&r, &d, out, seen, depth + 1)?;
        }
        out.push(p);
        Ok(())
    }

    // Downloads `(url, path, crc)` items in parallel, skipping files already there with the right CRC; returns the
    // failed requests.
    fn download_all(&self, items: &[(String, PathBuf, u32)]) -> Vec<String> {
        let crc_ok =
            |p: &Path, crc: u32| std::fs::read(p).is_ok_and(|b| crc32fast::hash(&b) == crc);
        let todo: Vec<&(String, PathBuf, u32)> =
            items.iter().filter(|(_, p, c)| !crc_ok(p, *c)).collect();
        let next = std::sync::atomic::AtomicUsize::new(0);
        let errors = Mutex::new(Vec::new());
        std::thread::scope(|s| {
            for _ in 0..WORKERS {
                s.spawn(|| {
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some((url, p, crc)) = todo.get(i) else {
                            break;
                        };
                        let r = self.get(url).and_then(|b| {
                            // The server's own list can be stale (lang_ru themes.js, 2026-11): kept, reported.
                            if crc32fast::hash(&b) != *crc {
                                println!("  warning: {url}: CRC32 differs from files.js (kept)");
                            }
                            if let Some(d) = p.parent() {
                                std::fs::create_dir_all(d)?;
                            }
                            Ok(std::fs::write(p, b)?)
                        });
                        if let Err(e) = r {
                            errors.lock().unwrap().push(format!("{e:#}"));
                        }
                    }
                });
            }
        });
        errors.into_inner().unwrap()
    }
}

// A font declared by a patch of the chain: patch key, declared name, local file (None = not available), status.
#[derive(Clone, Debug)]
pub struct FontDecl {
    pub patch: String,
    pub name: String,
    pub file: Option<PathBuf>,
    pub status: String,
}

// The fonts of a chain (language first, then its dependencies from the last applied) and the th08 game face: the
// `font` of the first th08.js in that order (thcrap: a later patch overrides an earlier one).
pub struct Fonts {
    pub face: Option<(String, String)>,
    pub decls: Vec<FontDecl>,
}

// A fetched language: cache folder (`files.js`, `patch.js`, `SOURCE.txt`, `th08/…`, `fonts/<repo>_<patch>/…`).
pub struct Fetched {
    pub dir: PathBuf,
    pub fonts: Fonts,
}

// Downloads the th08 text (and with `full` the images and fonts) of `lang` into `<cache>/<lang>/<revision>/`.
pub fn fetch(c: &Client, lang: &str, cache: &Path, full: bool) -> Result<Fetched> {
    let chain = if full {
        c.chain(lang)?
    } else {
        vec![c.patch("thpatch", lang)?]
    };
    let me = chain.last().context("empty chain")?;
    let rev = revision(&me.files_raw);
    let dir = cache.join(lang).join(&rev);
    std::fs::create_dir_all(&dir)?;
    let mut items: Vec<(String, PathBuf, u32)> = me
        .files
        .iter()
        .filter(|(n, _)| is_text(n) || (full && is_image(n)) || n.as_str() == "th08.js")
        .map(|(n, c)| (format!("{}{n}", me.base), dir.join(n), *c))
        .collect();
    let mut decls = Vec::new();
    let mut face = None;
    if full {
        for p in chain.iter().rev() {
            let sub = format!("{}_{}", p.repo, p.id);
            if face.is_none()
                && let Some(&crc) = p.files.get("th08.js")
            {
                let path = dir.join("fonts").join(&sub).join("th08.js");
                let e = c.download_all(&[(format!("{}th08.js", p.base), path.clone(), crc)]);
                if e.is_empty()
                    && let Ok(m) = std::fs::read(&path)
                        .map_err(anyhow::Error::from)
                        .and_then(|b| json_object(&b, "th08.js"))
                    && let Some(f) = m.get("font").and_then(Value::as_str)
                {
                    face = Some((p.key(), f.to_string()));
                }
            }
            for name in p.declared_fonts() {
                let (file, status) = if !is_th08_font(&name) {
                    (None, "for another game, skipped".to_string())
                } else if let Some(&crc) = p.files.get(&name) {
                    let path = dir.join("fonts").join(&sub).join(&name);
                    items.push((
                        format!("{}{}", p.base, name.replace(' ', "%20")),
                        path.clone(),
                        crc,
                    ));
                    (Some(path), "downloaded".to_string())
                } else {
                    (None, "declared, not on the server".to_string())
                };
                decls.push(FontDecl {
                    patch: p.key(),
                    name,
                    file,
                    status,
                });
            }
        }
    }
    let todo = items.len();
    println!(
        "{lang} revision {rev}: {todo} files ({}, {LICENCE}) in {}",
        me.base,
        dir.display()
    );
    let errors = c.download_all(&items);
    anyhow::ensure!(errors.is_empty(), "{lang}: {}", errors.join("; "));
    std::fs::write(dir.join("files.js"), &me.files_raw)?;
    std::fs::write(dir.join("patch.js"), serde_json::to_vec_pretty(&me.meta)?)?;
    if !dir.join("SOURCE.txt").is_file() {
        let now = jiff::Timestamp::now();
        std::fs::write(
            dir.join("SOURCE.txt"),
            format!(
                "url {}\nretrieved {}\nrevision {rev}\n",
                me.base,
                now.strftime("%Y-%m-%d %H:%M UTC")
            ),
        )?;
    }
    Ok(Fetched {
        dir,
        fonts: Fonts { face, decls },
    })
}

// The same for a local thcrap install: `<repos>/<repo>/<patch>/` folders (the language folder is one of them).
pub fn local_fonts(lang_dir: &Path) -> Fonts {
    let repos = lang_dir.parent().and_then(Path::parent);
    let repo_of = |d: &Path| {
        d.parent()
            .and_then(Path::file_name)
            .map_or_else(|| "local".to_string(), |s| s.to_string_lossy().into_owned())
    };
    let read = |d: &Path, f: &str| {
        std::fs::read(d.join(f))
            .ok()
            .and_then(|b| json_object(&b, f).ok())
    };
    // Depth-first like `Client::chain`, then the language first.
    let mut order: Vec<(String, PathBuf, Map<String, Value>)> = Vec::new();
    let mut stack = vec![(repo_of(lang_dir), lang_dir.to_path_buf(), 0usize)];
    let mut seen = std::collections::HashSet::new();
    while let Some((repo, d, depth)) = stack.pop() {
        let id = d
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        if depth > 8 || !seen.insert(format!("{repo}/{id}")) {
            continue;
        }
        let Some(meta) = read(&d, "patch.js") else {
            continue;
        };
        for dep in meta
            .get("dependencies")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            let (r, p) = dep
                .split_once('/')
                .map_or((repo.clone(), dep.to_string()), |(r, p)| {
                    (r.to_string(), p.to_string())
                });
            if let (Some(root), true) = (repos, valid_id(&r) && valid_id(&p)) {
                stack.push((r.clone(), root.join(&r).join(&p), depth + 1));
            }
        }
        order.push((format!("{repo}/{id}"), d, meta));
    }
    let mut face = None;
    let mut decls = Vec::new();
    for (key, d, meta) in &order {
        if face.is_none()
            && let Some(f) = read(d, "th08.js")
                .and_then(|m| m.get("font").and_then(Value::as_str).map(str::to_string))
        {
            face = Some((key.clone(), f));
        }
        for name in meta
            .get("fonts")
            .and_then(Value::as_object)
            .map(|m| m.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default()
        {
            let p = d.join(&name);
            let (file, status) = if !is_th08_font(&name) {
                (None, "for another game, skipped".to_string())
            } else if p.is_file() {
                (Some(p), "local file".to_string())
            } else {
                (None, "declared, not installed".to_string())
            };
            decls.push(FontDecl {
                patch: key.clone(),
                name,
                file,
                status,
            });
        }
    }
    Fonts { face, decls }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_filter() {
        assert!(is_text("stringdefs.js"));
        assert!(is_text("th08/msg1a.dat.jdiff"));
        assert!(is_image("th08/data/text/stg1txt.png"));
        assert!(!is_text("th08/data/text/stg1txt.png"));
        assert!(is_text("th08/spells.js"));
        assert!(!is_text("th08/end00a.end"));
        assert!(!is_text("th08.js"));
        assert!(!is_text("th07/msg1.dat.jdiff"));
        assert!(!is_image("th08/../x.png"));
        assert!(!is_image("/th08/x.png"));
        assert!(!is_image("th08/a\\b.png"));
        assert!(is_th08_font("PT Sans-Narrow.ttf") && is_th08_font("th08/MyriadPro-Cond.otf"));
        assert!(!is_th08_font("th175/data/font/ipam.ttf") && !is_th08_font("x.png"));
        assert!(valid_lang("lang_pt-br") && !valid_lang("lang_") && !valid_lang("lang_../x"));
    }

    #[test]
    fn files_list_drops_deleted() {
        let l = file_list(br#"{"b.js": 5, "a.js": null, "c": "x"}"#).unwrap();
        assert_eq!(l.into_iter().collect::<Vec<_>>(), [("b.js".to_string(), 5)]);
    }

    #[test]
    fn dependencies_and_fonts() {
        let meta = json_object(
            br#"{"dependencies": ["nmlgc/script_latin", "base", "../x"], "fonts": {"A.ttf": true}}"#,
            "t",
        )
        .unwrap();
        let p = Patch {
            repo: "thpatch".into(),
            id: "lang_xx".into(),
            base: String::new(),
            meta,
            files_raw: Vec::new(),
            files: BTreeMap::new(),
        };
        let d = p.dependencies();
        assert_eq!(
            d,
            [
                ("nmlgc".into(), "script_latin".into()),
                ("thpatch".into(), "base".into())
            ]
        );
        assert_eq!(p.declared_fonts(), ["A.ttf"]);
    }
}
