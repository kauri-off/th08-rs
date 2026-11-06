//! Windows installed copy: download the release's NSIS installer, check it against SHA256SUMS, run it with `/S /UPDATE`.

use crate::update::{Release, expected_sum};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const DIR: &str = "th08-update";

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    // Fraction done, None while the size is unknown.
    Downloading(Option<f32>),
    Verifying,
    // The installer runs; the launcher exits so it can replace th08.exe.
    Launched,
    Cancelled,
    Failed(Fail),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Fail {
    Network(String),
    // The release has no SHA256SUMS or it lists no installer.
    Sums,
    Hash,
    Install(String),
}

pub struct Job {
    status: Arc<Mutex<Status>>,
    cancel: Arc<AtomicBool>,
}

impl Job {
    pub fn start(r: &Release, repaint: impl Fn() + Send + 'static) -> Self {
        let status = Arc::new(Mutex::new(Status::Downloading(None)));
        let cancel = Arc::new(AtomicBool::new(false));
        let (st, stop, r) = (Arc::clone(&status), Arc::clone(&cancel), r.clone());
        std::thread::spawn(move || {
            let set = |s: Status| {
                *st.lock().unwrap() = s;
                repaint();
            };
            let end = run(&r, &stop, &set);
            if let Status::Failed(f) = &end {
                log::warn!("update: {f:?}");
            }
            set(end);
        });
        Self { status, cancel }
    }

    pub fn status(&self) -> Status {
        self.status.lock().unwrap().clone()
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_recv_response(Some(Duration::from_secs(30)))
        .https_only(true)
        .build()
        .into()
}

fn get(agent: &ureq::Agent, url: &str) -> Result<ureq::http::Response<ureq::Body>, Fail> {
    agent
        .get(url)
        .header("User-Agent", "th08-rs-update")
        .call()
        .map_err(|e| Fail::Network(e.to_string()))
}

// The asset name as a plain file name inside the temp dir (never a path).
fn safe_name(asset: &str) -> Option<&str> {
    let ok = !asset.is_empty()
        && !asset.contains(['/', '\\', ':'])
        && asset != "."
        && asset != ".."
        && asset.ends_with(".exe");
    ok.then_some(asset)
}

fn run(r: &Release, cancel: &AtomicBool, set: &dyn Fn(Status)) -> Status {
    let (Some(asset), Some(sums_url)) = (r.asset.as_deref().and_then(safe_name), r.sums.as_deref())
    else {
        return Status::Failed(Fail::Sums);
    };
    let agent = agent();
    let sums = match get(&agent, sums_url).and_then(|mut resp| {
        resp.body_mut()
            .read_to_string()
            .map_err(|e| Fail::Network(e.to_string()))
    }) {
        Ok(s) => s,
        Err(f) => return Status::Failed(f),
    };
    let Some(want) = expected_sum(&sums, asset) else {
        return Status::Failed(Fail::Sums);
    };
    let dir = std::env::temp_dir().join(DIR);
    // An installer left from an earlier update (it cannot delete itself) goes first.
    let _ = std::fs::remove_dir_all(&dir);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        return Status::Failed(Fail::Network(e.to_string()));
    }
    let path = dir.join(asset);
    let got = match download(&agent, &r.download, &path, cancel, set) {
        Ok(Some(h)) => h,
        Ok(None) => {
            let _ = std::fs::remove_file(&path);
            return Status::Cancelled;
        }
        Err(f) => {
            let _ = std::fs::remove_file(&path);
            return Status::Failed(f);
        }
    };
    set(Status::Verifying);
    if !got.eq_ignore_ascii_case(&want) {
        let _ = std::fs::remove_file(&path);
        return Status::Failed(Fail::Hash);
    }
    match launch(&path) {
        Ok(()) => Status::Launched,
        Err(e) => Status::Failed(Fail::Install(e.to_string())),
    }
}

// Streams `url` into `path`; Some(sha256 hex) when done, None when cancelled.
fn download(
    agent: &ureq::Agent,
    url: &str,
    path: &Path,
    cancel: &AtomicBool,
    set: &dyn Fn(Status),
) -> Result<Option<String>, Fail> {
    let net = |e: &dyn std::fmt::Display| Fail::Network(e.to_string());
    let mut resp = get(agent, url)?;
    let total = resp.body().content_length().filter(|&n| n > 0);
    let mut body = resp.body_mut().as_reader();
    let mut out = std::fs::File::create(path).map_err(|e| net(&e))?;
    let mut hash = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    let (mut done, mut shown) = (0u64, -1i32);
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Ok(None);
        }
        let n = body.read(&mut buf).map_err(|e| net(&e))?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
        out.write_all(&buf[..n]).map_err(|e| net(&e))?;
        done += n as u64;
        let frac = total.map(|t| (done as f64 / t as f64).min(1.0) as f32);
        let pct = frac.map_or(0, |f| (f * 100.0) as i32);
        if pct != shown {
            shown = pct;
            set(Status::Downloading(frac));
        }
    }
    out.sync_all().map_err(|e| net(&e))?;
    if total.is_some_and(|t| t != done) {
        return Err(Fail::Network(format!(
            "{done} of {} bytes",
            total.unwrap_or(0)
        )));
    }
    Ok(Some(hex(&hash.finalize())))
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn launch(installer: &Path) -> std::io::Result<()> {
    let cwd: PathBuf = installer
        .parent()
        .map_or_else(std::env::temp_dir, Path::to_path_buf);
    std::process::Command::new(installer)
        .args(["/S", "/UPDATE"])
        .current_dir(cwd)
        .spawn()
        .map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(
            safe_name("th08-0.2.0-windows-nsis.exe"),
            Some("th08-0.2.0-windows-nsis.exe")
        );
        for bad in ["", "..", "a/b.exe", r"..\x.exe", "C:x.exe", "th08.zip"] {
            assert_eq!(safe_name(bad), None, "{bad}");
        }
    }

    #[test]
    fn hex_digest() {
        assert_eq!(
            hex(&Sha256::digest(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
