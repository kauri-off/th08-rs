//! Port I/O layer: reads th08.dat / thbgm.dat from the game directory (native) or user-picked files (web)
//! (00 §1 data, 01 §7); the original loads synchronously or on a loader thread (00 §6).

use anyhow::Result;

#[derive(Clone)]
pub enum Source {
    #[cfg(not(target_arch = "wasm32"))]
    Native(std::path::PathBuf),
    #[cfg(target_arch = "wasm32")]
    Web(std::rc::Rc<Vec<web_sys::File>>),
}

impl Source {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn native(dir: std::path::PathBuf) -> Self {
        Source::Native(dir)
    }

    #[cfg(target_arch = "wasm32")]
    pub fn web(files: Vec<web_sys::File>) -> Self {
        Source::Web(std::rc::Rc::new(files))
    }

    // Names of the files picked in the web page (launcher screen).
    #[cfg(target_arch = "wasm32")]
    pub fn file_names(&self) -> Vec<String> {
        let Source::Web(files) = self;
        files.iter().map(|f| f.name()).collect()
    }

    pub async fn read(&self, name: &str) -> Result<Vec<u8>> {
        self.fetch(name, None).await
    }

    // Byte-range read, used to stream thbgm.dat tracks (02 §1) without loading the whole file.
    pub async fn read_range(&self, name: &str, offset: u64, len: usize) -> Result<Vec<u8>> {
        self.fetch(name, Some((offset, len))).await
    }

    #[cfg(not(target_arch = "wasm32"))]
    async fn fetch(&self, name: &str, range: Option<(u64, usize)>) -> Result<Vec<u8>> {
        use std::io::{Read, Seek, SeekFrom};
        let Source::Native(dir) = self;
        let path = find_file(dir, name);
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
    }

    #[cfg(target_arch = "wasm32")]
    async fn fetch(&self, name: &str, range: Option<(u64, usize)>) -> Result<Vec<u8>> {
        use wasm_bindgen_futures::JsFuture;
        let Source::Web(files) = self;
        let err = |e: wasm_bindgen::JsValue| anyhow::anyhow!("{e:?}");
        let file = files
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
