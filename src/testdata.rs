//! Test data and test doubles of the core (like th08-formats' testdata.rs): the repository root for checked-in
//! reference data, and (tests only) the original game files from TH08_DIR plus a `Ctx` loaded on a
//! `NullRenderer`, a silent `AudioOut` and a `MemHost`. Tests that need the game files skip without TH08_DIR.

use std::path::{Path, PathBuf};

// Repository root (specs/data, tests/data): the manifest dir, or two levels up for a crate under crates/
// (STEP10_CRATES risk 6). Pure path arithmetic, no file system access.
pub fn repo_root() -> PathBuf {
    let m = Path::new(env!("CARGO_MANIFEST_DIR"));
    match m.parent() {
        Some(p) if p.file_name().is_some_and(|n| n == "crates") => {
            p.parent().unwrap_or(p).to_path_buf()
        }
        _ => m.to_path_buf(),
    }
}

#[cfg(test)]
pub use imp::*;

#[cfg(test)]
mod imp {
    use crate::archive::Archive;
    use crate::assets::{BoxFuture, DataSource, Source};
    use crate::game::{AudioOut, Ctx, Io};
    use std::path::PathBuf;

    // The game directory (TH08_DIR), None when unset.
    pub fn game_dir() -> Option<PathBuf> {
        std::env::var_os("TH08_DIR").map(PathBuf::from)
    }

    // Exact name first, else a case-insensitive match (as on Windows, 01 §7).
    fn find(dir: &std::path::Path, name: &str) -> PathBuf {
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

    pub fn archive() -> Option<Archive> {
        let path = find(&game_dir()?, "th08.dat");
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        Some(Archive::from_bytes(bytes).expect("th08.dat"))
    }

    // Loose files of the game directory.
    struct GameFiles(PathBuf);

    impl DataSource for GameFiles {
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
        ) -> BoxFuture<'a, anyhow::Result<Vec<u8>>> {
            Box::pin(async move {
                let b = std::fs::read(find(&self.0, name))?;
                Ok(match range {
                    None => b,
                    Some((off, len)) => b
                        .get(off as usize..off as usize + len)
                        .ok_or_else(|| anyhow::anyhow!("{name}: short read"))?
                        .to_vec(),
                })
            })
        }
    }

    pub fn source() -> Option<Source> {
        Some(Source::new(GameFiles(game_dir()?)))
    }

    // Silent sound device without a MIDI synth (`midi_ready` false, as without a SoundFont).
    pub struct NullAudio;

    impl AudioOut for NullAudio {
        fn load(&mut self, _: crate::content::PresentView<'_>) {}
        fn midi_available(&mut self) -> bool {
            false
        }
        fn exec(&mut self, _: crate::sound::SoundCmd) {}
        fn is_started(&self) -> bool {
            true
        }
        fn ensure_started(&mut self) {}
    }

    // Runs a future whose reads complete at once (the game-file source), without an executor dependency.
    pub fn block_on<F: std::future::Future>(f: F) -> F::Output {
        let mut f = std::pin::pin!(f);
        let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
        loop {
            if let std::task::Poll::Ready(v) = f.as_mut().poll(&mut cx) {
                return v;
            }
            std::thread::yield_now();
        }
    }

    // A `Ctx` over the game files with `host`, no saves (`persist` false); None without TH08_DIR.
    pub fn ctx_with(
        core: crate::coreconfig::CoreConfig,
        host: Box<dyn crate::host::Host>,
    ) -> Option<Ctx> {
        let io = Io {
            renderer: Box::new(crate::drawlist::NullRenderer),
            audio: Box::new(NullAudio),
            host,
            drawlog: None,
        };
        Some(block_on(Ctx::load_with(source()?, io, core, false)).unwrap())
    }

    // `ctx_with` on a default `MemHost`.
    pub fn ctx(core: crate::coreconfig::CoreConfig) -> Option<Ctx> {
        ctx_with(core, Box::<crate::host::MemHost>::default())
    }

    // The test's `MemHost` (input keys, saves).
    pub fn mem_host(ctx: &mut Ctx) -> &mut crate::host::MemHost {
        ctx.host_as().expect("not a MemHost")
    }

    // Frames of the long enemy test (`TH08_TEST_FRAMES`, default 4000).
    pub fn test_frames() -> u32 {
        std::env::var("TH08_TEST_FRAMES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(4000)
    }
}
