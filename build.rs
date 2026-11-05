//! Build script: version line (TH08_VERSION / TH08_CHANNEL / TH08_RELEASE_TAG from xtask), Windows icon and version info.

fn main() {
    println!("cargo:rerun-if-changed=packaging/windows/th08.ico");
    println!("cargo:rerun-if-env-changed=TH08_VERSION");
    println!("cargo:rerun-if-env-changed=TH08_CHANNEL");
    println!("cargo:rerun-if-env-changed=TH08_RELEASE_TAG");
    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    let version = env("TH08_VERSION")
        .unwrap_or_else(|| std::env::var("CARGO_PKG_VERSION").expect("cargo sets it"));
    println!("cargo:rustc-env=TH08_VERSION={version}");
    println!(
        "cargo:rustc-env=TH08_CHANNEL={}",
        env("TH08_CHANNEL").unwrap_or_default()
    );
    // Set only by a public release build (xtask): turns the launcher's update check on.
    println!(
        "cargo:rustc-env=TH08_RELEASE_TAG={}",
        env("TH08_RELEASE_TAG").unwrap_or_default()
    );
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("packaging/windows/th08.ico");
        res.set("FileDescription", "th08 (Touhou 8 port)");
        res.set("ProductVersion", &version);
        res.compile().expect("embedding Windows resources");
    }
}
