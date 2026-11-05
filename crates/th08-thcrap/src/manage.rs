//! Translation packs for the launchers: the server's languages with an estimated coverage, install / update into
//! `<config>/packs/lang_<code>` (download cache `<config>/thcrap`), remove.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::Progress;
use crate::build::{self, Options, Summary};
use crate::thcrap::Vanilla;
use crate::thpatch::{self, Client};

// Text files of a complete th08 translation: 32 msg, 12 ending jdiffs, spells / spell comments / music comments.
const TEXT_FILES: usize = 32 + 12 + 3;

// A server language with th08 files.
#[derive(Clone, Debug, PartialEq)]
pub struct ServerLang {
    // `lang_xx`.
    pub id: String,
    pub title: String,
    // Language patches it builds on (`lang_en` for `lang_en-gb`), nearest first.
    pub base: Vec<String>,
    // th08 text files of the language itself, out of `TEXT_FILES`.
    pub text_files: usize,
    // The same with its base languages (what a pack of it covers; base text shows where it lacks its own).
    pub chain_files: usize,
    pub images: usize,
    // `thpatch::revision` of its `files.js` (an installed pack of the same revision is up to date when it has no
    // base languages).
    pub revision: String,
}

impl ServerLang {
    pub fn code(&self) -> &str {
        self.id.strip_prefix("lang_").unwrap_or(&self.id)
    }

    // Share of the th08 text files the language translates itself, 0–100: an estimate before download (the exact
    // slot coverage comes with the import).
    pub fn estimate(&self) -> f64 {
        100.0 * self.text_files as f64 / TEXT_FILES as f64
    }
}

fn th08_text(name: &str) -> bool {
    thpatch::is_text(name) && name.starts_with("th08/")
}

// Every server language that has (or inherits) th08 files, by id.
pub fn server_languages(c: &Client) -> Result<Vec<ServerLang>> {
    let langs = c.languages()?;
    let rows = std::sync::Mutex::new(BTreeMap::new());
    let next = std::sync::atomic::AtomicUsize::new(0);
    crate::parallel(8, || {
        loop {
            let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let Some((id, title)) = langs.get(i) else {
                break;
            };
            let Ok(raw) = thpatch::files_js(c, id) else {
                continue;
            };
            let Ok(files) = thpatch::file_list(&raw) else {
                continue;
            };
            let deps = c
                .patch("thpatch", id)
                .map(|p| p.dependencies())
                .unwrap_or_default();
            let base: Vec<String> = deps
                .into_iter()
                .filter(|(r, d)| r == "thpatch" && thpatch::valid_lang(d))
                .map(|(_, d)| d)
                .collect();
            let text: BTreeSet<String> = files.keys().filter(|n| th08_text(n)).cloned().collect();
            let images = files.keys().filter(|n| thpatch::is_image(n)).count();
            crate::lock(&rows).insert(
                id.clone(),
                (title.clone(), base, text, images, thpatch::revision(&raw)),
            );
        }
    });
    let rows = rows
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut out = Vec::new();
    for (id, (title, _, own, _, rev)) in &rows {
        // Base chain, nearest first; files of the whole chain count.
        let mut base = Vec::new();
        let mut text = BTreeSet::new();
        let mut images = 0;
        let mut todo = vec![id.clone()];
        while let Some(l) = todo.pop() {
            if base.contains(&l) || base.len() > 8 {
                continue;
            }
            if let Some((_, b, t, im, _)) = rows.get(&l) {
                text.extend(t.iter().cloned());
                images = images.max(*im);
                todo.extend(b.iter().rev().cloned());
            }
            base.push(l);
        }
        base.remove(0);
        let chain_files = text.len().min(TEXT_FILES);
        if chain_files + images == 0 {
            continue;
        }
        out.push(ServerLang {
            id: id.clone(),
            title: title.clone(),
            base,
            text_files: own.len().min(TEXT_FILES),
            chain_files,
            images,
            revision: rev.clone(),
        });
    }
    Ok(out)
}

// Key kind → (translated, total) slots.
pub type PerKind = BTreeMap<String, (usize, usize)>;

// Exact slot coverage of a language from a text-only download (cached): overall % and per key kind (have, total).
pub fn coverage(
    c: &Client,
    id: &str,
    cache: &Path,
    v: &Vanilla,
    p: &dyn Progress,
) -> Result<(f64, PerKind)> {
    let f = thpatch::fetch(c, id, cache, false, p)?;
    let lang = crate::thcrap::Lang::read_layers(&f.layers)?;
    let mut report = crate::thcrap::Report::default();
    let tables = crate::thcrap::import(&lang, v, &mut report);
    let per = crate::thcrap::coverage(|k| tables.values().any(|t| t.contains_key(k)), v);
    let (have, n) = per.values().fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
    let pct = if n == 0 {
        0.0
    } else {
        100.0 * have as f64 / n as f64
    };
    Ok((pct, per))
}

pub fn packs_dir(config: &Path) -> PathBuf {
    config.join("packs")
}

pub fn cache_dir(config: &Path) -> PathBuf {
    config.join("thcrap")
}

fn lang_id(code: &str) -> Result<String> {
    let id = if code.starts_with("lang_") {
        code.to_string()
    } else {
        format!("lang_{code}")
    };
    anyhow::ensure!(thpatch::valid_lang(&id), "{code}: not a language code");
    Ok(id)
}

// Downloads (or updates) `code` and builds its pack into `<config>/packs/lang_<code>`; the previous pack is replaced
// only once the new one is complete.
pub fn install(
    c: &Client,
    code: &str,
    v: &Vanilla,
    config: &Path,
    p: &dyn Progress,
) -> Result<Summary> {
    let id = lang_id(code)?;
    let f = thpatch::fetch(c, &id, &cache_dir(config), true, p)?;
    install_fetched(&id, &f, v, config, p)
}

// Downloads the files `install` needs into the cache (the browser's first step: it fetches, then builds).
pub fn fetch(c: &Client, code: &str, config: &Path, p: &dyn Progress) -> Result<thpatch::Fetched> {
    thpatch::fetch(c, &lang_id(code)?, &cache_dir(config), true, p)
}

// The build step of `install` over a finished download.
pub fn install_fetched(
    code: &str,
    f: &thpatch::Fetched,
    v: &Vanilla,
    config: &Path,
    p: &dyn Progress,
) -> Result<Summary> {
    let id = lang_id(code)?;
    let cache = cache_dir(config);
    let packs = packs_dir(config);
    crate::vfs::create_dir_all(&packs)?;
    let tmp = packs.join(format!(".{id}.new"));
    if crate::vfs::exists(&tmp) {
        crate::vfs::remove_dir_all(&tmp)?;
    }
    let mut sum = build::build_pack(
        &id,
        &f.title,
        &f.layers,
        &f.fonts,
        v,
        &tmp,
        Options::default(),
        p,
    )?;
    let dst = packs.join(&id);
    if crate::vfs::exists(&dst) {
        crate::vfs::remove_dir_all(&dst).with_context(|| format!("remove {}", dst.display()))?;
    }
    crate::vfs::rename(&tmp, &dst)?;
    sum.out = dst;
    // Older revisions of this language in the cache are not needed any more.
    for e in crate::vfs::list(&cache.join(&id)).into_iter().flatten() {
        if e != f.dir {
            let _ = crate::vfs::remove_dir_all(e);
        }
    }
    Ok(sum)
}

// Removes the pack `lang_<code>` from `<config>/packs` and its download cache.
pub fn remove(code: &str, config: &Path) -> Result<()> {
    let id = lang_id(code)?;
    let dst = packs_dir(config).join(&id);
    anyhow::ensure!(
        crate::vfs::is_file(&dst.join("pack.toml")),
        "{}: no installed pack",
        dst.display()
    );
    crate::vfs::remove_dir_all(&dst)?;
    let _ = crate::vfs::remove_dir_all(cache_dir(config).join(&id));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_and_estimate() {
        assert_eq!(lang_id("pt-br").unwrap(), "lang_pt-br");
        assert_eq!(lang_id("lang_ru").unwrap(), "lang_ru");
        assert!(lang_id("../x").is_err());
        let l = ServerLang {
            id: "lang_en-gb".into(),
            title: String::new(),
            base: vec!["lang_en".into()],
            text_files: TEXT_FILES,
            chain_files: TEXT_FILES,
            images: 0,
            revision: String::new(),
        };
        assert_eq!((l.code(), l.estimate()), ("en-gb", 100.0));
        assert!(th08_text("th08/spells.js") && !th08_text("stringdefs.js"));
    }

    #[test]
    fn remove_needs_a_pack() {
        let d = std::env::temp_dir().join(format!("th08-thcrap-rm-{}", std::process::id()));
        let p = packs_dir(&d).join("lang_xx");
        std::fs::create_dir_all(&p).unwrap();
        assert!(remove("xx", &d).is_err());
        std::fs::write(p.join("pack.toml"), "id = \"lang_xx\"\n").unwrap();
        std::fs::create_dir_all(cache_dir(&d).join("lang_xx/r")).unwrap();
        remove("xx", &d).unwrap();
        assert!(!p.exists() && !cache_dir(&d).join("lang_xx").exists());
        std::fs::remove_dir_all(&d).unwrap();
    }
}
