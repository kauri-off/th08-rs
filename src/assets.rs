//! Port I/O layer: where th08.dat / thbgm.dat bytes come from (00 §1 data, 01 §7); app side, not core
//! (ARCH_D_MOBILE §2.3). The original loads synchronously or on a loader thread (00 §6).

use anyhow::Result;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

// Native: sources and their reads cross to the loader thread (`spawn`); web: single-threaded JS objects.
#[cfg(not(target_arch = "wasm32"))]
pub trait MaybeSend: Send + Sync {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Send + Sync + ?Sized> MaybeSend for T {}
#[cfg(target_arch = "wasm32")]
pub trait MaybeSend {}
#[cfg(target_arch = "wasm32")]
impl<T: ?Sized> MaybeSend for T {}

#[cfg(not(target_arch = "wasm32"))]
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
#[cfg(target_arch = "wasm32")]
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

// A game data location (ARCH_D_MOBILE §2.3): game directory, APK assets (later), user-picked web files.
// Names are bare file names, matched case-insensitively as on Windows (01 §5, §7).
pub trait DataSource: MaybeSend {
    // File names available (launcher folder check / web file list).
    fn names(&self) -> Vec<String>;
    // Whole file (`range` None) or `len` bytes at `off`.
    fn fetch<'a>(
        &'a self,
        name: &'a str,
        range: Option<(u64, usize)>,
    ) -> BoxFuture<'a, Result<Vec<u8>>>;
}

// Shared handle over one `DataSource`; cloned into the audio streamer.
#[derive(Clone)]
pub struct Source(Arc<dyn DataSource>);

impl Source {
    pub fn new(src: impl DataSource + 'static) -> Self {
        Source(Arc::new(src))
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn native(dir: std::path::PathBuf) -> Self {
        Self::new(DirSource(dir))
    }

    #[cfg(target_arch = "wasm32")]
    pub fn web(files: Vec<web_sys::File>) -> Self {
        Self::new(WebFiles(files))
    }

    pub fn names(&self) -> Vec<String> {
        self.0.names()
    }

    pub async fn read(&self, name: &str) -> Result<Vec<u8>> {
        self.0.fetch(name, None).await
    }

    // Byte-range read, used to stream thbgm.dat tracks (02 §1) without loading the whole file.
    pub async fn read_range(&self, name: &str, offset: u64, len: usize) -> Result<Vec<u8>> {
        self.0.fetch(name, Some((offset, len))).await
    }
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

// Background loader, standing in for the original's loader threads (00 §3.2, §6).
#[cfg(not(target_arch = "wasm32"))]
pub fn spawn(fut: impl std::future::Future<Output = ()> + Send + 'static) {
    std::thread::spawn(move || pollster::block_on(fut));
}

#[cfg(target_arch = "wasm32")]
pub fn spawn(fut: impl std::future::Future<Output = ()> + 'static) {
    wasm_bindgen_futures::spawn_local(fut);
}
