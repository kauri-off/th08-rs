//! Android APK through Gradle: libth08.so per ABI, the settings schema, optional stored game assets, signing.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::assets::Game;
use crate::util::{self, BuildVersion, LocalEnv};
use crate::{Abi, Channel, DistArgs};

fn sdk() -> PathBuf {
    std::env::var_os("ANDROID_SDK")
        .or_else(|| std::env::var_os("ANDROID_HOME"))
        .map_or_else(
            || {
                PathBuf::from(std::env::var("HOME").unwrap_or_default())
                    .join(".local/share/android-sdk/Sdk")
            },
            PathBuf::from,
        )
}

fn newest(dir: &Path) -> Result<PathBuf> {
    let mut v: Vec<_> = fs::read_dir(dir)
        .with_context(|| format!("no {}", dir.display()))?
        .flatten()
        .map(|e| e.path())
        .collect();
    // build-tools are named N.N.N: sort numerically by component.
    v.sort_by_key(|p| {
        p.file_name().map(|n| {
            n.to_string_lossy()
                .split(['.', '-'])
                .map(|c| c.parse::<u32>().unwrap_or(0))
                .collect::<Vec<_>>()
        })
    });
    v.pop().with_context(|| format!("empty {}", dir.display()))
}

pub fn apk(
    a: &DistArgs,
    local: &LocalEnv,
    out: &Path,
    game: Option<&Game>,
    ver: &BuildVersion,
) -> Result<()> {
    let root = util::root();
    let sdk = sdk();
    let abis: Vec<&str> = if a.abi.is_empty() {
        vec!["arm64-v8a"]
    } else {
        a.abi.iter().map(|x| abi_name(*x)).collect()
    };
    let channel = a.channel.unwrap_or(Channel::Dev);
    let props = match channel {
        Channel::Debug if a.keystore.is_some() => {
            bail!("--channel debug signs with the throwaway key, not --keystore")
        }
        Channel::Debug => None,
        _ if a.debug_key => bail!("--debug-key is for --channel debug"),
        _ => Some(
            a.keystore
                .clone()
                .or_else(|| local.get("TH08_KEYSTORE_PROPS"))
                .with_context(|| {
                    format!(
                        "--channel {} needs --keystore PROPS (or TH08_KEYSTORE_PROPS)",
                        channel.name()
                    )
                })?,
        ),
    };
    let mut args = vec![
        format!("-Pth08.abis={}", abis.join(",")),
        format!(
            "-Pth08.minSdk={}",
            std::env::var("ANDROID_API").unwrap_or_else(|_| "26".into())
        ),
        format!(
            "-Pth08.profile={}",
            std::env::var("PROFILE").unwrap_or_else(|_| "release".into())
        ),
        format!("-Pth08.channel={}", channel.name()),
        format!("-Pth08.versionCode={}", ver.code),
        format!("-Pth08.versionName={}", ver.name),
    ];
    if let Some(p) = &props {
        if !p.is_file() {
            bail!("no keystore properties file {}", p.display());
        }
        // Only the path goes to Gradle; it reads the passwords itself (build.gradle.kts keyConfig).
        args.push(format!("-Pth08.keystore={}", util::absolute(p).display()));
    }
    // Gradle's GameData takes the .pak pair when the folder has both, else the .dat pair (build.gradle.kts gameFiles).
    if let Some(g) = game {
        args.push(format!("-Pth08.data={}", g.dir.display()));
        // Translation packs: assets/packs/lang_xx, copied into the app's pack folder by the launcher.
        if let Some(p) = &g.packs_dir {
            args.push(format!("-Pth08.packs={}", p.display()));
            args.push(format!("-Pth08.packIds={}", g.packs.join(",")));
        }
    }
    // The debug channel is Gradle's debuggable, unminified build type.
    let variant = match channel {
        Channel::Debug => "debug",
        Channel::Release | Channel::Dev => "release",
    };
    let task = format!("assemble{}{}", variant[..1].to_uppercase(), &variant[1..]);
    util::run(
        Command::new("gradle")
            .current_dir(&root)
            .env("ANDROID_HOME", &sdk)
            .args([
                "--no-daemon",
                "--console=plain",
                "-q",
                "-p",
                "android",
                &task,
            ])
            .args(&args),
    )?;
    let built = root.join(format!(
        "android/app/build/outputs/apk/{variant}/app-{variant}.apk"
    ));
    if let Some(d) = out.parent() {
        fs::create_dir_all(d)?;
    }
    util::copy(&built, out)?;
    util::run(
        Command::new(newest(&sdk.join("build-tools"))?.join("apksigner"))
            .arg("verify")
            .arg(out),
    )
}

fn abi_name(a: Abi) -> &'static str {
    match a {
        Abi::Arm64V8a => "arm64-v8a",
        Abi::X86_64 => "x86_64",
    }
}
