//! Save-data backup: score.dat, th08.cfg, replays and port.cfg player keys in one zip, and its validated import.

use std::io::{Cursor, Read, Write};

use th08_core::host::LocalTime;

pub const MANIFEST: &str = "manifest.json";
pub const FORMAT: u64 = 1;
// Largest zip the launchers read for an import.
pub const MAX_ZIP: u64 = 256 << 20;
const MAX_TOTAL: u64 = 256 << 20;
const MAX_REPLAYS: usize = 2000;
const MAX_REPLAY: u64 = 16 << 20;
const FILES: [(&str, u64); 4] = [
    ("score.dat", 1 << 20),
    ("th08.cfg", 4 << 10),
    ("port.cfg", 64 << 10),
    (MANIFEST, 4 << 10),
];

// Where the save files live: a save dir and config dir natively, localStorage on the web.
pub trait Store {
    // "score.dat", "th08.cfg" or "replay/<name>.rpy".
    fn read(&self, name: &str) -> Option<Vec<u8>>;
    // File names in replay/ (no directory part).
    fn replays(&self) -> Vec<String>;
    fn write(&mut self, name: &str, data: &[u8]) -> Result<(), String>;
    // port.cfg player rows as `key = value` lines.
    fn port_cfg(&self) -> String;
    fn set_port_cfg(&mut self, changes: &[(&'static str, Option<&str>)]) -> Result<(), String>;
    // Keeps the pre-import backup zip; returns where it went.
    fn keep_backup(&mut self, name: &str, zip: &[u8]) -> Result<String, String>;
}

// The game's replay names: plain file names ending in .rpy.
pub fn is_replay_name(n: &str) -> bool {
    let Some(stem) = n
        .len()
        .checked_sub(4)
        .filter(|&i| n.is_char_boundary(i) && n[i..].eq_ignore_ascii_case(".rpy"))
        .map(|i| &n[..i])
    else {
        return false;
    };
    !stem.is_empty()
        && n.len() <= 64
        && !stem.starts_with('.')
        && stem
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
}

pub fn date_stamp(t: &LocalTime) -> String {
    format!("{:04}-{:02}-{:02}", t.year, t.month, t.day)
}

pub fn export_name(t: &LocalTime) -> String {
    format!("th08-saves-{}.zip", date_stamp(t))
}

fn backup_name(t: &LocalTime) -> String {
    format!(
        "th08-saves-backup-{}_{:02}{:02}{:02}.zip",
        date_stamp(t),
        t.hour,
        t.minute,
        t.second
    )
}

fn platform() -> &'static str {
    if cfg!(target_arch = "wasm32") {
        "web"
    } else if cfg!(target_os = "android") {
        "android"
    } else {
        "desktop"
    }
}

fn manifest(t: &LocalTime) -> String {
    serde_json::json!({
        "format": FORMAT,
        "port": env!("TH08_VERSION"),
        "channel": env!("TH08_CHANNEL"),
        "platform": platform(),
        "date": format!("{} {:02}:{:02}:{:02}", date_stamp(t), t.hour, t.minute, t.second),
    })
    .to_string()
}

// The store's files in archive order: score.dat, th08.cfg, replay/*.rpy (sorted), port.cfg if it has player rows.
fn collect(store: &dyn Store) -> Vec<(String, Vec<u8>)> {
    let mut out: Vec<(String, Vec<u8>)> = ["score.dat", "th08.cfg"]
        .iter()
        .filter_map(|n| Some((n.to_string(), store.read(n)?)))
        .collect();
    let mut replays: Vec<String> = store
        .replays()
        .into_iter()
        .filter(|n| is_replay_name(n))
        .collect();
    replays.sort();
    replays.dedup();
    // Unreadable replays stay out, so the archive passes `validate`.
    out.extend(replays.into_iter().filter_map(|n| {
        let name = format!("replay/{n}");
        let data = store.read(&name)?;
        replay_error(&data).is_none().then_some((name, data))
    }));
    let cfg = store.port_cfg();
    if !cfg.is_empty() {
        out.push(("port.cfg".into(), cfg.into_bytes()));
    }
    out
}

fn zip_files(files: &[(String, Vec<u8>)], t: &LocalTime) -> Result<Vec<u8>, String> {
    let date = zip::DateTime::from_date_and_time(
        u16::try_from(t.year.clamp(1980, 2107)).unwrap_or(1980),
        u8::try_from(t.month).unwrap_or(1),
        u8::try_from(t.day).unwrap_or(1),
        u8::try_from(t.hour).unwrap_or(0),
        u8::try_from(t.minute).unwrap_or(0),
        u8::try_from(t.second).unwrap_or(0),
    )
    .unwrap_or_default();
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .last_modified_time(date);
    let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let man = manifest(t);
    let all = std::iter::once((MANIFEST, man.as_bytes()))
        .chain(files.iter().map(|(n, d)| (n.as_str(), d.as_slice())));
    for (name, data) in all {
        w.start_file(name, opts).map_err(|e| e.to_string())?;
        w.write_all(data).map_err(|e| e.to_string())?;
    }
    Ok(w.finish().map_err(|e| e.to_string())?.into_inner())
}

pub struct Exported {
    pub zip: Vec<u8>,
    // Files besides the manifest.
    pub files: usize,
}

pub fn export(store: &dyn Store, t: &LocalTime) -> Result<Exported, String> {
    let files = collect(store);
    Ok(Exported {
        zip: zip_files(&files, t)?,
        files: files.len(),
    })
}

// A checked archive: only known names, each within its size limit.
#[derive(Debug, Default)]
pub struct Archive {
    pub score: Option<Vec<u8>>,
    pub cfg: Option<Vec<u8>>,
    pub replays: Vec<(String, Vec<u8>)>,
    pub port_cfg: Option<String>,
}

fn limit_of(name: &str) -> Option<u64> {
    if let Some(r) = name.strip_prefix("replay/") {
        return is_replay_name(r).then_some(MAX_REPLAY);
    }
    FILES.iter().find(|f| f.0 == name).map(|f| f.1)
}

pub fn validate(data: &[u8]) -> Result<Archive, String> {
    if data.len() as u64 > MAX_ZIP {
        return Err(format!("larger than {} MiB", MAX_ZIP >> 20));
    }
    let mut z = zip::ZipArchive::new(Cursor::new(data)).map_err(|e| format!("not a zip: {e}"))?;
    if z.len() > MAX_REPLAYS + FILES.len() + 1 {
        return Err(format!("{} entries", z.len()));
    }
    let mut a = Archive::default();
    let mut seen = std::collections::HashSet::new();
    let mut total = 0u64;
    let mut manifest = None;
    for i in 0..z.len() {
        let mut f = z.by_index(i).map_err(|e| format!("entry {i}: {e}"))?;
        let name = f
            .name()
            .map_err(|e| format!("entry {i}: {e}"))?
            .into_owned();
        if f.is_dir() {
            if name == "replay/" {
                continue;
            }
            return Err(format!("unexpected folder {name}"));
        }
        let limit = limit_of(&name).ok_or_else(|| format!("unexpected file {name}"))?;
        if f.is_symlink() {
            return Err(format!("{name}: a link"));
        }
        if !seen.insert(name.clone()) {
            return Err(format!("{name} twice"));
        }
        if f.size() > limit {
            return Err(format!("{name}: larger than {limit} bytes"));
        }
        let mut buf = Vec::new();
        (&mut f)
            .take(limit + 1)
            .read_to_end(&mut buf)
            .map_err(|e| format!("{name}: {e}"))?;
        if buf.len() as u64 > limit {
            return Err(format!("{name}: larger than {limit} bytes"));
        }
        total += buf.len() as u64;
        if total > MAX_TOTAL {
            return Err(format!("more than {} MiB unpacked", MAX_TOTAL >> 20));
        }
        match name.as_str() {
            MANIFEST => manifest = Some(buf),
            "score.dat" => a.score = Some(buf),
            "th08.cfg" => a.cfg = Some(buf),
            "port.cfg" => {
                a.port_cfg =
                    Some(String::from_utf8(buf).map_err(|_| "port.cfg: not UTF-8".to_string())?);
            }
            _ => a.replays.push((name["replay/".len()..].to_string(), buf)),
        }
    }
    let m: serde_json::Value = manifest
        .as_deref()
        .and_then(|b| serde_json::from_slice(b).ok())
        .ok_or("no th08 save manifest")?;
    if m["format"].as_u64() != Some(FORMAT) {
        return Err(format!("unknown save format {}", m["format"]));
    }
    if let Some(s) = &a.score {
        th08_core::scoredat::ScoreDat::parse(s, None).map_err(|e| format!("score.dat: {e}"))?;
    }
    if let Some((n, e)) = a
        .replays
        .iter()
        .find_map(|(n, r)| replay_error(r).map(|e| (n, e)))
    {
        return Err(format!("replay/{n}: {e}"));
    }
    Ok(a)
}

// Why a replay cannot be read; a well-formed slow-mode replay is kept (the game itself refuses to play it).
fn replay_error(r: &[u8]) -> Option<th08_core::replay::ReplayError> {
    match th08_core::replay::parse(r, None) {
        Ok(_) | Err(th08_core::replay::ReplayError::SlowMode) => None,
        Err(e) => Some(e),
    }
}

// Player rows of an imported port.cfg that this build knows and accepts; the last line of a key wins.
fn port_cfg_changes(text: &str) -> Vec<(&'static str, String)> {
    let mut out: Vec<(&'static str, String)> = Vec::new();
    for l in text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        let Some((k, v)) = l.split_once('=') else {
            continue;
        };
        let Some(e) = crate::settings::find(k.trim()) else {
            continue;
        };
        if e.audience != crate::settings::Audience::Player || e.check(v.trim()).is_err() {
            continue;
        }
        out.retain(|(key, _)| *key != e.key);
        out.push((e.key, v.trim().to_string()));
    }
    out
}

#[derive(Debug)]
pub struct Imported {
    pub files: usize,
    pub replays: usize,
    pub port_cfg: bool,
    pub backup: Option<String>,
}

// Validates `data` fully, keeps a backup zip of the current files, then writes the archive's files over them.
// port.cfg rows are applied only with `with_port_cfg` (paths like game_dir are machine-specific).
pub fn import(
    store: &mut dyn Store,
    data: &[u8],
    with_port_cfg: bool,
    t: &LocalTime,
) -> Result<Imported, String> {
    let a = validate(data)?;
    let cfg = a
        .port_cfg
        .as_deref()
        .filter(|_| with_port_cfg)
        .map(port_cfg_changes)
        .unwrap_or_default();
    let current = collect(store);
    let backup = if current.is_empty() {
        None
    } else {
        let zip = zip_files(&current, t)?;
        Some(
            store
                .keep_backup(&backup_name(t), &zip)
                .map_err(|e| format!("backup not saved, nothing imported: {e}"))?,
        )
    };
    let fail = |e: String| match &backup {
        Some(b) => format!("{e} (the previous files are in {b})"),
        None => e,
    };
    let mut files = 0;
    for (name, d) in [("score.dat", &a.score), ("th08.cfg", &a.cfg)] {
        if let Some(d) = d {
            store.write(name, d).map_err(fail)?;
            files += 1;
        }
    }
    for (n, d) in &a.replays {
        store.write(&format!("replay/{n}"), d).map_err(fail)?;
    }
    if !cfg.is_empty() {
        let c: Vec<(&'static str, Option<&str>)> =
            cfg.iter().map(|(k, v)| (*k, Some(v.as_str()))).collect();
        store.set_port_cfg(&c).map_err(fail)?;
    }
    Ok(Imported {
        files: files + a.replays.len(),
        replays: a.replays.len(),
        port_cfg: !cfg.is_empty(),
        backup,
    })
}

// Native saves: the save dir (score.dat, th08.cfg, replay/) and the config dir's port.cfg; backups go next to the
// saves.
#[cfg(not(target_arch = "wasm32"))]
pub struct DirStore {
    pub save: std::path::PathBuf,
    pub dirs: crate::settings::PlatformDirs,
}

#[cfg(not(target_arch = "wasm32"))]
impl Store for DirStore {
    fn read(&self, name: &str) -> Option<Vec<u8>> {
        std::fs::read(self.save.join(name)).ok()
    }

    fn replays(&self) -> Vec<String> {
        std::fs::read_dir(self.save.join("replay"))
            .into_iter()
            .flatten()
            .filter_map(|e| {
                let e = e.ok()?;
                e.file_type().ok()?.is_file().then_some(())?;
                e.file_name().into_string().ok()
            })
            .collect()
    }

    fn write(&mut self, name: &str, data: &[u8]) -> Result<(), String> {
        let p = self.save.join(name);
        crate::platform::write_atomic(&p, data).map_err(|e| format!("{}: {e}", p.display()))
    }

    fn port_cfg(&self) -> String {
        crate::settings::player_port_cfg(&self.dirs)
    }

    fn set_port_cfg(&mut self, changes: &[(&'static str, Option<&str>)]) -> Result<(), String> {
        crate::settings::try_write_port_cfg(&self.dirs, changes)
    }

    fn keep_backup(&mut self, name: &str, zip: &[u8]) -> Result<String, String> {
        let p = self.save.join(name);
        crate::platform::write_atomic(&p, zip).map_err(|e| format!("{}: {e}", p.display()))?;
        Ok(p.display().to_string())
    }
}

// The Android launcher's save export / import (Saves.kt): one JSON request naming the app's files dir and a zip
// path in its cache, as the launcher copies the zip to and from the picked document.
#[cfg(not(target_arch = "wasm32"))]
pub mod bridge {
    use serde_json::{Value, json};
    use std::path::PathBuf;

    fn run(r: &Value) -> Result<Value, String> {
        let dir = |k: &str| r[k].as_str().map(PathBuf::from).ok_or(format!("no {k}"));
        let config = dir("config")?;
        let t = crate::platform::Clock::default().local_time();
        let op = r["op"].as_str().unwrap_or("");
        if op == "name" {
            return Ok(json!({"name": super::export_name(&t)}));
        }
        let mut st = super::DirStore {
            save: r["save"]
                .as_str()
                .map_or_else(|| config.clone(), PathBuf::from),
            dirs: crate::settings::PlatformDirs {
                config: Some(config.clone()),
                save: Some(config),
                soundfonts: Vec::new(),
            },
        };
        let zip = dir("zip")?;
        match op {
            "export" => {
                let out = super::export(&st, &t)?;
                crate::platform::write_atomic(&zip, &out.zip).map_err(|e| e.to_string())?;
                Ok(json!({"files": out.files}))
            }
            "import" => {
                let len = std::fs::metadata(&zip).map_err(|e| e.to_string())?.len();
                if len > super::MAX_ZIP {
                    return Err(format!("larger than {} MiB", super::MAX_ZIP >> 20));
                }
                let data = std::fs::read(&zip).map_err(|e| e.to_string())?;
                let with_cfg = r["port_cfg"].as_bool().unwrap_or(false);
                let r = super::import(&mut st, &data, with_cfg, &t)?;
                Ok(
                    json!({"files": r.files, "replays": r.replays, "port_cfg": r.port_cfg,
                          "backup": r.backup}),
                )
            }
            _ => Err(format!("unknown op {op:?}")),
        }
    }

    pub fn handle(req: &str) -> String {
        let r = serde_json::from_str(req)
            .map_err(|e| format!("bad request: {e}"))
            .and_then(|r| run(&r));
        match r {
            Ok(v) => v.to_string(),
            Err(e) => json!({"error": e}).to_string(),
        }
    }

    #[cfg(target_os = "android")]
    #[unsafe(no_mangle)]
    pub extern "system" fn Java_org_th08_port_Saves_call<'c>(
        mut env: jni::EnvUnowned<'c>,
        _class: jni::objects::JClass<'c>,
        req: jni::objects::JString<'c>,
    ) -> jni::objects::JString<'c> {
        env.with_env(|env| -> jni::errors::Result<jni::objects::JString<'c>> {
            let r = req.try_to_string(env)?;
            jni::objects::JString::from_str(env, handle(&r))
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
    }
}

// Web saves: the localStorage items the game writes, port.cfg rows as `th08.<key>` items; files go out as
// downloads.
#[cfg(target_arch = "wasm32")]
pub struct WebStore {
    pub storage: crate::platform::Storage,
    pub dirs: crate::settings::PlatformDirs,
}

#[cfg(target_arch = "wasm32")]
impl Store for WebStore {
    fn read(&self, name: &str) -> Option<Vec<u8>> {
        self.storage.read(name)
    }

    fn replays(&self) -> Vec<String> {
        self.storage
            .list("replay/")
            .into_iter()
            .filter_map(|n| n.strip_prefix("replay/").map(str::to_string))
            .collect()
    }

    fn write(&mut self, name: &str, data: &[u8]) -> Result<(), String> {
        self.storage.try_write(name, data)
    }

    fn port_cfg(&self) -> String {
        crate::settings::player_port_cfg(&self.dirs)
    }

    fn set_port_cfg(&mut self, changes: &[(&'static str, Option<&str>)]) -> Result<(), String> {
        crate::settings::try_write_port_cfg(&self.dirs, changes)
    }

    fn keep_backup(&mut self, name: &str, zip: &[u8]) -> Result<String, String> {
        download(name, zip)?;
        Ok(name.to_string())
    }
}

#[cfg(target_arch = "wasm32")]
fn js_err(e: &wasm_bindgen::JsValue) -> String {
    e.as_string().unwrap_or_else(|| format!("{e:?}"))
}

// Hands `data` to the browser as a file download named `name`.
#[cfg(target_arch = "wasm32")]
pub fn download(name: &str, data: &[u8]) -> Result<(), String> {
    use wasm_bindgen::JsCast;
    let w = web_sys::window().ok_or("no window")?;
    let doc = w.document().ok_or("no document")?;
    let parts = js_sys::Array::of1(&js_sys::Uint8Array::from(data));
    let opts = web_sys::BlobPropertyBag::new();
    opts.set_type("application/zip");
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &opts)
        .map_err(|e| js_err(&e))?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(|e| js_err(&e))?;
    let a: web_sys::HtmlAnchorElement = doc
        .create_element("a")
        .map_err(|e| js_err(&e))?
        .dyn_into()
        .map_err(|_| "no anchor element")?;
    a.set_href(&url);
    a.set_download(name);
    a.click();
    // The URL stays valid until the download has started.
    let revoke = js_sys::Function::new_with_args("u", "URL.revokeObjectURL(u)");
    let _ = w.set_timeout_with_callback_and_timeout_and_arguments_1(&revoke, 60_000, &url.into());
    Ok(())
}

// A zip picked in the browser's file dialog, read asynchronously; the screen polls `take`.
#[cfg(target_arch = "wasm32")]
#[derive(Clone, Default)]
pub struct Picker(std::sync::Arc<std::sync::Mutex<PickState>>);

#[cfg(target_arch = "wasm32")]
#[derive(Default)]
enum PickState {
    #[default]
    Idle,
    Open,
    Done(Result<Vec<u8>, String>),
}

#[cfg(target_arch = "wasm32")]
impl Picker {
    pub fn open(&self) -> Result<(), String> {
        use wasm_bindgen::JsCast;
        use wasm_bindgen::closure::Closure;
        let doc = web_sys::window()
            .and_then(|w| w.document())
            .ok_or("no document")?;
        let input: web_sys::HtmlInputElement = doc
            .create_element("input")
            .map_err(|e| js_err(&e))?
            .dyn_into()
            .map_err(|_| "no input element")?;
        input.set_type("file");
        input.set_accept(".zip,application/zip");
        let slot = self.0.clone();
        let el = input.clone();
        let change = Closure::<dyn FnMut()>::new(move || {
            let Some(file) = el.files().and_then(|f| f.get(0)) else {
                *slot.lock().unwrap() = PickState::Idle;
                return;
            };
            if file.size() > MAX_ZIP as f64 {
                *slot.lock().unwrap() =
                    PickState::Done(Err(format!("larger than {} MiB", MAX_ZIP >> 20)));
                return;
            }
            let slot = slot.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let r = wasm_bindgen_futures::JsFuture::from(file.array_buffer())
                    .await
                    .map(|b| js_sys::Uint8Array::new(&b).to_vec())
                    .map_err(|e| js_err(&e));
                *slot.lock().unwrap() = PickState::Done(r);
            });
        });
        let slot = self.0.clone();
        let cancel = Closure::<dyn FnMut()>::new(move || {
            *slot.lock().unwrap() = PickState::Idle;
        });
        input.set_onchange(Some(change.as_ref().unchecked_ref()));
        input
            .add_event_listener_with_callback("cancel", cancel.as_ref().unchecked_ref())
            .map_err(|e| js_err(&e))?;
        change.forget();
        cancel.forget();
        *self.0.lock().unwrap() = PickState::Open;
        input.click();
        Ok(())
    }

    pub fn waiting(&self) -> bool {
        matches!(*self.0.lock().unwrap(), PickState::Open)
    }

    pub fn take(&self) -> Option<Result<Vec<u8>, String>> {
        let mut g = self.0.lock().unwrap();
        match std::mem::take(&mut *g) {
            PickState::Done(r) => Some(r),
            other => {
                *g = other;
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::PlatformDirs;
    use std::path::{Path, PathBuf};

    const T: LocalTime = LocalTime {
        year: 2026,
        month: 11,
        day: 5,
        hour: 9,
        minute: 8,
        second: 7,
    };

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("th08-saves-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    // A real score.dat whose LSNM name is `tag`, and a real replay whose user block is `tag`.
    fn score(tag: &[u8]) -> Vec<u8> {
        let mut s = th08_core::scoredat::ScoreDat::default();
        s.lsnm[..tag.len()].copy_from_slice(tag);
        s.encode(&mut th08_core::rng::Rng::seeded(1))
    }

    fn rpy(tag: &[u8]) -> Vec<u8> {
        let mut r = th08_core::replay::Replay::default();
        r.stages[0] = Some(th08_core::replay::StageRecord {
            inputs: vec![0, 1],
            ..Default::default()
        });
        r.user = tag.to_vec();
        r.encode(&mut th08_core::rng::Rng::seeded(1))
    }

    fn store(root: &Path) -> DirStore {
        DirStore {
            save: root.join("save"),
            dirs: PlatformDirs {
                config: Some(root.join("config")),
                save: Some(root.join("save")),
                soundfonts: Vec::new(),
            },
        }
    }

    fn fill(root: &Path, tag: &[u8]) {
        let s = root.join("save");
        std::fs::create_dir_all(s.join("replay")).unwrap();
        std::fs::write(s.join("score.dat"), score(tag)).unwrap();
        std::fs::write(s.join("th08.cfg"), [b"cfg-", tag].concat()).unwrap();
        std::fs::write(s.join("replay/th8_01.rpy"), rpy(&[b"r1-", tag].concat())).unwrap();
        std::fs::write(s.join("replay/notes.txt"), b"x").unwrap();
        std::fs::create_dir_all(root.join("config")).unwrap();
        std::fs::write(
            root.join("config/port.cfg"),
            "# mine\nscale = 3\ngod = on\ngame_dir = /games/th08\n",
        )
        .unwrap();
    }

    fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let o = zip::write::SimpleFileOptions::default();
        for (n, d) in entries {
            w.start_file(*n, o).unwrap();
            w.write_all(d).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    fn snapshot(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        let mut v = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(d).unwrap() {
                let p = e.unwrap().path();
                if p.is_dir() {
                    stack.push(p);
                } else {
                    v.push((p.clone(), std::fs::read(&p).unwrap()));
                }
            }
        }
        v.sort();
        v
    }

    #[test]
    fn replay_names() {
        assert!(is_replay_name("th8_01.rpy"));
        assert!(is_replay_name("th8_udAbc1.RPY"));
        for n in [
            "../x.rpy", "a/b.rpy", ".rpy", ".x.rpy", "x.txt", "a\\b.rpy", "ö.rpy", "",
        ] {
            assert!(!is_replay_name(n), "{n}");
        }
    }

    #[test]
    fn round_trip() {
        let a = tmp("a");
        fill(&a, b"A");
        let out = export(&store(&a), &T).unwrap();
        assert_eq!(out.files, 4);
        assert_eq!(export_name(&T), "th08-saves-2026-11-05.zip");
        let arc = validate(&out.zip).unwrap();
        assert_eq!(arc.replays.len(), 1);
        let cfg = arc.port_cfg.unwrap();
        assert!(cfg.contains("scale = 3") && cfg.contains("game_dir = /games/th08"));
        assert!(!cfg.contains("god"), "developer keys stay out");

        let b = tmp("b");
        let mut sb = store(&b);
        let r = import(&mut sb, &out.zip, false, &T).unwrap();
        assert_eq!((r.files, r.replays, r.port_cfg), (3, 1, false));
        assert!(r.backup.is_none(), "nothing to back up");
        assert_eq!(
            std::fs::read(b.join("save/score.dat")).unwrap(),
            score(b"A")
        );
        assert_eq!(std::fs::read(b.join("save/th08.cfg")).unwrap(), b"cfg-A");
        assert_eq!(
            std::fs::read(b.join("save/replay/th8_01.rpy")).unwrap(),
            rpy(b"r1-A")
        );
        assert!(!b.join("config/port.cfg").exists());

        let r = import(&mut sb, &out.zip, true, &T).unwrap();
        assert!(r.port_cfg);
        let pc = std::fs::read_to_string(b.join("config/port.cfg")).unwrap();
        assert!(pc.contains("scale = 3") && pc.contains("game_dir = /games/th08"));
        assert_eq!(export(&sb, &T).unwrap().files, 4);
        let _ = std::fs::remove_dir_all(&a);
        let _ = std::fs::remove_dir_all(&b);
    }

    #[test]
    fn backup_before_import() {
        let a = tmp("src");
        fill(&a, b"A");
        let zip = export(&store(&a), &T).unwrap().zip;
        let b = tmp("dst");
        fill(&b, b"B");
        std::fs::write(b.join("save/replay/th8_02.rpy"), rpy(b"r2-B")).unwrap();
        let r = import(&mut store(&b), &zip, false, &T).unwrap();
        let bk = PathBuf::from(r.backup.unwrap());
        assert_eq!(bk, b.join("save/th08-saves-backup-2026-11-05_090807.zip"));
        let old = validate(&std::fs::read(&bk).unwrap()).unwrap();
        assert_eq!(old.score.as_deref(), Some(&score(b"B")[..]));
        assert_eq!(old.cfg.as_deref(), Some(&b"cfg-B"[..]));
        assert_eq!(old.replays.len(), 2);
        assert_eq!(
            std::fs::read(b.join("save/score.dat")).unwrap(),
            score(b"A")
        );
        // Replays not in the archive stay.
        assert_eq!(
            std::fs::read(b.join("save/replay/th8_02.rpy")).unwrap(),
            rpy(b"r2-B")
        );
        let _ = std::fs::remove_dir_all(&a);
        let _ = std::fs::remove_dir_all(&b);
    }

    #[test]
    fn bad_zips_leave_saves_untouched() {
        let man = manifest(&T);
        let m = man.as_bytes();
        let big = vec![0u8; (1 << 20) + 1];
        let mut body = vec![0u8; 0x1C];
        body.extend_from_slice(b"TH8K\x01\0\x01\0\x01\0\0\0");
        let short_chunk = th08_core::scoredat::encode_container(&body, [0, 0]);
        let cases: Vec<(&str, Vec<u8>)> = vec![
            (
                "traversal",
                zip_of(&[(MANIFEST, m), ("../score.dat", b"x")]),
            ),
            (
                "replay traversal",
                zip_of(&[(MANIFEST, m), ("replay/../../x.rpy", b"x")]),
            ),
            (
                "absolute",
                zip_of(&[(MANIFEST, m), ("/etc/score.dat", b"x")]),
            ),
            ("unknown", zip_of(&[(MANIFEST, m), ("evil.exe", b"x")])),
            (
                "unknown dir",
                zip_of(&[(MANIFEST, m), ("replay/sub/a.rpy", b"x")]),
            ),
            ("oversize", zip_of(&[(MANIFEST, m), ("score.dat", &big)])),
            ("no manifest", zip_of(&[("score.dat", b"x")])),
            (
                "bad format",
                zip_of(&[(MANIFEST, br#"{"format":9}"#), ("score.dat", b"x")]),
            ),
            ("not a zip", b"PK\x03\x04 nope".to_vec()),
            (
                "bad score.dat",
                zip_of(&[(MANIFEST, m), ("score.dat", b"score-X")]),
            ),
            (
                "short chunk score.dat",
                zip_of(&[(MANIFEST, m), ("score.dat", &short_chunk)]),
            ),
            (
                "bad replay",
                zip_of(&[(MANIFEST, m), ("replay/th8_01.rpy", b"T8RP nope")]),
            ),
        ];
        let root = tmp("bad");
        fill(&root, b"B");
        let before = snapshot(&root);
        for (what, z) in &cases {
            let e = import(&mut store(&root), z, true, &T);
            assert!(e.is_err(), "{what} accepted");
            assert_eq!(snapshot(&root), before, "{what} changed the saves");
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn bridge_round_trip() {
        let a = tmp("bra");
        fill(&a, b"A");
        let call = |v: serde_json::Value| -> serde_json::Value {
            serde_json::from_str(&bridge::handle(&v.to_string())).unwrap()
        };
        let zip = a.join("out.zip");
        let save = a.join("save");
        let cfg = a.join("config");
        let v = call(serde_json::json!({"op": "export", "config": cfg, "save": save, "zip": zip}));
        assert_eq!(v["files"], 4, "{v}");
        let b = tmp("brb");
        let v =
            call(serde_json::json!({"op": "import", "config": b, "zip": zip, "port_cfg": true}));
        assert_eq!(
            (v["files"].as_u64(), v["port_cfg"].as_bool()),
            (Some(3), Some(true)),
            "{v}"
        );
        assert_eq!(std::fs::read(b.join("score.dat")).unwrap(), score(b"A"));
        assert!(
            std::fs::read_to_string(b.join("port.cfg"))
                .unwrap()
                .contains("scale = 3")
        );
        let v = call(serde_json::json!({"op": "import", "config": b, "zip": b.join("score.dat")}));
        assert!(v["error"].as_str().unwrap().contains("not a zip"), "{v}");
        assert!(
            call(serde_json::json!({"op": "name", "config": b}))["name"]
                .as_str()
                .unwrap()
                .starts_with("th08-saves-")
        );
        assert!(bridge::handle("nope").contains("bad request"));
        let _ = std::fs::remove_dir_all(&a);
        let _ = std::fs::remove_dir_all(&b);
    }
}
