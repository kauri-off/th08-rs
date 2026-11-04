//! `cargo xtask dist`: one entry point for public packages and personal builds with game data.

mod android;
mod assets;
mod desktop;
mod util;
mod web;

use std::path::PathBuf;

use anyhow::{Result, bail};
use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(name = "cargo xtask", about = "th08 build tasks")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Build one package into target/dist/ (th08-<ver>-<target>[-assets[-packed]]).
    Dist(DistArgs),
}

#[derive(clap::Args)]
pub struct DistArgs {
    /// Package to build.
    #[arg(long, value_enum)]
    target: Target,
    /// Game data to bundle: none (public), raw (th08.dat + thbgm.dat from --data) or packed (.pak via `repack`).
    #[arg(long, value_enum, default_value_t = Assets::None)]
    assets: Assets,
    /// Game folder with th08.dat / thbgm.dat (read only); default TH08_GAME_SRC (env or packaging/local/local.env).
    #[arg(long)]
    data: Option<PathBuf>,
    /// Translation packs to bundle, e.g. en,ru (thpatch language codes, imported with thcrap-import); personal
    /// packages only, so it needs --assets raw or packed.
    #[arg(long, value_delimiter = ',')]
    packs: Vec<String>,
    /// Android ABI, repeatable (default arm64-v8a).
    #[arg(long, value_enum)]
    abi: Vec<Abi>,
    /// Android keystore.properties for the release and dev channels (read only by Gradle); required for release,
    /// dev defaults to TH08_KEYSTORE_PROPS.
    #[arg(long, conflicts_with = "debug_key")]
    keystore: Option<PathBuf>,
    /// Android: sign with the throwaway key target/android/debug.keystore (debug channel only).
    #[arg(long)]
    debug_key: bool,
    /// Android channel: release (org.th08.port, public), dev (org.th08.port.dev, personal key, default) or debug
    /// (org.th08.port.debug, throwaway key).
    #[arg(long, value_enum)]
    channel: Option<Channel>,
    /// Output file (directory for web and windows-exe with assets); default target/dist/<name>.
    #[arg(long)]
    out: Option<PathBuf>,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Target {
    LinuxAppimage,
    WindowsExe,
    WindowsNsis,
    AndroidApk,
    Web,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Assets {
    None,
    Raw,
    Packed,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Abi {
    #[value(name = "arm64-v8a")]
    Arm64V8a,
    #[value(name = "x86_64")]
    X86_64,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Channel {
    Release,
    Dev,
    Debug,
}

impl Channel {
    pub fn name(self) -> &'static str {
        match self {
            Channel::Release => "release",
            Channel::Dev => "dev",
            Channel::Debug => "debug",
        }
    }
}

impl Target {
    fn name(self) -> &'static str {
        match self {
            Target::LinuxAppimage => "linux-appimage",
            Target::WindowsExe => "windows-exe",
            Target::WindowsNsis => "windows-nsis",
            Target::AndroidApk => "android-apk",
            Target::Web => "web",
        }
    }

    // File extension of the default output; "" = directory.
    fn ext(self, with_game: bool) -> &'static str {
        match self {
            Target::LinuxAppimage => ".AppImage",
            Target::WindowsExe if with_game => "",
            Target::WindowsExe | Target::WindowsNsis => ".exe",
            Target::AndroidApk => ".apk",
            Target::Web => "",
        }
    }
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Dist(a) => dist(&a),
    }
}

fn dist(a: &DistArgs) -> Result<()> {
    if a.target != Target::AndroidApk
        && (!a.abi.is_empty() || a.keystore.is_some() || a.debug_key || a.channel.is_some())
    {
        bail!("--abi / --keystore / --debug-key / --channel are only for --target android-apk");
    }
    if a.channel == Some(Channel::Release) && (a.assets != Assets::None || !a.packs.is_empty()) {
        bail!("--channel release is public: no --assets other than none, no --packs");
    }
    let ver = util::build_version()?;
    eprintln!("version {} (code {})", ver.name, ver.code);
    util::VERSION_NAME.get_or_init(|| ver.name.clone());
    util::check_personal_target()?;
    let local = util::LocalEnv::load()?;
    let suffix = match a.assets {
        Assets::None => "",
        Assets::Raw => "-assets",
        Assets::Packed => "-assets-packed",
    };
    let name = format!("th08-{}-{}{suffix}", util::version()?, a.target.name());
    let game = assets::prepare(a, &local, &name)?;
    let out = match &a.out {
        Some(p) => util::absolute(p),
        None => util::dist_dir().join(format!("{name}{}", a.target.ext(game.is_some()))),
    };
    if game.is_some() {
        util::inside_target(&out)?;
    }
    match a.target {
        Target::LinuxAppimage => desktop::appimage(&name, &out, game.as_ref())?,
        Target::WindowsExe => desktop::windows_exe(&out, game.as_ref())?,
        Target::WindowsNsis => desktop::nsis(&name, &out, game.as_ref())?,
        Target::AndroidApk => android::apk(a, &local, &out, game.as_ref(), &ver)?,
        Target::Web => web::build(&out)?,
    }
    println!("built {} ({})", out.display(), util::human_size(&out));
    if game.is_some() {
        assets::notice();
    }
    Ok(())
}
