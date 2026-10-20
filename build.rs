//! Build script: embeds the icon and version info into the Windows executable (port packaging, no spec section).

fn main() {
    println!("cargo:rerun-if-changed=packaging/windows/th08.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("packaging/windows/th08.ico");
        res.set("FileDescription", "th08 (Touhou 8 port)");
        res.compile().expect("embedding Windows resources");
    }
}
