//! score.dat (15 §2): the boot-loaded version list for ExeCheck and the port's save storage; the codec and records
//! are th08_formats::scoredat.

pub use th08_formats::scoredat::*;

// Supervisor+0x360 / +0x35C: the version list buffer (15 §2.10); unset → ExeCheck step 1 (pass).
static VER_LIST: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();

// Supervisor_OnRegister 0x4461A0: keeps the list for the whole run (the archive copy is the same for every caller).
pub fn set_ver_list(list: Vec<u8>) {
    let _ = VER_LIST.set(list);
}

pub fn ver_list() -> Option<&'static [u8]> {
    VER_LIST.get().map(Vec::as_slice)
}

// ExeCheck 0x44858D (15 §2.10) against the boot-loaded list: true = pass (0), false = fail (−1).
pub fn exe_check(tag: &[u8], size: u32, sum: u32) -> bool {
    exe_check_in(ver_list(), tag, size, sum)
}

// ScoreDat_Load chunk walk (15 §2.1) with the boot-loaded version list.
pub fn parse(file: &[u8]) -> Result<ScoreDat> {
    ScoreDat::parse(file, ver_list())
}

// Port save storage (not a port of the original, which writes next to th08.exe, 00 §1): never the game directory. Native: `TH08_SAVE_DIR`, else the port config dir (settings.rs),
// else next to the binary. Web: localStorage (hex).
#[cfg(not(target_arch = "wasm32"))]
thread_local! {
    static SAVE_DIR: std::cell::RefCell<Option<std::path::PathBuf>> = const { std::cell::RefCell::new(None) };
}

// Tools/tests: a private save directory for this thread (overrides `TH08_SAVE_DIR`).
#[cfg(not(target_arch = "wasm32"))]
pub fn set_save_dir(dir: Option<std::path::PathBuf>) {
    SAVE_DIR.with_borrow_mut(|d| *d = dir);
}

#[cfg(not(target_arch = "wasm32"))]
fn save_dir() -> Option<std::path::PathBuf> {
    if let Some(d) = SAVE_DIR.with_borrow(|d| d.clone()) {
        return Some(d);
    }
    let s = crate::settings::get();
    s.save_dir
        .clone()
        .or_else(crate::settings::config_dir)
        .or_else(crate::settings::exe_dir)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn storage_read(name: &str) -> Option<Vec<u8>> {
    std::fs::read(save_dir()?.join(name)).ok()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn storage_write(name: &str, data: &[u8]) {
    let Some(dir) = save_dir() else { return };
    let path = dir.join(name);
    if let Some(p) = path.parent() {
        let _ = std::fs::create_dir_all(p);
    }
    if let Err(e) = std::fs::write(&path, data) {
        log::warn!("{}: {e}", path.display());
    }
}

// Result state 12 creates the directory `replay` (16 §2.6); web storage has no directories.
#[cfg(not(target_arch = "wasm32"))]
pub fn storage_mkdir(name: &str) {
    if let Some(dir) = save_dir() {
        let _ = std::fs::create_dir_all(dir.join(name));
    }
}

#[cfg(target_arch = "wasm32")]
pub fn storage_mkdir(_: &str) {}

#[cfg(not(target_arch = "wasm32"))]
pub fn storage_list(prefix: &str) -> Vec<String> {
    let Some(dir) = save_dir() else {
        return Vec::new();
    };
    let (sub, file) = prefix.rsplit_once('/').unwrap_or(("", prefix));
    let Ok(rd) = std::fs::read_dir(dir.join(sub)) else {
        return Vec::new();
    };
    rd.filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.starts_with(file))
        .map(|n| {
            if sub.is_empty() {
                n
            } else {
                format!("{sub}/{n}")
            }
        })
        .collect()
}

#[cfg(target_arch = "wasm32")]
fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

#[cfg(target_arch = "wasm32")]
pub fn storage_read(name: &str) -> Option<Vec<u8>> {
    let s = local_storage()?.get_item(&format!("th08:{name}")).ok()??;
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).ok())
        .collect()
}

#[cfg(target_arch = "wasm32")]
pub fn storage_write(name: &str, data: &[u8]) {
    use std::fmt::Write;
    let mut s = String::with_capacity(data.len() * 2);
    for b in data {
        let _ = write!(s, "{b:02x}");
    }
    if let Some(ls) = local_storage() {
        let _ = ls.set_item(&format!("th08:{name}"), &s);
    }
}

#[cfg(target_arch = "wasm32")]
pub fn storage_list(prefix: &str) -> Vec<String> {
    let Some(ls) = local_storage() else {
        return Vec::new();
    };
    let n = ls.length().unwrap_or(0);
    (0..n)
        .filter_map(|i| ls.key(i).ok()?)
        .filter_map(|k| k.strip_prefix("th08:").map(str::to_string))
        .filter(|k| k.starts_with(prefix))
        .collect()
}
