//! thpatch translations → presentation packs: download, key mapping, font chain, pack build and install.

pub mod build;
pub mod fontchain;
pub mod manage;
pub mod thcrap;
pub mod thpatch;
pub mod vfs;

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

// `n` copies of a worker loop on threads; one run on the calling thread in the browser (no threads there).
fn parallel(n: usize, worker: impl Fn() + Sync) {
    #[cfg(not(target_arch = "wasm32"))]
    std::thread::scope(|s| {
        for _ in 0..n {
            s.spawn(&worker);
        }
    });
    #[cfg(target_arch = "wasm32")]
    {
        let _ = n;
        worker();
    }
}
