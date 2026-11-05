//! th08.pak / thbgm.pak built from th08.dat / thbgm.dat, shared by the repack tool and the Android import.

use std::sync::atomic::{AtomicBool, Ordering};

pub mod bgm;
pub mod content;
#[cfg(feature = "device")]
pub mod import;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    NotGameData(String),
    #[error("th08.dat is a different version of the game: {} entries differ ({})", .0.len(), .0.join(", "))]
    Version(Vec<String>),
    #[error("not enough free space")]
    NoSpace,
    #[error("cancelled")]
    Cancelled,
    #[error("{0}")]
    Io(std::io::Error),
    #[error("{0}")]
    Failed(String),
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        if e.kind() == std::io::ErrorKind::StorageFull {
            Self::NoSpace
        } else {
            Self::Io(e)
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Content,
    Bgm,
}

impl Stage {
    pub fn name(self) -> &'static str {
        match self {
            Self::Content => "content",
            Self::Bgm => "bgm",
        }
    }
}

type OnProgress<'a> = &'a (dyn Fn(Stage, u64, u64) + Sync);

// Cancel flag and progress sink (stage, done, total) of one conversion.
#[derive(Clone, Copy)]
pub struct Ctl<'a> {
    cancel: Option<&'a AtomicBool>,
    progress: OnProgress<'a>,
}

impl Default for Ctl<'_> {
    fn default() -> Self {
        Self {
            cancel: None,
            progress: &|_, _, _| {},
        }
    }
}

impl<'a> Ctl<'a> {
    pub fn new(cancel: &'a AtomicBool, progress: OnProgress<'a>) -> Self {
        Self {
            cancel: Some(cancel),
            progress,
        }
    }

    pub fn cancelled(&self) -> bool {
        self.cancel.is_some_and(|c| c.load(Ordering::Relaxed))
    }

    pub fn check(&self) -> Result<()> {
        if self.cancelled() {
            Err(Error::Cancelled)
        } else {
            Ok(())
        }
    }

    pub fn report(&self, stage: Stage, done: u64, total: u64) {
        (self.progress)(stage, done, total);
    }
}
