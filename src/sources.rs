//! Concrete game data sources (ARCH_D_MOBILE §2.3): the game directory (desktop / Android external dir) and the
//! files picked in the web page; app side, implementing `assets::DataSource`.

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
