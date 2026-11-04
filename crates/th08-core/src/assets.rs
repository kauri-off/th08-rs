//! Game data source traits: where th08.dat / thbgm.dat bytes come from; the concrete sources are in `sources.rs`.

use anyhow::Result;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

// Native: sources and their reads cross to the loader thread (`audio::spawn`); web: single-threaded JS objects.
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

// A game data location: game directory, APK assets (later), user-picked web files.
// Names are bare file names, matched case-insensitively as on Windows.
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

    pub fn names(&self) -> Vec<String> {
        self.0.names()
    }

    pub async fn read(&self, name: &str) -> Result<Vec<u8>> {
        self.0.fetch(name, None).await
    }

    // Byte-range read, used to stream thbgm.dat tracks without loading the whole file.
    pub async fn read_range(&self, name: &str, offset: u64, len: usize) -> Result<Vec<u8>> {
        self.0.fetch(name, Some((offset, len))).await
    }
}
