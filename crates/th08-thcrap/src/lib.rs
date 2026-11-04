//! thpatch translations → presentation packs: download, key mapping, font chain, pack build and install.
#![cfg(not(target_arch = "wasm32"))]

pub mod build;
pub mod fontchain;
pub mod manage;
pub mod thcrap;
pub mod thpatch;

// Where download / build messages go: the CLI prints them, the launchers show them.
pub trait Progress: Sync {
    fn line(&self, s: &str);
    // Files of the current download done / total.
    fn files(&self, _done: usize, _total: usize) {}
}

// Progress on stdout (CLI).
pub struct Stdout;

impl Progress for Stdout {
    fn line(&self, s: &str) {
        println!("{s}");
    }
}
