//! Concrete game data sources (ARCH_D_MOBILE §2.1, §2.3): the game directory (desktop / Android external dir), the
//! APK's bundled assets (Android) and the files picked in the web page; app side, implementing `assets::DataSource`.

use anyhow::Result;
use th08_core::assets::{BoxFuture, DataSource, Source};

// Game directory as a `Source`.
#[cfg(not(target_arch = "wasm32"))]
pub fn native(dir: std::path::PathBuf) -> Source {
    Source::new(DirSource(dir))
}

// The web page's picked files as a `Source`.
#[cfg(target_arch = "wasm32")]
pub fn web(files: Vec<web_sys::File>) -> Source {
    Source::new(WebFiles(files))
}

// Desktop / Android external dir: loose files in one directory.
#[cfg(not(target_arch = "wasm32"))]
pub struct DirSource(pub std::path::PathBuf);

#[cfg(not(target_arch = "wasm32"))]
impl DataSource for DirSource {
    fn names(&self) -> Vec<String> {
        std::fs::read_dir(&self.0)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect()
    }

    fn fetch<'a>(
        &'a self,
        name: &'a str,
        range: Option<(u64, usize)>,
    ) -> BoxFuture<'a, Result<Vec<u8>>> {
        Box::pin(async move {
            use std::io::{Read, Seek, SeekFrom};
            let path = find_file(&self.0, name);
            Ok(match range {
                None => std::fs::read(path)?,
                Some((off, len)) => {
                    let mut f = std::fs::File::open(path)?;
                    f.seek(SeekFrom::Start(off))?;
                    let mut buf = vec![0; len];
                    f.read_exact(&mut buf)?;
                    buf
                }
            })
        })
    }
}

// Android: th08.dat / thbgm.dat bundled as stored (uncompressed) APK assets (ARCH_D §2.1). Each entry is opened once
// as `(fd, start, len)` through `AAsset_openFileDescriptor64`; reads are `pread(fd, start + off)`, so the 450 MB
// thbgm.dat is never read whole.
#[cfg(target_os = "android")]
pub struct ApkAssetSource {
    mgr: winit::platform::android::activity::ndk::asset::AssetManager,
    names: Vec<String>,
    open: std::sync::Mutex<std::collections::HashMap<String, std::sync::Arc<ApkEntry>>>,
}

#[cfg(target_os = "android")]
struct ApkEntry {
    file: std::fs::File,
    start: u64,
    len: u64,
}

#[cfg(target_os = "android")]
impl ApkAssetSource {
    // The APK's root `assets/` as a source, or None when it holds no th08.dat (APK without bundled data).
    pub fn new(mgr: winit::platform::android::activity::ndk::asset::AssetManager) -> Option<Self> {
        let names: Vec<String> = mgr
            .open_dir(c"")?
            .map(|n| n.to_string_lossy().into_owned())
            .collect();
        names
            .iter()
            .any(|n| n.eq_ignore_ascii_case("th08.dat"))
            .then(|| ApkAssetSource {
                mgr,
                names,
                open: Default::default(),
            })
    }

    pub fn names_list(&self) -> &[String] {
        &self.names
    }

    fn entry(&self, name: &str) -> Result<std::sync::Arc<ApkEntry>> {
        let real = self
            .names
            .iter()
            .find(|n| n.eq_ignore_ascii_case(name))
            .ok_or_else(|| anyhow::anyhow!("{name} is not bundled in the APK"))?;
        let mut open = self.open.lock().unwrap();
        if let Some(e) = open.get(real) {
            return Ok(e.clone());
        }
        let c = std::ffi::CString::new(real.as_str())?;
        let asset = self
            .mgr
            .open(&c)
            .ok_or_else(|| anyhow::anyhow!("cannot open APK asset {real}"))?;
        // Fails for a compressed entry: build_apk.sh stores .dat files (`aapt2 -0 .dat`).
        let fd = asset
            .open_file_descriptor()
            .map_err(|e| anyhow::anyhow!("APK asset {real} is not stored uncompressed: {e}"))?;
        let e = std::sync::Arc::new(ApkEntry {
            file: fd.fd.into(),
            start: fd.offset as u64,
            len: fd.size as u64,
        });
        log::info!("APK asset {real}: {} bytes at {}", e.len, e.start);
        open.insert(real.clone(), e.clone());
        Ok(e)
    }
}

#[cfg(target_os = "android")]
impl DataSource for ApkAssetSource {
    fn names(&self) -> Vec<String> {
        self.names.clone()
    }

    fn fetch<'a>(
        &'a self,
        name: &'a str,
        range: Option<(u64, usize)>,
    ) -> BoxFuture<'a, Result<Vec<u8>>> {
        Box::pin(async move {
            use std::os::unix::fs::FileExt;
            let e = self.entry(name)?;
            let (off, len) = range.unwrap_or((0, e.len as usize));
            anyhow::ensure!(
                off + len as u64 <= e.len,
                "{name}: read {len} bytes at {off} past its end ({})",
                e.len
            );
            let mut buf = vec![0; len];
            e.file.read_exact_at(&mut buf, e.start + off)?;
            Ok(buf)
        })
    }
}

// Web: the files picked in the page (OPFS cache, index.html), read with `File.slice`.
#[cfg(target_arch = "wasm32")]
pub struct WebFiles(pub Vec<web_sys::File>);

#[cfg(target_arch = "wasm32")]
impl DataSource for WebFiles {
    fn names(&self) -> Vec<String> {
        self.0.iter().map(|f| f.name()).collect()
    }

    fn fetch<'a>(
        &'a self,
        name: &'a str,
        range: Option<(u64, usize)>,
    ) -> BoxFuture<'a, Result<Vec<u8>>> {
        Box::pin(async move {
            use wasm_bindgen_futures::JsFuture;
            let err = |e: wasm_bindgen::JsValue| anyhow::anyhow!("{e:?}");
            let file = self
                .0
                .iter()
                // Web: files are matched by name only, case-insensitively.
                .find(|f| f.name().eq_ignore_ascii_case(name))
                .ok_or_else(|| anyhow::anyhow!("{name} was not provided"))?;
            let blob = match range {
                None => file.slice().map_err(err)?,
                Some((off, len)) => file
                    .slice_with_f64_and_f64(off as f64, (off + len as u64) as f64)
                    .map_err(err)?,
            };
            let buf = JsFuture::from(blob.array_buffer()).await.map_err(err)?;
            Ok(js_sys::Uint8Array::new(&buf).to_vec())
        })
    }
}

// Game file lookup: exact name first, else a case-insensitive match in `dir` (as on Windows and the web);
// a missing file gives `dir/name` so the read error names it.
#[cfg(not(target_arch = "wasm32"))]
pub fn find_file(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
    let exact = dir.join(name);
    if exact.exists() {
        return exact;
    }
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(name))
        .map_or(exact, |e| e.path())
}
