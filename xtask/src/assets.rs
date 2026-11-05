//! Game data for personal builds: guard (CI, target/, web), raw th08.dat / thbgm.dat or `repack` packs.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::util::{self, LocalEnv};
use crate::{Assets, DistArgs, Target};

// Files to bundle: (source, lowercase name in the package). `dir` holds all of them (Gradle `th08.data`).
// `packs`: translation pack folders (`<packs_dir>/lang_xx`), bundled as `packs/lang_xx` next to the game files.
pub struct Game {
    pub dir: PathBuf,
    pub files: Vec<(PathBuf, String)>,
    pub packs_dir: Option<PathBuf>,
    pub packs: Vec<String>,
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
        if let Some(src) = &self.packs_dir {
            for id in &self.packs {
                copy_dir(&src.join(id), &dst.join("packs").join(id))?;
            }
        }
        Ok(())
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.files.iter().map(|(_, n)| n.as_str())
    }
}

fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for e in fs::read_dir(src).with_context(|| format!("read {}", src.display()))? {
        let p = e?.path();
        let d = dst.join(p.file_name().context("file name")?);
        if p.is_dir() {
            copy_dir(&p, &d)?;
        } else {
            util::copy(&p, &d)?;
        }
    }
    Ok(())
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
        if !a.packs.is_empty() {
            bail!(
                "--packs needs --assets raw or --assets packed (translation packs are never in public builds)"
            );
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
    let mut game = match a.assets {
        Assets::Raw => raw(&data)?,
        _ => packed(&data, name)?,
    };
    if !a.packs.is_empty() {
        game.packs_dir = Some(packs(&data, &a.packs)?);
        game.packs = a.packs.iter().map(|c| pack_id(c)).collect();
    }
    Ok(Some(game))
}

fn pack_id(code: &str) -> String {
    if code.starts_with("lang_") {
        code.to_owned()
    } else {
        format!("lang_{code}")
    }
}

// Translation packs built by `thcrap-import` from the server (cache <personal target>/thcrap) and this th08.dat
// into <personal target>/dist-packs/lang_xx; fonts without an open licence are listed in pack.toml `personal`.
fn packs(data: &Path, codes: &[String]) -> Result<PathBuf> {
    let out = util::personal_target().join("dist-packs");
    util::inside_target(&out)?;
    fs::create_dir_all(&out)?;
    let dat = find(data, "th08.dat")?;
    let cache = util::personal_target().join("thcrap");
    for c in codes {
        let id = pack_id(c);
        eprintln!("translation pack {id} (Touhou Patch Center, CC BY-SA 4.0; personal use)");
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
                    "thcrap-import",
                    "--",
                    "--force",
                    "--lang",
                    &id,
                    "--dat",
                ])
                .arg(&dat)
                .arg("--cache")
                .arg(&cache)
                .arg("-o")
                .arg(out.join(&id)),
        )?;
        util::ensure_nonempty(&out.join(&id).join("pack.toml"))?;
    }
    Ok(out)
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
        packs_dir: None,
        packs: Vec::new(),
    })
}

const PACKS: [&str; 2] = ["th08.pak", "thbgm.pak"];

// Sources whose change can change the pack bytes (repack is deterministic).
const REPACK_SOURCES: [&str; 5] = [
    "crates/th08-tools/src/bin/repack.rs",
    "crates/th08-repack/src/content.rs",
    "crates/th08-repack/src/bgm.rs",
    "crates/th08-formats/src/pack.rs",
    "crates/th08-audio/src/bgm.rs",
];

// Cache key of a repack: name, size and mtime of each game file, sha256 of the repack sources.
fn pack_stamp(data: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    let mut s = String::from("repack v1\n");
    for n in ["th08.dat", "thbgm.dat"] {
        let p = find(data, n)?;
        let m = fs::metadata(&p)?;
        let t = m
            .modified()?
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        s += &format!(
            "{} {} {}.{:09}\n",
            p.display(),
            m.len(),
            t.as_secs(),
            t.subsec_nanos()
        );
    }
    for f in REPACK_SOURCES {
        let h = Sha256::digest(fs::read(util::root().join(f)).with_context(|| f.to_owned())?);
        s += &format!(
            "{f} {}\n",
            h.iter().map(|b| format!("{b:02x}")).collect::<String>()
        );
    }
    Ok(s)
}

// th08.pak + thbgm.pak from the `repack` bin, cached in <personal target>/pack/ with a
// `dist.stamp`; a rerun with the same game files and repack sources reuses them.
fn packed(data: &Path, _name: &str) -> Result<Game> {
    let root = util::root();
    if !root.join(REPACK_SOURCES[0]).is_file() {
        bail!(
            "--assets packed needs the `repack` bin (docs/arch/ASSETS_FORMAT.md AS1/AS2); use --assets raw"
        );
    }
    let cache = util::personal_target().join("pack");
    util::inside_target(&cache)?;
    fs::create_dir_all(&cache)?;
    let stamp = pack_stamp(data)?;
    let stamp_file = cache.join("dist.stamp");
    let fresh = fs::read_to_string(&stamp_file).is_ok_and(|s| s == stamp)
        && PACKS
            .iter()
            .all(|n| util::ensure_nonempty(&cache.join(n)).is_ok());
    if fresh {
        eprintln!("reusing {} (dist.stamp matches)", cache.display());
    } else {
        let tmp = cache.join(".dist-tmp");
        util::fresh_dir(&tmp)?;
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
                .arg(&tmp),
        )?;
        let _ = fs::remove_file(&stamp_file);
        for n in PACKS {
            let p = tmp.join(n);
            util::ensure_nonempty(&p).with_context(|| format!("repack did not write {n}"))?;
            // rename keeps links to the cached packs (e.g. pack/game_bgm/) valid.
            fs::rename(&p, cache.join(n))?;
        }
        fs::remove_dir_all(&tmp)?;
        fs::write(&stamp_file, &stamp)?;
    }
    Ok(Game {
        files: PACKS
            .iter()
            .map(|n| (cache.join(n), (*n).to_owned()))
            .collect(),
        dir: cache,
        packs_dir: None,
        packs: Vec::new(),
    })
}
