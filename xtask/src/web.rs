//! Web build: th08.wasm (lib, wasm32-unknown-unknown) + wasm-bindgen glue, wasm-opt, web/index.html in one folder.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::util;

// Pinned binaryen (wasm-opt), sha256-checked, cached in target/dist/tools/; other hosts need this version on PATH.
const BINARYEN: &str = "version_133";
const BINARYEN_LINUX: (&str, &str) = (
    "https://github.com/WebAssembly/binaryen/releases/download/version_133/binaryen-version_133-x86_64-linux.tar.gz",
    "2dc9c7813f5375db93d96ead4b78222fcc3e2677bbb832297af4797782a37489",
);

fn wasm_opt() -> Result<PathBuf> {
    if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        let tools = util::dist_dir().join("tools");
        let tar = tools.join(format!("binaryen-{BINARYEN}-x86_64-linux.tar.gz"));
        util::fetch(BINARYEN_LINUX, &tar)?;
        let inner = format!("binaryen-{BINARYEN}/bin/wasm-opt");
        util::run(
            Command::new("tar")
                .arg("-xzf")
                .arg(&tar)
                .arg("-C")
                .arg(&tools)
                .arg(&inner),
        )?;
        return Ok(tools.join(inner));
    }
    let want = BINARYEN.trim_start_matches("version_");
    let out = Command::new("wasm-opt")
        .arg("--version")
        .output()
        .with_context(|| format!("no wasm-opt (binaryen {BINARYEN}) on PATH"))?;
    let have = String::from_utf8_lossy(&out.stdout);
    if have.split_whitespace().nth(2) != Some(want) {
        bail!("wasm-opt {} is not binaryen {BINARYEN}", have.trim());
    }
    Ok(PathBuf::from("wasm-opt"))
}

// wasm-bindgen-cli must match the wasm-bindgen crate in Cargo.lock (the glue format is version-locked).
fn locked_version() -> Result<String> {
    let lock = fs::read_to_string(util::root().join("Cargo.lock"))?;
    let mut lines = lock.lines();
    while let Some(l) = lines.next() {
        if l == "name = \"wasm-bindgen\"" {
            let v = lines
                .next()
                .and_then(|v| v.strip_prefix("version = \""))
                .and_then(|v| v.strip_suffix('"'));
            return v
                .map(str::to_owned)
                .context("Cargo.lock: wasm-bindgen version");
        }
    }
    bail!("Cargo.lock has no wasm-bindgen")
}

fn bindgen() -> PathBuf {
    let home = std::env::var_os("CARGO_HOME").map_or_else(
        || PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".cargo"),
        PathBuf::from,
    );
    let p = home.join("bin").join(if cfg!(windows) {
        "wasm-bindgen.exe"
    } else {
        "wasm-bindgen"
    });
    if p.is_file() {
        p
    } else {
        PathBuf::from("wasm-bindgen")
    }
}

pub fn build(out: &Path) -> Result<()> {
    let want = locked_version()?;
    let tool = bindgen();
    let have = Command::new(&tool)
        .arg("--version")
        .output()
        .with_context(|| format!("no wasm-bindgen ({want}) installed"))?;
    let have = String::from_utf8_lossy(&have.stdout);
    if have.split_whitespace().nth(1) != Some(want.as_str()) {
        bail!(
            "wasm-bindgen-cli {} does not match Cargo.lock {want}: cargo install wasm-bindgen-cli --version {want}",
            have.trim()
        );
    }
    util::run(util::cargo().args([
        "build",
        "--lib",
        "--profile",
        util::PROFILE,
        "--locked",
        "--target",
        "wasm32-unknown-unknown",
    ]))?;
    let wasm = util::cargo_target()
        .join("wasm32-unknown-unknown")
        .join(util::PROFILE)
        .join("th08.wasm");
    fs::create_dir_all(out)?;
    util::run(
        Command::new(&tool)
            .args(["--target", "web", "--no-typescript", "--out-dir"])
            .arg(out)
            .arg(&wasm),
    )?;
    // No --fast-math: float folding keeps IEEE semantics, as the native builds do.
    let bg = out.join("th08_bg.wasm");
    util::run(
        Command::new(wasm_opt()?)
            .arg("-O")
            .arg(&bg)
            .arg("-o")
            .arg(&bg),
    )?;
    util::copy(
        &util::root().join("web/index.html"),
        &out.join("index.html"),
    )
}
