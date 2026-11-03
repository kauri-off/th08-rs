//! Web build: th08.wasm (lib, wasm32-unknown-unknown) + wasm-bindgen glue + web/index.html in one folder.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::util;

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
        "--release",
        "--locked",
        "--target",
        "wasm32-unknown-unknown",
    ]))?;
    let wasm = util::cargo_target().join("wasm32-unknown-unknown/release/th08.wasm");
    fs::create_dir_all(out)?;
    util::run(
        Command::new(&tool)
            .args(["--target", "web", "--no-typescript", "--out-dir"])
            .arg(out)
            .arg(&wasm),
    )?;
    util::copy(
        &util::root().join("web/index.html"),
        &out.join("index.html"),
    )
}
