//! score.dat (15 §2): the boot-loaded version list for ExeCheck and the port's save storage; the codec and records
//! are th08_formats::scoredat.

pub use th08_formats::scoredat::*;

// Port host IO state, passed in by the app / tools (`Settings::host_io`) and held by `Ctx::host`: the save storage
// directory and the boot-loaded version list. Step 7 turns it into the `Host` trait (ARCH_SUMMARY decision 3).
#[derive(Clone, Debug, Default)]
pub struct HostIo {
    // Port save storage (not a port of the original, which writes next to th08.exe, 00 §1): never the game directory.
    // Native only; None = no saves. Web: localStorage (hex).
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    save_dir: Option<std::path::PathBuf>,
    // Supervisor+0x360 / +0x35C: the version list buffer (15 §2.10), kept for the whole run (Supervisor_OnRegister
    // 0x4461A0; the archive copy is the same for every caller); unset → ExeCheck step 1 (pass).
    ver_list: Option<Vec<u8>>,
}

impl HostIo {
    pub fn new(save_dir: Option<std::path::PathBuf>) -> Self {
        Self {
            save_dir,
            ver_list: None,
        }
    }

    // Supervisor_OnRegister 0x4461A0: set once at boot.
    pub fn set_ver_list(&mut self, list: Vec<u8>) {
        if self.ver_list.is_none() {
            self.ver_list = Some(list);
        }
    }

    pub fn ver_list(&self) -> Option<&[u8]> {
        self.ver_list.as_deref()
    }

    // ExeCheck 0x44858D (15 §2.10) against the boot-loaded list: true = pass (0), false = fail (−1).
    pub fn exe_check(&self, tag: &[u8], size: u32, sum: u32) -> bool {
        exe_check_in(self.ver_list(), tag, size, sum)
    }

    // ScoreDat_Load chunk walk (15 §2.1) with the boot-loaded version list.
    pub fn parse(&self, file: &[u8]) -> Result<ScoreDat> {
        ScoreDat::parse(file, self.ver_list())
    }

    // Tools/tests: a private save directory.
    pub fn set_save_dir(&mut self, dir: Option<std::path::PathBuf>) {
        self.save_dir = dir;
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl HostIo {
    pub fn read(&self, name: &str) -> Option<Vec<u8>> {
        std::fs::read(self.save_dir.as_ref()?.join(name)).ok()
    }

    pub fn write(&self, name: &str, data: &[u8]) {
        let Some(dir) = &self.save_dir else { return };
        let path = dir.join(name);
        if let Some(p) = path.parent() {
            let _ = std::fs::create_dir_all(p);
        }
        if let Err(e) = std::fs::write(&path, data) {
            log::warn!("{}: {e}", path.display());
        }
    }

    // Result state 12 creates the directory `replay` (16 §2.6); web storage has no directories.
    pub fn mkdir(&self, name: &str) {
        if let Some(dir) = &self.save_dir {
            let _ = std::fs::create_dir_all(dir.join(name));
        }
    }

    pub fn list(&self, prefix: &str) -> Vec<String> {
        let Some(dir) = &self.save_dir else {
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
}

#[cfg(target_arch = "wasm32")]
fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

#[cfg(target_arch = "wasm32")]
impl HostIo {
    pub fn read(&self, name: &str) -> Option<Vec<u8>> {
        let s = local_storage()?.get_item(&format!("th08:{name}")).ok()??;
        (0..s.len() / 2)
            .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).ok())
            .collect()
    }

    pub fn write(&self, name: &str, data: &[u8]) {
        use std::fmt::Write;
        let mut s = String::with_capacity(data.len() * 2);
        for b in data {
            let _ = write!(s, "{b:02x}");
        }
        if let Some(ls) = local_storage() {
            let _ = ls.set_item(&format!("th08:{name}"), &s);
        }
    }

    pub fn mkdir(&self, _: &str) {}

    pub fn list(&self, prefix: &str) -> Vec<String> {
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
}
