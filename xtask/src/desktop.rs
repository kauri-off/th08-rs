//! Desktop packages: Linux AppImage, Windows exe (native MSVC or cross x86_64-pc-windows-gnu), NSIS installer.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

use crate::assets::Game;
use crate::util;

// Pinned AppImage tooling, sha256-checked, cached in target/dist/tools/.
const APPIMAGETOOL: (&str, &str) = (
    "https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-x86_64.AppImage",
    "ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0",
);
const RUNTIME: (&str, &str) = (
    "https://github.com/AppImage/type2-runtime/releases/download/20251108/runtime-x86_64",
    "2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d",
);
const WIN_GNU: &str = "x86_64-pc-windows-gnu";

fn sha256(p: &Path) -> Option<String> {
    let bytes = fs::read(p).ok()?;
    Some(
        Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
    )
}

fn fetch((url, sha): (&str, &str), dest: &Path) -> Result<()> {
    if sha256(dest).as_deref() == Some(sha) {
        return Ok(());
    }
    let part = dest.with_extension("part");
    fs::create_dir_all(dest.parent().context("tool path")?)?;
    util::run(Command::new("curl").args(["-sSfL", url, "-o"]).arg(&part))?;
    if sha256(&part).as_deref() != Some(sha) {
        bail!("sha256 mismatch for {url}");
    }
    fs::rename(&part, dest)?;
    Ok(())
}

// Release build of the th08 binary; `triple` None = host.
fn build_th08(triple: Option<&str>) -> Result<PathBuf> {
    let mut c = util::cargo();
    c.args(["build", "--release", "--locked", "--bin", "th08"]);
    let mut dir = util::cargo_target();
    if let Some(t) = triple {
        c.args(["--target", t]);
        dir.push(t);
    }
    if triple == Some(WIN_GNU) {
        // Static libgcc/winpthread: the exe then needs only Windows system DLLs (UCRT is part of Windows 10+).
        c.env(
            "CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS",
            "-C target-feature=+crt-static",
        );
    }
    util::run(&mut c)?;
    let exe = if triple.is_some_and(|t| t.contains("windows")) || cfg!(windows) {
        "th08.exe"
    } else {
        "th08"
    };
    Ok(dir.join("release").join(exe))
}

// th08.exe: native on a Windows host (+crt-static from .cargo/config.toml), else cross-built and stripped.
fn windows_binary(dst: &Path) -> Result<()> {
    if cfg!(windows) {
        return util::copy(&build_th08(None)?, dst);
    }
    util::copy(&build_th08(Some(WIN_GNU))?, dst)?;
    util::run(Command::new("x86_64-w64-mingw32-strip").arg(dst))
}

#[cfg(unix)]
fn symlink(target: &str, link: &Path) -> Result<()> {
    std::os::unix::fs::symlink(target, link).with_context(|| format!("symlink {}", link.display()))
}

#[cfg(not(unix))]
fn symlink(_: &str, _: &Path) -> Result<()> {
    bail!("the AppImage is built on Linux only")
}

// AppDir with only the port binary (glibc, libasound, GPU/window libs come from the host) + optional
// usr/share/th08/game/ (launcher.rs bundled_candidates).
pub fn appimage(name: &str, out: &Path, game: Option<&Game>) -> Result<()> {
    let root = util::root();
    let tools = util::dist_dir().join("tools");
    let (tool, runtime) = (
        tools.join("appimagetool-x86_64.AppImage"),
        tools.join("runtime-x86_64"),
    );
    fetch(APPIMAGETOOL, &tool)?;
    fetch(RUNTIME, &runtime)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(&tool, fs::Permissions::from_mode(0o755))?;
    }
    let bin = build_th08(None)?;

    let appdir = util::dist_dir().join("stage").join(name).join("AppDir");
    util::fresh_dir(&appdir)?;
    let usr_bin = appdir.join("usr/bin/th08");
    util::copy(&bin, &usr_bin)?;
    util::run(Command::new("strip").arg(&usr_bin))?;
    util::copy(
        &root.join("packaging/linux/th08.desktop"),
        &appdir.join("usr/share/applications/th08.desktop"),
    )?;
    util::copy(
        &root.join("packaging/linux/th08.png"),
        &appdir.join("usr/share/icons/hicolor/256x256/apps/th08.png"),
    )?;
    symlink(
        "usr/share/applications/th08.desktop",
        &appdir.join("th08.desktop"),
    )?;
    symlink(
        "usr/share/icons/hicolor/256x256/apps/th08.png",
        &appdir.join("th08.png"),
    )?;
    symlink("th08.png", &appdir.join(".DirIcon"))?;
    symlink("usr/bin/th08", &appdir.join("AppRun"))?;
    if let Some(g) = game {
        g.copy_to(&appdir.join("usr/share/th08/game"))?;
    }

    if let Some(d) = out.parent() {
        fs::create_dir_all(d)?;
    }
    let _ = fs::remove_file(out);
    // Extract-and-run: no FUSE needed (CI containers). appimagetool can fail with exit 0, so check the output.
    util::run(
        Command::new(&tool)
            .env("ARCH", "x86_64")
            .env("APPIMAGE_EXTRACT_AND_RUN", "1")
            .arg("--no-appstream")
            .arg("--runtime-file")
            .arg(&runtime)
            .arg(&appdir)
            .arg(out),
    )?;
    util::ensure_nonempty(out)?;
    fs::remove_dir_all(appdir.parent().context("stage")?)?;
    Ok(())
}

// Public: th08.exe at `out`. With game data: folder `out` with th08.exe, LICENSE and game/.
pub fn windows_exe(out: &Path, game: Option<&Game>) -> Result<()> {
    let Some(g) = game else {
        return windows_binary(out);
    };
    util::inside_target(out)?;
    util::fresh_dir(out)?;
    windows_binary(&out.join("th08.exe"))?;
    util::copy(&util::root().join("LICENSE"), &out.join("LICENSE"))?;
    g.copy_to(&out.join("game"))
}

// Per-user installer from packaging/windows/installer.nsi; game files go in via generated game_*.nsh includes.
pub fn nsis(name: &str, out: &Path, game: Option<&Game>) -> Result<()> {
    let root = util::root();
    let stage = util::dist_dir().join("stage").join(name);
    util::fresh_dir(&stage)?;
    windows_binary(&stage.join("th08.exe"))?;
    for f in [
        "packaging/windows/th08.ico",
        "LICENSE",
        "packaging/windows/installer.nsi",
    ] {
        let p = root.join(f);
        util::copy(&p, &stage.join(p.file_name().context("file name")?))?;
    }
    let mut defs = Vec::new();
    if let Some(g) = game {
        g.copy_to(&stage.join("game"))?;
        let install: String = g
            .names()
            .map(|n| format!("File \"${{SRC}}\\game\\{n}\"\n"))
            .collect();
        let uninstall: String = g
            .names()
            .map(|n| format!("Delete \"$INSTDIR\\game\\{n}\"\n"))
            .collect();
        fs::write(stage.join("game_install.nsh"), install)?;
        fs::write(stage.join("game_uninstall.nsh"), uninstall)?;
        defs.push("-DWITH_GAME".to_owned());
    }
    let version = util::version()?;
    let vi = format!("{}.0", version.split('-').next().unwrap_or(&version));
    let setup = out
        .file_name()
        .context("--out has no file name")?
        .to_string_lossy()
        .into_owned();
    let setup = setup.as_str();
    defs.extend([format!("-DVERSION={version}"), format!("-DVI_VERSION={vi}")]);
    if util::have("makensis") {
        let src = stage.to_string_lossy().into_owned();
        util::run(
            Command::new("makensis")
                .current_dir(&stage)
                .arg("-V2")
                .args(&defs)
                .arg(format!("-DSRC={src}"))
                .arg(format!("-DOUT={src}/{setup}"))
                .arg("installer.nsi"),
        )?;
    } else {
        makensis_docker(&stage, &defs, setup)?;
    }
    if let Some(d) = out.parent() {
        fs::create_dir_all(d)?;
    }
    fs::rename(stage.join(setup), out).or_else(|_| fs::copy(stage.join(setup), out).map(drop))?;
    util::ensure_nonempty(out)?;
    fs::remove_dir_all(&stage)?;
    Ok(())
}

// makensis in a container (debian:stable-slim + nsis); only the staging dir is mounted.
fn makensis_docker(stage: &Path, defs: &[String], setup: &str) -> Result<()> {
    const IMAGE: &str = "th08-nsis:local";
    if !util::have("docker") {
        bail!("windows-nsis needs makensis on PATH or docker");
    }
    let present = Command::new("docker")
        .args(["image", "inspect", IMAGE])
        .output()
        .is_ok_and(|o| o.status.success());
    if !present {
        let df = "FROM debian:stable-slim\nRUN apt-get update && apt-get install -y --no-install-recommends nsis \
                  && rm -rf /var/lib/apt/lists/*\n";
        let ctx = stage.join("docker");
        util::fresh_dir(&ctx)?;
        fs::write(ctx.join("Dockerfile"), df)?;
        util::run(
            Command::new("docker")
                .args(["build", "-t", IMAGE])
                .arg(&ctx),
        )?;
        fs::remove_dir_all(&ctx)?;
    }
    let uid = String::from_utf8(Command::new("id").arg("-u").output()?.stdout)?;
    let gid = String::from_utf8(Command::new("id").arg("-g").output()?.stdout)?;
    util::run(
        Command::new("docker")
            .args([
                "run",
                "--rm",
                "--user",
                &format!("{}:{}", uid.trim(), gid.trim()),
                "-v",
            ])
            .arg(format!("{}:/stage", stage.display()))
            .args(["-w", "/stage", IMAGE, "makensis", "-V2"])
            .args(defs)
            .args([
                "-DSRC=/stage",
                &format!("-DOUT=/stage/{setup}"),
                "installer.nsi",
            ]),
    )
}
