//! Translation packs from the settings screens: server language list, download / update / remove jobs, progress.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use th08_thcrap::Progress;
use th08_thcrap::manage::{self, ServerLang};
#[cfg(not(target_arch = "wasm32"))]
use th08_thcrap::{thcrap::Vanilla, thpatch::Client};

// Where the importer reads the vanilla text (it maps the translation's keys through th08.dat / th08.pak).
#[cfg(not(target_arch = "wasm32"))]
pub enum Data {
    Dir(PathBuf),
    Bytes {
        name: String,
        bytes: Vec<u8>,
    },
    // An open file holding the content file at `off..off + len` (an APK asset's descriptor); closed after reading.
    #[cfg(unix)]
    Fd {
        fd: std::os::fd::OwnedFd,
        off: u64,
        len: usize,
        name: String,
    },
}

#[cfg(not(target_arch = "wasm32"))]
impl Data {
    fn vanilla(self) -> anyhow::Result<Vanilla> {
        match self {
            Data::Dir(d) => Vanilla::from_game_dir(&d),
            Data::Bytes { name, bytes } => Vanilla::from_bytes(&name, bytes),
            #[cfg(unix)]
            Data::Fd { fd, off, len, name } => {
                use std::os::unix::fs::FileExt as _;
                let f = std::fs::File::from(fd);
                let mut b = vec![0; len];
                f.read_exact_at(&mut b, off)?;
                Vanilla::from_bytes(&name, b)
            }
        }
    }
}

// The outcome of the last job, for the screen to word.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Installed { lang: String, percent: f64 },
    Removed { lang: String },
    Failed { lang: String, error: String },
}

#[derive(Clone, Debug, Default)]
pub struct State {
    // The running job: a language code, or "" for the list.
    pub busy: Option<String>,
    // Files of the running download done / total; `building` once the download is over.
    pub files: (usize, usize),
    pub building: bool,
    pub last_line: String,
    pub server: Option<Vec<ServerLang>>,
    pub outcome: Option<Outcome>,
    // Bumped whenever the installed packs change.
    pub generation: u64,
}

type Repaint = Arc<dyn Fn() + Send + Sync>;

pub struct Manager {
    config: PathBuf,
    state: Arc<Mutex<State>>,
    repaint: Repaint,
}

struct Report {
    state: Arc<Mutex<State>>,
    repaint: Repaint,
}

impl Progress for Report {
    fn line(&self, s: &str) {
        log::info!("translations: {s}");
        s.trim()
            .clone_into(&mut self.state.lock().unwrap().last_line);
        (self.repaint)();
    }

    fn files(&self, done: usize, total: usize) {
        self.state.lock().unwrap().files = (done, total);
        (self.repaint)();
    }
}

impl Manager {
    // `config` = the config folder (packs go to `<config>/packs`); `repaint` wakes the screen after a change.
    pub fn new(config: &Path, repaint: impl Fn() + Send + Sync + 'static) -> Self {
        Self {
            config: config.to_path_buf(),
            state: Arc::default(),
            repaint: Arc::new(repaint),
        }
    }

    pub fn config(&self) -> &Path {
        &self.config
    }

    pub fn packs_dir(&self) -> PathBuf {
        manage::packs_dir(&self.config)
    }

    pub fn state(&self) -> State {
        self.state.lock().unwrap().clone()
    }

    pub fn busy(&self) -> bool {
        self.state.lock().unwrap().busy.is_some()
    }

    fn report(&self) -> Report {
        Report {
            state: self.state.clone(),
            repaint: self.repaint.clone(),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Manager {
    // Runs `job` on a new thread unless one is running; false = refused.
    fn spawn(
        &self,
        what: &str,
        job: impl FnOnce(&Report) -> Option<Outcome> + Send + 'static,
    ) -> bool {
        {
            let mut s = self.state.lock().unwrap();
            if s.busy.is_some() {
                return false;
            }
            s.busy = Some(what.to_string());
            s.files = (0, 0);
            s.building = false;
            s.last_line.clear();
        }
        let r = self.report();
        std::thread::spawn(move || {
            let out = job(&r);
            let mut s = r.state.lock().unwrap();
            if out.is_some() {
                s.outcome = out;
            }
            s.busy = None;
            drop(s);
            (r.repaint)();
        });
        true
    }

    pub fn load_list(&self) -> bool {
        self.spawn("", |r| match manage::server_languages(&Client::new()) {
            Ok(l) => {
                r.state.lock().unwrap().server = Some(l);
                None
            }
            Err(e) => Some(Outcome::Failed {
                lang: "thpatch.net".into(),
                error: format!("{e:#}"),
            }),
        })
    }

    // Download (or update) `code` into `<config>/packs/lang_<code>`.
    pub fn install(&self, code: &str, data: Data) -> bool {
        let code = code.to_string();
        let config = self.config.clone();
        self.spawn(&code.clone(), move |r| {
            let res = data.vanilla().and_then(|v| {
                let built = Building(r);
                manage::install(&Client::new(), &code, &v, &config, &built)
            });
            let mut s = r.state.lock().unwrap();
            s.generation += 1;
            Some(match res {
                Ok(sum) => Outcome::Installed {
                    lang: code,
                    percent: sum.percent,
                },
                Err(e) => Outcome::Failed {
                    lang: code,
                    error: format!("{e:#}"),
                },
            })
        })
    }

    pub fn remove(&self, code: &str) -> bool {
        let code = code.to_string();
        let config = self.config.clone();
        self.spawn(&code.clone(), move |r| {
            let res = manage::remove(&code, &config);
            r.state.lock().unwrap().generation += 1;
            Some(match res {
                Ok(()) => Outcome::Removed { lang: code },
                Err(e) => Outcome::Failed {
                    lang: code,
                    error: format!("{e:#}"),
                },
            })
        })
    }
}

#[cfg(target_arch = "wasm32")]
mod web;

// Progress that marks the build step once the download reports its last file.
#[cfg(not(target_arch = "wasm32"))]
struct Building<'a>(&'a Report);

#[cfg(not(target_arch = "wasm32"))]
impl Progress for Building<'_> {
    fn line(&self, s: &str) {
        self.0.line(s);
    }

    fn files(&self, done: usize, total: usize) {
        self.0.files(done, total);
        self.0.state.lock().unwrap().building = done == total;
    }
}

// The Android settings screen's calls (JNI `Packs.call`): one JSON request, one JSON reply.
// `{"op": "status" | "list" | "install" | "remove", "config": DIR, "dir": GAME_DIR?, "code": "en"?,
//   "fd": N?, "off": N?, "len": N?, "name": "th08.pak"?}`; the reply always carries the state and the installed packs.
#[cfg(not(target_arch = "wasm32"))]
pub mod bridge {
    use super::{Data, Manager, Outcome};
    use serde_json::{Value, json};
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

    static MANAGER: Mutex<Option<Manager>> = Mutex::new(None);

    fn data(r: &Value) -> Option<Data> {
        #[cfg(unix)]
        if let Some(fd) = r["fd"]
            .as_i64()
            .and_then(|f| i32::try_from(f).ok())
            .filter(|&f| f >= 0)
        {
            use std::os::fd::FromRawFd as _;
            // SAFETY: the caller hands over a detached descriptor it no longer uses (ParcelFileDescriptor.detachFd).
            let fd = unsafe { std::os::fd::OwnedFd::from_raw_fd(fd) };
            return Some(Data::Fd {
                fd,
                off: r["off"].as_u64().unwrap_or(0),
                len: usize::try_from(r["len"].as_u64()?).ok()?,
                name: r["name"].as_str().unwrap_or("th08.dat").to_string(),
            });
        }
        let d = PathBuf::from(r["dir"].as_str()?);
        crate::launcher::is_game_dir(&d).then_some(Data::Dir(d))
    }

    fn outcome(o: &Outcome) -> Value {
        match o {
            Outcome::Installed { lang, percent } => {
                json!({"kind": "installed", "lang": lang, "percent": percent})
            }
            Outcome::Removed { lang } => json!({"kind": "removed", "lang": lang}),
            Outcome::Failed { lang, error } => {
                json!({"kind": "failed", "lang": lang, "error": error})
            }
        }
    }

    fn reply(m: &Manager, game_dir: Option<&Path>, accepted: bool) -> Value {
        let st = m.state();
        let server: Option<Vec<Value>> = st.server.as_ref().map(|l| {
            l.iter()
                .map(|x| {
                    json!({"code": x.code(), "title": x.title, "estimate": x.estimate(),
                           "base": x.base.iter().map(|b| b.strip_prefix("lang_").unwrap_or(b)).collect::<Vec<_>>(),
                           "revision": x.revision})
                })
                .collect()
        });
        let installed: Vec<Value> = crate::packs::installed(game_dir, Some(m.config()))
            .iter()
            .map(|p| {
                json!({"id": p.id, "language": p.language, "title": p.title, "revision": p.revision,
                       "coverage": p.coverage, "managed": p.managed})
            })
            .collect();
        json!({
            "accepted": accepted,
            "busy": st.busy,
            "done": st.files.0,
            "total": st.files.1,
            "building": st.building,
            "line": st.last_line,
            "server": server,
            "outcome": st.outcome.as_ref().map(outcome),
            "generation": st.generation,
            "installed": installed,
        })
    }

    pub fn handle(req: &str) -> String {
        let r: Value = match serde_json::from_str(req) {
            Ok(v) => v,
            Err(e) => return json!({"error": format!("bad request: {e}")}).to_string(),
        };
        let Some(config) = r["config"].as_str().map(PathBuf::from) else {
            return json!({"error": "no config folder"}).to_string();
        };
        let mut g = MANAGER.lock().unwrap();
        if g.as_ref().is_none_or(|m| m.config() != config) {
            *g = Some(Manager::new(&config, || {}));
        }
        let m = g.as_ref().unwrap();
        let game_dir = r["dir"].as_str().map(PathBuf::from);
        let code = r["code"].as_str().unwrap_or("");
        let accepted = match r["op"].as_str().unwrap_or("status") {
            "list" => m.load_list(),
            "install" => data(&r).is_some_and(|d| m.install(code, d)),
            "remove" => m.remove(code),
            _ => true,
        };
        reply(m, game_dir.as_deref(), accepted).to_string()
    }

    #[cfg(target_os = "android")]
    #[unsafe(no_mangle)]
    pub extern "system" fn Java_org_th08_port_Packs_call<'c>(
        mut env: jni::EnvUnowned<'c>,
        _class: jni::objects::JClass<'c>,
        req: jni::objects::JString<'c>,
    ) -> jni::objects::JString<'c> {
        env.with_env(|env| -> jni::errors::Result<jni::objects::JString<'c>> {
            let r = req.try_to_string(env)?;
            jni::objects::JString::from_str(env, handle(&r))
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn status_and_refusals() {
            let d = std::env::temp_dir().join(format!("th08-bridge-{}", std::process::id()));
            std::fs::create_dir_all(d.join("packs/lang_xx")).unwrap();
            std::fs::write(
                d.join("packs/lang_xx/pack.toml"),
                "id = \"lang_xx\"\nlanguage = \"xx\"\ntitle = \"X\"\ncoverage = 50.0\n",
            )
            .unwrap();
            let cfg = d.to_string_lossy().to_string();
            let v: Value =
                serde_json::from_str(&handle(&json!({"op": "status", "config": cfg}).to_string()))
                    .unwrap();
            assert_eq!(v["installed"][0]["language"], "xx");
            assert_eq!(v["installed"][0]["managed"], true);
            assert_eq!(v["installed"][0]["coverage"], 50.0);
            // No game data: the install is refused before any download.
            let v: Value = serde_json::from_str(&handle(
                &json!({"op": "install", "config": cfg, "code": "en", "dir": "/nonexistent"})
                    .to_string(),
            ))
            .unwrap();
            assert_eq!(v["accepted"], false);
            assert!(handle("nope").contains("bad request"));
            std::fs::remove_dir_all(&d).unwrap();
        }
    }
}
