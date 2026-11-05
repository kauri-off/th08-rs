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

// Version name stamped into the game builds (build.rs reads TH08_VERSION); set once by `dist`.
pub static VERSION_NAME: std::sync::OnceLock<String> = std::sync::OnceLock::new();

pub fn cargo() -> Command {
    let mut c = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
    c.current_dir(root());
    if let Some(v) = VERSION_NAME.get() {
        c.env("TH08_VERSION", v);
    }
    // A release build's tag is baked into the game (build.rs): the launcher's update check runs only then.
    match std::env::var("TH08_RELEASE_TAG") {
        Ok(t) if !t.is_empty() => c.env("TH08_RELEASE_TAG", t),
        _ => c.env_remove("TH08_RELEASE_TAG"),
    };
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

// Version name and Android versionCode of this checkout.
pub struct BuildVersion {
    pub name: String,
    pub code: u32,
}

// 2026-01-01T00:00Z: versionCode counts minutes from here.
const CODE_EPOCH: i64 = 1_767_225_600;
// Android's largest accepted versionCode.
const CODE_MAX: i64 = 2_100_000_000;

fn git(args: &[&str]) -> Option<String> {
    let o = Command::new("git")
        .current_dir(root())
        .args(args)
        .output()
        .ok()?;
    let s = String::from_utf8(o.stdout).ok()?;
    (o.status.success() && !s.trim().is_empty()).then(|| s.trim().to_owned())
}

pub fn build_version() -> Result<BuildVersion> {
    let cargo = version()?;
    // The release workflow passes the tag it is about to create; a tag-triggered run names it in GITHUB_REF.
    let ci_tag = std::env::var("TH08_RELEASE_TAG")
        .ok()
        .filter(|t| !t.is_empty())
        .or_else(|| {
            (std::env::var("GITHUB_REF_TYPE").as_deref() == Ok("tag"))
                .then(|| std::env::var("GITHUB_REF_NAME").unwrap_or_default())
        });
    let describe = git(&["describe", "--tags", "--dirty", "--always"]);
    let has_tags = git(&["describe", "--tags", "--abbrev=0"]).is_some();
    let name = version_name(&cargo, describe.as_deref(), has_tags, ci_tag.as_deref())?;
    let code = if let Some(v) = std::env::var("VERSION_CODE").ok().filter(|v| !v.is_empty()) {
        v.parse()
            .with_context(|| format!("VERSION_CODE {v:?} is not a number"))?
    } else {
        let dirty = describe.as_ref().is_none_or(|d| d.ends_with("-dirty"));
        let commit = git(&["log", "-1", "--format=%ct"]).and_then(|t| t.parse().ok());
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();
        version_code(commit, dirty, i64::try_from(now)?)?
    };
    Ok(BuildVersion { name, code })
}

// `describe`: `git describe --tags --dirty --always` (None without git); `ci_tag`: the pushed tag in CI.
fn version_name(
    cargo: &str,
    describe: Option<&str>,
    has_tags: bool,
    ci_tag: Option<&str>,
) -> Result<String> {
    if let Some(t) = ci_tag {
        let v = t
            .strip_prefix('v')
            .with_context(|| format!("tag {t:?} is not vX.Y.Z"))?;
        if v != cargo {
            bail!("tag {t} does not match the Cargo.toml version {cargo}");
        }
        return Ok(v.to_owned());
    }
    Ok(match describe {
        None => cargo.to_owned(),
        Some(d) if has_tags => d.strip_prefix('v').unwrap_or(d).to_owned(),
        Some(d) => format!("{cargo}-g{d}"),
    })
}

// Minutes since CODE_EPOCH of the commit time, or of `now` for a dirty tree (a fresh build installs over older ones).
fn version_code(commit: Option<i64>, dirty: bool, now: i64) -> Result<u32> {
    let t = match commit {
        Some(c) if !dirty => c,
        _ => now,
    };
    let m = (t - CODE_EPOCH) / 60;
    if !(1..=CODE_MAX).contains(&m) {
        bail!("versionCode {m} out of range (time {t})");
    }
    Ok(u32::try_from(m)?)
}

#[cfg(test)]
mod tests {
    use super::{version_code, version_name};

    #[test]
    fn names() {
        let n = |d, tags, tag| version_name("0.1.0", d, tags, tag);
        assert_eq!(
            n(Some("v0.1.0-42-g3409632-dirty"), true, None).unwrap(),
            "0.1.0-42-g3409632-dirty"
        );
        assert_eq!(n(Some("v0.1.0"), true, None).unwrap(), "0.1.0");
        assert_eq!(
            n(Some("0.2.0-1-gabc1234"), true, None).unwrap(),
            "0.2.0-1-gabc1234"
        );
        assert_eq!(n(Some("3409632"), false, None).unwrap(), "0.1.0-g3409632");
        assert_eq!(
            n(Some("3409632-dirty"), false, None).unwrap(),
            "0.1.0-g3409632-dirty"
        );
        assert_eq!(n(None, false, None).unwrap(), "0.1.0");
        assert_eq!(
            n(Some("v0.1.0-3-gabc"), true, Some("v0.1.0")).unwrap(),
            "0.1.0"
        );
        assert!(n(Some("v0.2.0"), true, Some("v0.2.0")).is_err());
        assert!(n(Some("0.1.0"), true, Some("0.1.0")).is_err());
    }

    #[test]
    fn codes() {
        let e = 1_767_225_600;
        assert_eq!(version_code(Some(e + 60), false, e + 6000).unwrap(), 1);
        assert_eq!(version_code(Some(e + 60), true, e + 6000).unwrap(), 100);
        assert_eq!(version_code(None, false, e + 125).unwrap(), 2);
        // 2026-11-04T00:00Z
        assert_eq!(
            version_code(Some(1_793_750_400), false, 0).unwrap(),
            442_080
        );
        assert!(version_code(Some(e), false, 0).is_err());
    }
}
