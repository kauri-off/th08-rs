//! Game data for personal builds: guard (CI, target/, web), raw th08.dat / thbgm.dat or `repack` packs (ASSETS_FORMAT §3).

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::util::{self, LocalEnv};
use crate::{Assets, DistArgs, Target};

// Files to bundle: (source, lowercase name in the package). `dir` holds all of them (Gradle `th08.data`).
pub struct Game {
    pub dir: PathBuf,
    pub files: Vec<(PathBuf, String)>,
    pub packed: bool,
}

impl Game {
    // Copies the files into `dst` (read-only sources; copies are 0644 so later rebuilds can replace them).
    pub fn copy_to(&self, dst: &Path) -> Result<()> {
        util::inside_target(dst)?;
        fs::create_dir_all(dst)?;
        for (src, name) in &self.files {
            let d = dst.join(name);
            util::copy(src, &d)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt as _;
                fs::set_permissions(&d, fs::Permissions::from_mode(0o644))?;
            }
        }
        Ok(())
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.files.iter().map(|(_, n)| n.as_str())
    }
}

pub fn notice() {
    eprintln!(
        "NOTICE: this package contains your copy of the Touhou 8 game data. Personal use only: \
         never distribute, upload or commit it."
    );
}

pub fn prepare(a: &DistArgs, local: &LocalEnv, name: &str) -> Result<Option<Game>> {
    if a.assets == Assets::None {
        if a.data.is_some() {
            bail!("--data needs --assets raw or --assets packed");
        }
        return Ok(None);
    }
    if std::env::var_os("CI").is_some_and(|v| !v.is_empty()) {
        bail!(
            "--assets {} refused: CI is set (public builds use --assets none)",
            assets_name(a.assets)
        );
    }
    if a.target == Target::Web {
        bail!("--target web is public only: use --assets none");
    }
    notice();
    let data = a
        .data
        .clone()
        .or_else(|| local.get("TH08_GAME_SRC"))
        .context("game folder unknown: pass --data DIR or set TH08_GAME_SRC (env or packaging/local/local.env)")?;
    if !data.is_dir() {
        bail!("no game folder {}", data.display());
    }
    let data = util::absolute(&data);
    match a.assets {
        Assets::Raw => raw(&data).map(Some),
        _ => packed(&data, name).map(Some),
    }
}

fn assets_name(a: Assets) -> &'static str {
    match a {
        Assets::None => "none",
        Assets::Raw => "raw",
        Assets::Packed => "packed",
    }
}

// The game file `name` in `dir`, any letter case.
fn find(dir: &Path, name: &str) -> Result<PathBuf> {
    fs::read_dir(dir)?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.is_file()
                && p.file_name()
                    .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(name))
        })
        .with_context(|| format!("no {name} in {}", dir.display()))
}

fn raw(data: &Path) -> Result<Game> {
    let files = ["th08.dat", "thbgm.dat"]
        .iter()
        .map(|n| Ok((find(data, n)?, (*n).to_owned())))
        .collect::<Result<_>>()?;
    Ok(Game {
        dir: data.to_path_buf(),
        files,
        packed: false,
    })
}

// th08.pak + thbgm.pak from the `repack` bin (ASSETS_FORMAT AS1/AS2), written under target/dist/packs/.
fn packed(data: &Path, name: &str) -> Result<Game> {
    let root = util::root();
    if !root.join("crates/th08-tools/src/bin/repack.rs").is_file() {
        bail!(
            "--assets packed needs the `repack` bin (docs/arch/ASSETS_FORMAT.md AS1/AS2), which is not in this \
             checkout yet; use --assets raw"
        );
    }
    let out = util::dist_dir().join("packs").join(name);
    util::inside_target(&out)?;
    util::fresh_dir(&out)?;
    util::run(
        util::cargo()
            .args([
                "run",
                "-q",
                "--release",
                "--locked",
                "-p",
                "th08-tools",
                "--bin",
                "repack",
                "--",
            ])
            .arg(data)
            .arg(&out),
    )?;
    let files = ["th08.pak", "thbgm.pak"]
        .iter()
        .map(|n| {
            let p = out.join(n);
            util::ensure_nonempty(&p)
                .with_context(|| format!("repack did not write {n} (AS2 not landed yet?)"))?;
            Ok((p, (*n).to_owned()))
        })
        .collect::<Result<_>>()?;
    Ok(Game {
        dir: out,
        files,
        packed: true,
    })
}
