//! Paths, the target/ guard, packaging/local/local.env and command helpers for `cargo xtask dist`.

use std::collections::HashMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask/ has a parent")
        .to_path_buf()
}

// Personal outputs root: <repo>/target, even with CARGO_TARGET_DIR set (the guard is about the repo's own target/).
// TH08_TARGET may name another checkout's target/ (a git worktree writing to the main checkout's target/).
pub fn personal_target() -> PathBuf {
    std::env::var_os("TH08_TARGET")
        .filter(|v| !v.is_empty())
        .map_or_else(|| root().join("target"), |p| absolute(Path::new(&p)))
}

// TH08_TARGET must be some repo's target/: named `target`, next to a Cargo.toml.
pub fn check_personal_target() -> Result<()> {
    let t = personal_target();
    if t.file_name().is_some_and(|n| n == "target")
        && t.parent().is_some_and(|p| p.join("Cargo.toml").is_file())
    {
        Ok(())
    } else {
        bail!(
            "TH08_TARGET {} is not a checkout's target/ dir",
            t.display()
        )
    }
}

pub fn dist_dir() -> PathBuf {
    personal_target().join("dist")
}

// cargo's output dir (CARGO_TARGET_DIR or <repo>/target).
pub fn cargo_target() -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR")
        .map_or_else(|| root().join("target"), |p| absolute(Path::new(&p)))
}

// Absolute, lexically normalised path (`..` removed) without requiring it to exist.
pub fn absolute(p: &Path) -> PathBuf {
    let p = if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(p)
    };
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            c => out.push(c),
        }
    }
    // Resolve symlinks of the longest existing prefix, so a link out of target/ cannot pass the guard.
    let mut base = out.clone();
    let mut rest = Vec::new();
    while !base.exists() {
        match (
            base.file_name().map(ToOwned::to_owned),
            base.parent().map(Path::to_path_buf),
        ) {
            (Some(n), Some(parent)) => {
                rest.push(n);
                base = parent;
            }
            _ => return out,
        }
    }
    let mut real = fs::canonicalize(&base).unwrap_or(base);
    real.extend(rest.iter().rev());
    real
}

// Builds with game data write only under <repo>/target/ (or TH08_TARGET).
pub fn inside_target(p: &Path) -> Result<()> {
    let t = absolute(&personal_target());
    let p = absolute(p);
    if p.starts_with(&t) && p != t {
        Ok(())
    } else {
        bail!(
            "refusing to write game data outside {}: {}",
            t.display(),
            p.display()
        )
    }
}

pub fn version() -> Result<String> {
    let toml = fs::read_to_string(root().join("Cargo.toml"))?;
    let pkg = toml
        .split("\n[package]")
        .nth(1)
        .context("no [package] in Cargo.toml")?;
    pkg.lines()
        .find_map(|l| {
            l.strip_prefix("version = \"")
                .and_then(|v| v.strip_suffix('"'))
        })
        .map(str::to_owned)
        .context("no package version in Cargo.toml")
}

// packaging/local/local.env (git-ignored, KEY=VALUE lines); process env overrides it.
pub struct LocalEnv(HashMap<String, String>);

impl LocalEnv {
    pub fn load() -> Result<Self> {
        let path = root().join("packaging/local/local.env");
        let mut m = HashMap::new();
        if let Ok(text) = fs::read_to_string(&path) {
            for l in text
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
            {
                let (k, v) = l
                    .split_once('=')
                    .with_context(|| format!("{}: bad line {l:?}", path.display()))?;
                m.insert(k.trim().to_owned(), expand_home(v.trim().trim_matches('"')));
            }
        }
        Ok(Self(m))
    }

    // Env value as given (relative to the cwd); a local.env value relative to the repo root.
    pub fn get(&self, key: &str) -> Option<PathBuf> {
        if let Some(v) = std::env::var_os(key).filter(|v| !v.is_empty()) {
            return Some(PathBuf::from(v));
        }
        self.0.get(key).map(|v| root().join(v))
    }
}

fn expand_home(v: &str) -> String {
    match (v.strip_prefix("~/"), std::env::var("HOME")) {
        (Some(rest), Ok(home)) => format!("{home}/{rest}"),
        _ => v.to_owned(),
    }
}

pub fn cargo() -> Command {
    let mut c = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
    c.current_dir(root());
    c
}

// Runs `c`, echoing program + args (never env values: keystore passwords stay out of the log).
pub fn run(c: &mut Command) -> Result<()> {
    let args: Vec<_> = c
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    let line = format!("{} {}", c.get_program().to_string_lossy(), args.join(" "));
    eprintln!("+ {line}");
    let st = c.status().with_context(|| format!("cannot start {line}"))?;
    if !st.success() {
        bail!("failed ({st}): {line}");
    }
    Ok(())
}

pub fn have(program: &str) -> bool {
    Command::new(program).arg("--version").output().is_ok()
}

pub fn fresh_dir(p: &Path) -> Result<()> {
    if p.exists() {
        fs::remove_dir_all(p).with_context(|| format!("cannot remove {}", p.display()))?;
    }
    fs::create_dir_all(p).with_context(|| format!("cannot create {}", p.display()))
}

pub fn copy(src: &Path, dst: &Path) -> Result<()> {
    if let Some(d) = dst.parent() {
        fs::create_dir_all(d)?;
    }
    fs::copy(src, dst)
        .with_context(|| format!("cannot copy {} to {}", src.display(), dst.display()))?;
    Ok(())
}

pub fn human_size(p: &Path) -> String {
    fn size(p: &Path) -> u64 {
        match fs::symlink_metadata(p) {
            Ok(m) if m.is_dir() => fs::read_dir(p)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| size(&e.path()))
                .sum(),
            Ok(m) => m.len(),
            Err(_) => 0,
        }
    }
    let b = size(p);
    #[allow(clippy::cast_precision_loss)]
    let mb = b as f64 / 1_048_576.0;
    format!("{b} bytes, {mb:.1} MiB")
}

pub fn ensure_nonempty(p: &Path) -> Result<()> {
    match fs::metadata(p) {
        Ok(m) if m.len() > 0 => Ok(()),
        _ => bail!("expected output {} is missing or empty", p.display()),
    }
}
