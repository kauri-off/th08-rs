//! Launcher update check: the latest GitHub release, asked at most once a day in release builds only.

use serde_json::{Value, json};
use std::path::Path;

pub const LATEST_URL: &str = "https://api.github.com/repos/kauri-off/th08-rs/releases/latest";
const STATE_FILE: &str = "update_check.json";
const DAY_SECS: u64 = 24 * 60 * 60;
// Test-only: a release JSON file read instead of the network, without the channel and the daily gate (desktop: this
// env var; Android outside the release channel: `<files>/update_test.json`).
#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
const JSON_ENV: &str = "TH08_UPDATE_JSON";
#[cfg(target_os = "android")]
const JSON_FILE: &str = "update_test.json";

#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub version: String,
    // The release's page ("What is new").
    pub page: String,
    // This platform's asset, else the release page.
    pub download: String,
    // That asset's file name, as SHA256SUMS lists it.
    pub asset: Option<String>,
    // The release's SHA256SUMS asset.
    pub sums: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Platform {
    Android,
    WindowsInstalled,
    WindowsPortable,
    Linux,
}

impl Platform {
    // Asset name suffix as the release workflow names them (`th08-<version>-<target>.<ext>`).
    fn asset_suffix(self) -> &'static str {
        match self {
            Platform::Android => "-android-apk.apk",
            Platform::WindowsInstalled => "-windows-nsis.exe",
            Platform::WindowsPortable => "-windows-exe.exe",
            Platform::Linux => "-linux-appimage.AppImage",
        }
    }

    pub fn here() -> Self {
        if cfg!(target_os = "android") {
            Platform::Android
        } else if cfg!(windows) {
            #[cfg(windows)]
            if std::env::current_exe().is_ok_and(|e| installed_copy(&HkcuUninstall, &e)) {
                return Platform::WindowsInstalled;
            }
            Platform::WindowsPortable
        } else {
            Platform::Linux
        }
    }
}

// The NSIS installer's InstallLocation (HKCU uninstall key), behind a trait for the tests.
#[cfg(any(windows, test))]
pub trait InstallRegistry {
    fn install_location(&self) -> Option<String>;
}

#[cfg(windows)]
pub struct HkcuUninstall;

#[cfg(windows)]
impl InstallRegistry for HkcuUninstall {
    fn install_location(&self) -> Option<String> {
        use windows_sys::Win32::System::Registry::{
            HKEY_CURRENT_USER, RRF_RT_REG_SZ, RegGetValueW,
        };
        let wide = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
        let key = wide(r"Software\Microsoft\Windows\CurrentVersion\Uninstall\th08-port");
        let value = wide("InstallLocation");
        let mut buf = vec![0u16; 1024];
        let mut len = (buf.len() * 2) as u32;
        // SAFETY: both names are NUL-terminated and `len` is the byte size of `buf`.
        let rc = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                value.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buf.as_mut_ptr().cast(),
                &mut len,
            )
        };
        if rc != 0 {
            return None;
        }
        buf.truncate(len as usize / 2);
        let s = String::from_utf16_lossy(&buf);
        let s = s.trim_end_matches('\0');
        (!s.is_empty()).then(|| s.to_string())
    }
}

// Windows paths compare without case, separator style or a trailing separator.
#[cfg(any(windows, test))]
fn norm_path(p: &Path) -> String {
    let p = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let s = p.to_string_lossy().replace('/', "\\").to_lowercase();
    let s = s.strip_prefix(r"\\?\").unwrap_or(&s);
    s.trim_end_matches('\\').to_string()
}

// The running exe is the copy the installer put there (its folder is InstallLocation).
#[cfg(any(windows, test))]
pub fn installed_copy(reg: &dyn InstallRegistry, exe: &Path) -> bool {
    let (Some(loc), exe) = (reg.install_location(), norm_path(exe)) else {
        return false;
    };
    exe.rsplit_once('\\')
        .is_some_and(|(dir, _)| !dir.is_empty() && norm_path(Path::new(&loc)) == dir)
}

// SHA256SUMS (`<hex>  <name>` or `<hex> *<name>`): the lowercase checksum listed first for `asset`.
pub fn expected_sum(sums: &str, asset: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let line = line.trim_end_matches('\r');
        let (hex, rest) = line.split_at_checked(64)?;
        let name = rest.strip_prefix(' ')?;
        let name = name.strip_prefix(['*', ' '])?;
        (hex.bytes().all(|b| b.is_ascii_hexdigit()) && name == asset)
            .then(|| hex.to_ascii_lowercase())
    })
}

pub fn sum_matches(sums: &str, asset: &str, actual_hex: &str) -> bool {
    expected_sum(sums, asset).is_some_and(|e| e.eq_ignore_ascii_case(actual_hex))
}

// Release channel: the public APK (`org.th08.port`) or a desktop build with TH08_RELEASE_TAG.
pub fn release_build() -> bool {
    const CHANNEL: &str = env!("TH08_CHANNEL");
    const TAG: &str = env!("TH08_RELEASE_TAG");
    if cfg!(target_os = "android") {
        CHANNEL == "release"
    } else {
        !TAG.is_empty()
    }
}

fn parse_version(v: &str) -> Option<semver::Version> {
    semver::Version::parse(v.trim().trim_start_matches('v')).ok()
}

// `latest` is newer than `running`; an unparsable version is never newer.
pub fn is_newer(latest: &str, running: &str) -> bool {
    match (parse_version(latest), parse_version(running)) {
        (Some(l), Some(r)) => l > r,
        _ => false,
    }
}

// The release a `releases/latest` reply describes, with this platform's download.
pub fn parse_release(json: &str, platform: Platform) -> Option<Release> {
    let v: Value = serde_json::from_str(json).ok()?;
    if v["draft"].as_bool() == Some(true) || v["prerelease"].as_bool() == Some(true) {
        return None;
    }
    let version = v["tag_name"].as_str()?.trim_start_matches('v').to_string();
    parse_version(&version)?;
    let page = v["html_url"].as_str()?.to_string();
    let find = |want: &dyn Fn(&str) -> bool| {
        v["assets"].as_array().into_iter().flatten().find_map(|a| {
            let name = a["name"].as_str().filter(|n| want(n))?;
            Some((
                name.to_string(),
                a["browser_download_url"].as_str()?.to_string(),
            ))
        })
    };
    let asset = find(&|n| n.ends_with(platform.asset_suffix()));
    let sums = find(&|n| n == "SHA256SUMS").map(|(_, url)| url);
    let (asset, download) = match asset {
        Some((name, url)) => (Some(name), url),
        None => (None, page.clone()),
    };
    Some(Release {
        version,
        page,
        download,
        asset,
        sums,
    })
}

pub fn due(last: Option<u64>, now: u64) -> bool {
    last.is_none_or(|t| now < t || now - t >= DAY_SECS)
}

// The state file: when the last request was made and the reply it got (kept, so the row stays for the day).
struct State {
    checked: Option<u64>,
    reply: Option<String>,
}

fn read_state(dir: &Path) -> State {
    let v: Value = std::fs::read(dir.join(STATE_FILE))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or(Value::Null);
    State {
        checked: v["checked"].as_u64(),
        reply: v["reply"].as_str().map(String::from),
    }
}

fn write_state(dir: &Path, s: &State) {
    let v = json!({"checked": s.checked, "reply": s.reply});
    if let Err(e) = crate::platform::write_atomic(&dir.join(STATE_FILE), v.to_string().as_bytes()) {
        log::debug!("update check state: {e}");
    }
}

// Asks `fetch` when a day has passed since the last request (an attempt counts, failed or not), else uses the
// stored reply; Some when that release is newer than `running`.
pub fn check(
    dir: &Path,
    now: u64,
    running: &str,
    platform: Platform,
    fetch: impl FnOnce() -> Option<String>,
) -> Option<Release> {
    let mut st = read_state(dir);
    if due(st.checked, now) {
        st.checked = Some(now);
        if let Some(r) = fetch() {
            st.reply = Some(r);
        }
        write_state(dir, &st);
    }
    let r = parse_release(st.reply.as_deref()?, platform)?;
    is_newer(&r.version, running).then_some(r)
}

#[cfg(not(target_arch = "wasm32"))]
fn fetch_latest() -> Option<String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(10)))
        .https_only(true)
        .build()
        .into();
    let r = agent
        .get(LATEST_URL)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "th08-rs-update-check")
        .call();
    match r {
        Ok(mut resp) => resp.body_mut().read_to_string().ok(),
        Err(e) => {
            log::debug!("update check: {e}");
            None
        }
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

// The launcher's check: the test JSON when set, else the network in a release build; None when off or on any error.
#[cfg(not(target_arch = "wasm32"))]
pub fn run(config: Option<&Path>) -> Option<Release> {
    let running = env!("TH08_VERSION");
    let platform = Platform::here();
    if let Some(p) = test_json(config) {
        let s = std::fs::read_to_string(p).ok()?;
        let r = parse_release(&s, platform)?;
        return is_newer(&r.version, running).then_some(r);
    }
    if !release_build() {
        return None;
    }
    check(config?, now_secs(), running, platform, fetch_latest)
}

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
fn test_json(_config: Option<&Path>) -> Option<std::ffi::OsString> {
    std::env::var_os(JSON_ENV)
}

#[cfg(target_os = "android")]
fn test_json(config: Option<&Path>) -> Option<std::path::PathBuf> {
    let p = config?.join(JSON_FILE);
    (!release_build() && p.is_file()).then_some(p)
}

// A tool or test run (hermetic settings) never asks, unless it sets the test JSON.
#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
pub fn wanted(s: &crate::settings::Settings) -> bool {
    s.update_check && (!s.hermetic || std::env::var_os(JSON_ENV).is_some())
}

// Desktop launcher: the check on a thread of its own, the answer read by the screen when it is there.
#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
pub struct Background(std::sync::Arc<std::sync::Mutex<Option<Release>>>);

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
impl Background {
    pub fn start(
        config: Option<std::path::PathBuf>,
        repaint: impl FnOnce() + Send + 'static,
    ) -> Self {
        let slot: std::sync::Arc<std::sync::Mutex<Option<Release>>> = std::sync::Arc::default();
        // The test JSON answers at once, so an offscreen render shows the row.
        if std::env::var_os(JSON_ENV).is_some() {
            *slot.lock().unwrap() = run(config.as_deref());
            return Self(slot);
        }
        let out = std::sync::Arc::clone(&slot);
        std::thread::spawn(move || {
            if let Some(r) = run(config.as_deref()) {
                *out.lock().unwrap() = Some(r);
                repaint();
            }
        });
        Self(slot)
    }

    pub fn get(&self) -> Option<Release> {
        self.0.lock().unwrap().clone()
    }
}

// Android launcher (UpdateCheck.kt): `{"config": dir}` → the newer release, or `{}`.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_org_th08_port_UpdateCheck_call<'c>(
    mut env: jni::EnvUnowned<'c>,
    _class: jni::objects::JClass<'c>,
    req: jni::objects::JString<'c>,
) -> jni::objects::JString<'c> {
    env.with_env(|env| -> jni::errors::Result<jni::objects::JString<'c>> {
        let r = req.try_to_string(env)?;
        let reply = serde_json::from_str::<Value>(&r)
            .ok()
            .and_then(|v| v["config"].as_str().map(std::path::PathBuf::from))
            .and_then(|c| run(Some(&c)))
            .map_or_else(
                || json!({}),
                |r| {
                    json!({"version": r.version, "page": r.page, "download": r.download, "asset": r.asset,
                        "sums": r.sums})
                },
            );
        jni::objects::JString::from_str(env, reply.to_string())
    })
    .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPLY: &str = r#"{
        "tag_name": "v0.2.0", "html_url": "https://github.com/kauri-off/th08-rs/releases/tag/v0.2.0",
        "draft": false, "prerelease": false,
        "assets": [
            {"name": "th08-0.2.0-windows-exe.exe", "browser_download_url": "https://x/th08-0.2.0-windows-exe.exe"},
            {"name": "th08-0.2.0-windows-nsis.exe", "browser_download_url": "https://x/th08-0.2.0-windows-nsis.exe"},
            {"name": "th08-0.2.0-linux-appimage.AppImage", "browser_download_url": "https://x/a.AppImage"},
            {"name": "th08-0.2.0-linux-appimage.AppImage.zsync", "browser_download_url": "https://x/a.zsync"},
            {"name": "th08-0.2.0-android-apk.apk", "browser_download_url": "https://x/a.apk"},
            {"name": "SHA256SUMS", "browser_download_url": "https://x/SHA256SUMS"}
        ]
    }"#;

    #[test]
    fn versions() {
        assert!(is_newer("0.2.0", "0.1.9"));
        assert!(is_newer("v0.1.10", "0.1.9"));
        assert!(is_newer("1.0.0", "1.0.0-rc.1"));
        assert!(!is_newer("0.1.2", "0.1.2"));
        assert!(!is_newer("0.1.1", "0.1.2"));
        assert!(!is_newer("garbage", "0.1.2"));
        assert!(!is_newer("0.2.0", "0.1.2-5-gdeadbeef-dirty-x y"));
    }

    #[test]
    fn assets() {
        let r = |p| parse_release(REPLY, p).unwrap().download;
        assert_eq!(r(Platform::Android), "https://x/a.apk");
        assert_eq!(r(Platform::Linux), "https://x/a.AppImage");
        assert_eq!(
            r(Platform::WindowsInstalled),
            "https://x/th08-0.2.0-windows-nsis.exe"
        );
        assert_eq!(
            r(Platform::WindowsPortable),
            "https://x/th08-0.2.0-windows-exe.exe"
        );
        let apk = parse_release(REPLY, Platform::Android).unwrap();
        assert_eq!(apk.asset.as_deref(), Some("th08-0.2.0-android-apk.apk"));
        assert_eq!(apk.sums.as_deref(), Some("https://x/SHA256SUMS"));
        let rel = parse_release(REPLY, Platform::Linux).unwrap();
        assert_eq!(rel.version, "0.2.0");
        assert!(rel.page.ends_with("/v0.2.0"));
        let bare = r#"{"tag_name": "v0.2.0", "html_url": "https://p", "assets": []}"#;
        let b = parse_release(bare, Platform::Linux).unwrap();
        assert_eq!(
            (b.download.as_str(), b.asset, b.sums),
            ("https://p", None, None)
        );
        let pre = r#"{"tag_name": "v0.3.0", "html_url": "https://p", "prerelease": true}"#;
        assert_eq!(parse_release(pre, Platform::Linux), None);
        assert_eq!(parse_release("not json", Platform::Linux), None);
    }

    #[test]
    fn daily_gate() {
        assert!(due(None, 0));
        assert!(!due(Some(1000), 1000 + DAY_SECS - 1));
        assert!(due(Some(1000), 1000 + DAY_SECS));
        assert!(due(Some(1000), 999));
    }

    #[test]
    fn check_asks_once_a_day() {
        let dir = std::env::temp_dir().join(format!("th08-update-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let calls = std::cell::Cell::new(0);
        let fetch = |reply: Option<&'static str>| {
            let calls = &calls;
            move || {
                calls.set(calls.get() + 1);
                reply.map(String::from)
            }
        };
        let t0 = 1_800_000_000;
        // Offline: the attempt still counts for the day; nothing to show.
        assert_eq!(check(&dir, t0, "0.1.2", Platform::Linux, fetch(None)), None);
        assert_eq!(
            check(&dir, t0 + 60, "0.1.2", Platform::Linux, fetch(Some(REPLY))),
            None
        );
        assert_eq!(calls.get(), 1);
        let next = t0 + DAY_SECS;
        let r = check(&dir, next, "0.1.2", Platform::Linux, fetch(Some(REPLY))).unwrap();
        assert_eq!(r.version, "0.2.0");
        assert_eq!(calls.get(), 2);
        // The stored reply serves the rest of the day without a request.
        let again = check(&dir, next + 3600, "0.1.2", Platform::Linux, fetch(None));
        assert_eq!(again, Some(r));
        assert_eq!(calls.get(), 2);
        assert_eq!(
            check(&dir, next + 3600, "0.2.0", Platform::Linux, fetch(None)),
            None
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn sums() {
        let (a, b) = ("a".repeat(64), "B".repeat(64));
        let nsis = "th08-0.2.0-windows-nsis.exe";
        let text = format!(
            "{a}  th08-0.2.0-windows-exe.exe\r\n{b} *{nsis}\r\nnot a line\r\n{}  short.exe\r\n{a}  {nsis}\r\n",
            "c".repeat(63)
        );
        assert_eq!(
            expected_sum(&text, "th08-0.2.0-windows-exe.exe"),
            Some(a.clone())
        );
        // The first line for a name wins; the checksum comes back lowercase.
        assert_eq!(expected_sum(&text, nsis), Some("b".repeat(64)));
        assert_eq!(expected_sum(&text, "short.exe"), None);
        assert_eq!(expected_sum(&text, "th08-0.2.0-windows-nsis"), None);
        assert_eq!(expected_sum("", nsis), None);
        assert!(sum_matches(&text, nsis, &"b".repeat(64)));
        assert!(sum_matches(&text, nsis, &b));
        assert!(!sum_matches(&text, nsis, &a));
        assert!(!sum_matches(&text, "missing.exe", &a));
        let x = "\u{e9}".repeat(40);
        assert_eq!(expected_sum(&format!("{x}  {nsis}"), nsis), None);
    }

    struct Reg(Option<&'static str>);

    impl InstallRegistry for Reg {
        fn install_location(&self) -> Option<String> {
            self.0.map(String::from)
        }
    }

    #[test]
    fn installed() {
        let exe = Path::new(r"C:\Users\me\AppData\Local\Programs\th08\th08.exe");
        let here = |loc| installed_copy(&Reg(loc), exe);
        assert!(here(Some(r"C:\Users\me\AppData\Local\Programs\th08")));
        assert!(here(Some(r"c:\users\ME\AppData\Local\Programs\th08\")));
        assert!(here(Some("C:/Users/me/AppData/Local/Programs/th08")));
        assert!(!here(Some(r"D:\Games\th08")));
        assert!(!here(Some(r"C:\Users\me\AppData\Local\Programs")));
        assert!(!here(None));
        let unix = Path::new("/opt/th08/th08");
        assert!(installed_copy(&Reg(Some("/opt/th08")), unix));
        assert!(!installed_copy(
            &Reg(Some("/opt/th08")),
            Path::new("/home/x/th08")
        ));
    }
}
