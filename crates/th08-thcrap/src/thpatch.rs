//! thpatch patches for th08 (I18N G3, I18N §1.2 server file list): the server's language list, a language patch and
//! its dependency chain (`patch.js` `dependencies`, repos found through `repo.js` neighbours), and the cached download
//! `<cache>/<lang>/<revision>/` of its th08 text, images and declared fonts. Downloads are untrusted data: names are
//! checked, sizes capped, CRC32 checked against `files.js`, full `.end` scripts never fetched, only JSON is parsed.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context, Result};
use serde_json::{Map, Value};

use crate::Progress;

pub const SERVER: &str = "https://srv.thpatch.net/";
pub const LICENCE: &str =
    "Creative Commons Attribution-ShareAlike 4.0 International (CC BY-SA 4.0)";
pub const LICENCE_URL: &str = "https://creativecommons.org/licenses/by-sa/4.0/";
const MAX_FILE: u64 = 32 << 20;
// Per fetched language, far above any real th08 patch chain.
const MAX_ITEMS: usize = 5000;
const MAX_TOTAL: u64 = 512 << 20;
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

// Natively a blocking HTTPS agent. In the browser requests cannot block: `get` answers from the responses the page
// fetched and records every other URL as pending; the page fetches those and runs the job again until none is left.
pub struct Client {
    #[cfg(not(target_arch = "wasm32"))]
    agent: ureq::Agent,
    #[cfg(target_arch = "wasm32")]
    responses: Mutex<HashMap<String, Result<Vec<u8>, String>>>,
    #[cfg(target_arch = "wasm32")]
    pending: Mutex<std::collections::BTreeSet<String>>,
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
        let repos = HashMap::from([("thpatch".to_string(), SERVER.to_string())]);
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            agent: ureq::Agent::config_builder()
                .timeout_global(Some(std::time::Duration::from_secs(60)))
                .https_only(true)
                .build()
                .into(),
            #[cfg(target_arch = "wasm32")]
            responses: Mutex::default(),
            #[cfg(target_arch = "wasm32")]
            pending: Mutex::default(),
            repos: Mutex::new(repos),
            neighbours: Mutex::new(None),
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn get(&self, url: &str) -> Result<Vec<u8>> {
        match self.responses.lock().unwrap().get(url) {
            Some(Ok(b)) => Ok(b.clone()),
            Some(Err(e)) => anyhow::bail!("GET {url}: {e}"),
            None => {
                self.pending.lock().unwrap().insert(url.to_string());
                anyhow::bail!("GET {url}: not fetched yet")
            }
        }
    }

    // The URLs asked for since the last call that have no response yet.
    #[cfg(target_arch = "wasm32")]
    pub fn take_pending(&self) -> Vec<String> {
        std::mem::take(&mut *self.pending.lock().unwrap())
            .into_iter()
            .collect()
    }

    // A response fetched by the page: the body (at most `MAX_FILE` bytes) or the error.
    #[cfg(target_arch = "wasm32")]
    pub fn answer(&self, url: &str, r: Result<Vec<u8>, String>) {
        let r = r.and_then(|b| {
            if b.len() as u64 > MAX_FILE {
                Err(format!("larger than {} MiB", MAX_FILE >> 20))
            } else {
                Ok(b)
            }
        });
        self.responses.lock().unwrap().insert(url.to_string(), r);
    }

    // Up to three attempts: a phone's connection drops single requests of a parallel batch.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn get(&self, url: &str) -> Result<Vec<u8>> {
        let mut last = None;
        for attempt in 0..3u64 {
            if attempt > 0 {
                std::thread::sleep(std::time::Duration::from_millis(500 * attempt));
            }
            let r = self
                .agent
                .get(url)
                .call()
                .with_context(|| format!("GET {url}"))
                .and_then(|mut r| Ok(r.body_mut().with_config().limit(MAX_FILE).read_to_vec()?));
            match r {
                Ok(b) => return Ok(b),
                Err(e) => last = Some(e),
            }
        }
        Err(last.unwrap_or_else(|| anyhow::anyhow!("GET {url}")))
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

    // `lang` and every patch it depends on, dependencies first (thcrap's stacking order, the language last);
    // `langs_only` follows only `lang_*` dependencies (the text layers).
    pub fn chain(&self, lang: &str, langs_only: bool, p: &dyn Progress) -> Result<Vec<Patch>> {
        anyhow::ensure!(valid_lang(lang), "{lang}: not a lang_xx patch id");
        let mut out: Vec<Patch> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let mut v = Visit {
            out: &mut out,
            seen: &mut seen,
            langs_only,
            p,
        };
        self.visit("thpatch", lang, &mut v, 0)?;
        Ok(out)
    }

    fn visit(&self, repo: &str, id: &str, v: &mut Visit<'_>, depth: usize) -> Result<()> {
        if depth > 8 || (v.langs_only && !valid_lang(id)) || !v.seen.insert(format!("{repo}/{id}"))
        {
            return Ok(());
        }
        let p = match self.patch(repo, id) {
            Ok(p) => p,
            // A missing dependency only costs its fonts (or its base text); the language itself must exist.
            Err(e) if depth > 0 => {
                v.p.line(&format!("  dependency {repo}/{id}: {e:#}"));
                return Ok(());
            }
            Err(e) => return Err(e),
        };
        for (r, d) in p.dependencies() {
            self.visit(&r, &d, v, depth + 1)?;
        }
        v.out.push(p);
        Ok(())
    }

    // Downloads `(url, path, crc)` items in parallel, skipping files already there with the right CRC; returns the
    // failed requests.
    fn download_all(&self, items: &[(String, PathBuf, u32)], prog: &dyn Progress) -> Vec<String> {
        let crc_ok =
            |p: &Path, crc: u32| crate::vfs::read(p).is_ok_and(|b| crc32fast::hash(&b) == crc);
        let todo: Vec<&(String, PathBuf, u32)> =
            items.iter().filter(|(_, p, c)| !crc_ok(p, *c)).collect();
        let next = std::sync::atomic::AtomicUsize::new(0);
        let done = std::sync::atomic::AtomicUsize::new(0);
        let errors = Mutex::new(Vec::new());
        let bytes = std::sync::atomic::AtomicU64::new(0);
        prog.files(0, todo.len());
        let worker = || {
            loop {
                let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let Some((url, p, crc)) = todo.get(i) else {
                    break;
                };
                let r = self.get(url).and_then(|b| {
                    charge(&bytes, b.len(), url)?;
                    // The server's own list can be stale (lang_ru themes.js, 2026-11): kept, reported.
                    if crc32fast::hash(&b) != *crc {
                        prog.line(&format!(
                            "  warning: {url}: CRC32 differs from files.js (kept)"
                        ));
                    }
                    if let Some(d) = p.parent() {
                        crate::vfs::create_dir_all(d)?;
                    }
                    Ok(crate::vfs::write(p, b)?)
                });
                if let Err(e) = r {
                    errors.lock().unwrap().push(format!("{e:#}"));
                }
                let n = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                prog.files(n, todo.len());
            }
        };
        crate::parallel(WORKERS, worker);
        errors.into_inner().unwrap()
    }
}

fn check_count(lang: &str, n: usize) -> Result<()> {
    anyhow::ensure!(
        n <= MAX_ITEMS,
        "{lang}: {n} files listed, more than {MAX_ITEMS}"
    );
    Ok(())
}

// Adds a body to the running byte total of one fetch; past `MAX_TOTAL` the download fails.
fn charge(total: &std::sync::atomic::AtomicU64, len: usize, url: &str) -> Result<()> {
    let len = len as u64;
    let before = total.fetch_add(len, std::sync::atomic::Ordering::Relaxed);
    anyhow::ensure!(
        before + len <= MAX_TOTAL,
        "{url}: language larger than {} MiB",
        MAX_TOTAL >> 20
    );
    Ok(())
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

// Patch-chain state of `Client::chain`.
struct Visit<'a> {
    out: &'a mut Vec<Patch>,
    seen: &'a mut std::collections::HashSet<String>,
    langs_only: bool,
    p: &'a dyn Progress,
}

// A fetched language: cache folder (`files.js`, `patch.js`, `SOURCE.txt`, `th08/…`, `fonts/<repo>_<patch>/…`,
// `base/<lang>/…` = the text of the languages it builds on), its text layers (base languages first, `dir` last) and
// its fonts.
pub struct Fetched {
    pub dir: PathBuf,
    pub layers: Vec<PathBuf>,
    pub title: String,
    pub fonts: Fonts,
}

// Downloads the th08 text (and with `full` the images and fonts) of `lang` into `<cache>/<lang>/<revision>/`.
pub fn fetch(
    c: &Client,
    lang: &str,
    cache: &Path,
    full: bool,
    p: &dyn Progress,
) -> Result<Fetched> {
    let chain = c.chain(lang, !full, p)?;
    let me = chain.last().context("empty chain")?;
    // Language patches of the chain (a variant such as lang_en-gb on lang_en), base first.
    let langs: Vec<&Patch> = chain
        .iter()
        .filter(|q| q.repo == "thpatch" && valid_lang(&q.id))
        .collect();
    let rev = if langs.len() > 1 {
        revision(
            &langs
                .iter()
                .flat_map(|q| q.files_raw.clone())
                .collect::<Vec<u8>>(),
        )
    } else {
        revision(&me.files_raw)
    };
    let dir = cache.join(lang).join(&rev);
    crate::vfs::create_dir_all(&dir)?;
    let mut layers = Vec::new();
    let mut items: Vec<(String, PathBuf, u32)> = Vec::new();
    for q in &langs {
        let d = if q.id == me.id {
            dir.clone()
        } else {
            dir.join("base").join(&q.id)
        };
        items.extend(
            q.files
                .iter()
                .filter(|(n, _)| {
                    is_text(n)
                        || (full && is_image(n))
                        || (q.id == me.id && n.as_str() == "th08.js")
                })
                .map(|(n, c)| (format!("{}{n}", q.base), d.join(n), *c)),
        );
        layers.push(d);
    }
    let mut decls = Vec::new();
    let mut face = None;
    if full {
        for q in chain.iter().rev() {
            let sub = format!("{}_{}", q.repo, q.id);
            if face.is_none()
                && let Some(&crc) = q.files.get("th08.js")
            {
                let path = dir.join("fonts").join(&sub).join("th08.js");
                let e = c.download_all(&[(format!("{}th08.js", q.base), path.clone(), crc)], p);
                if e.is_empty()
                    && let Ok(m) = crate::vfs::read(&path)
                        .map_err(anyhow::Error::from)
                        .and_then(|b| json_object(&b, "th08.js"))
                    && let Some(f) = m.get("font").and_then(Value::as_str)
                {
                    face = Some((q.key(), f.to_string()));
                }
            }
            for name in q.declared_fonts() {
                let (file, status) = if !is_th08_font(&name) {
                    (None, "for another game, skipped".to_string())
                } else if let Some(&crc) = q.files.get(&name) {
                    let path = dir.join("fonts").join(&sub).join(&name);
                    items.push((
                        format!("{}{}", q.base, name.replace(' ', "%20")),
                        path.clone(),
                        crc,
                    ));
                    (Some(path), "downloaded".to_string())
                } else {
                    (None, "declared, not on the server".to_string())
                };
                decls.push(FontDecl {
                    patch: q.key(),
                    name,
                    file,
                    status,
                });
            }
        }
    }
    let todo = items.len();
    check_count(lang, todo)?;
    p.line(&format!(
        "{lang} revision {rev}: {todo} files ({}, {LICENCE}) in {}",
        me.base,
        dir.display()
    ));
    let errors = c.download_all(&items, p);
    anyhow::ensure!(errors.is_empty(), "{lang}: {}", errors.join("; "));
    crate::vfs::write(dir.join("files.js"), &me.files_raw)?;
    crate::vfs::write(dir.join("patch.js"), serde_json::to_vec_pretty(&me.meta)?)?;
    if !crate::vfs::is_file(&dir.join("SOURCE.txt")) {
        let now = jiff::Timestamp::now();
        crate::vfs::write(
            dir.join("SOURCE.txt"),
            format!(
                "url {}\nretrieved {}\nrevision {rev}\n",
                me.base,
                now.strftime("%Y-%m-%d %H:%M UTC")
            ),
        )?;
    }
    let title = me
        .meta
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    Ok(Fetched {
        dir,
        layers,
        title,
        fonts: Fonts { face, decls },
    })
}

// Text layers of a local thcrap `lang_xx` folder: the `lang_*` patches it depends on (`<repos>/<repo>/<patch>/`,
// base first), then the folder itself.
pub fn local_layers(lang_dir: &Path) -> Vec<PathBuf> {
    let repos = lang_dir.parent().and_then(Path::parent);
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    local_visit(repos, lang_dir, &mut out, &mut seen, 0);
    out
}

fn local_visit(
    repos: Option<&Path>,
    d: &Path,
    out: &mut Vec<PathBuf>,
    seen: &mut std::collections::HashSet<PathBuf>,
    depth: usize,
) {
    if depth > 8 || !seen.insert(d.to_path_buf()) {
        return;
    }
    let repo = d
        .parent()
        .and_then(Path::file_name)
        .map(|s| s.to_string_lossy().into_owned());
    let meta = crate::vfs::read(d.join("patch.js"))
        .ok()
        .and_then(|b| json_object(&b, "patch.js").ok());
    for dep in meta
        .iter()
        .filter_map(|m| m.get("dependencies").and_then(Value::as_array))
        .flatten()
        .filter_map(Value::as_str)
    {
        let (r, p) = dep
            .split_once('/')
            .map_or((repo.clone(), dep), |(r, p)| (Some(r.to_string()), p));
        if let (Some(root), Some(r)) = (repos, r)
            && valid_id(&r)
            && valid_lang(p)
        {
            local_visit(repos, &root.join(r).join(p), out, seen, depth + 1);
        }
    }
    if depth == 0 || crate::vfs::is_dir(d) {
        out.push(d.to_path_buf());
    }
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
        crate::vfs::read(d.join(f))
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
            } else if crate::vfs::is_file(&p) {
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
    fn download_caps() {
        #[cfg(not(target_arch = "wasm32"))]
        assert!(Client::new().agent.config().https_only());
        assert!(check_count("lang_en", MAX_ITEMS).is_ok());
        assert!(check_count("lang_en", MAX_ITEMS + 1).is_err());
        let t = std::sync::atomic::AtomicU64::new(0);
        assert!(charge(&t, (MAX_TOTAL - 1) as usize, "a").is_ok());
        assert!(charge(&t, 1, "b").is_ok());
        assert!(charge(&t, 1, "c").is_err());
    }

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

    // A local thcrap install: `<repos>/thpatch/lang_en-gb` on `lang_en`; non-language dependencies are not text layers.
    #[test]
    fn local_text_layers() {
        let root = std::env::temp_dir().join(format!("th08-local-{}", std::process::id()));
        let repo = root.join("thpatch");
        for (id, deps) in [
            ("lang_en-gb", r#"["lang_en"]"#),
            ("lang_en", r#"["nmlgc/base_tsa"]"#),
        ] {
            std::fs::create_dir_all(repo.join(id)).unwrap();
            std::fs::write(
                repo.join(id).join("patch.js"),
                format!(r#"{{"dependencies": {deps}}}"#),
            )
            .unwrap();
        }
        let l = local_layers(&repo.join("lang_en-gb"));
        assert_eq!(l, [repo.join("lang_en"), repo.join("lang_en-gb")]);
        assert_eq!(local_layers(&repo.join("lang_en")), [repo.join("lang_en")]);
        std::fs::remove_dir_all(&root).unwrap();
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
