//! Browser translation jobs: the importer on the page's event loop, its requests fetched in rounds, packs to site storage.

use std::path::Path;

use js_sys::{Array, Promise, Uint8Array};
use th08_thcrap::Progress;
use th08_thcrap::manage;
use th08_thcrap::thcrap::Vanilla;
use th08_thcrap::thpatch::Client;
use th08_thcrap::vfs;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::{JsFuture, future_to_promise, spawn_local};

use super::{Manager, Outcome, Report};

// The in-memory folder a job builds under; emptied after each job.
pub const CONFIG: &str = "/th08";

// Requests in flight at once, and rounds before a job gives up (a language chain needs about ten).
const PARALLEL: usize = 8;
const MAX_ROUNDS: usize = 40;
const NOT_FETCHED: &str = "not fetched yet";

#[expect(clippy::needless_pass_by_value, reason = "a map_err callback")]
fn js_err(e: JsValue) -> String {
    e.as_string()
        .or_else(|| {
            e.dyn_ref::<js_sys::Error>()
                .map(|e| String::from(e.message()))
        })
        .unwrap_or_else(|| format!("{e:?}"))
}

async fn fetch_once(url: &str) -> Result<Vec<u8>, String> {
    let w = web_sys::window().ok_or("no window")?;
    let resp: web_sys::Response = JsFuture::from(w.fetch_with_str(url))
        .await
        .map_err(js_err)?
        .dyn_into()
        .map_err(js_err)?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let buf = JsFuture::from(resp.array_buffer().map_err(js_err)?)
        .await
        .map_err(js_err)?;
    Ok(Uint8Array::new(&buf).to_vec())
}

// Two attempts for a dropped connection; an HTTP status is final.
async fn fetch(url: String) -> Result<Vec<u8>, String> {
    match fetch_once(&url).await {
        Err(e) if !e.starts_with("HTTP ") => fetch_once(&url).await,
        r => r,
    }
}

// Fetches `urls`, `PARALLEL` at a time, into the client's responses; `done` counts finished requests.
async fn fetch_all(c: &Client, urls: Vec<String>, mut done: impl FnMut()) {
    for chunk in urls.chunks(PARALLEL) {
        let promises: Array = chunk
            .iter()
            .map(|u| {
                let u = u.clone();
                future_to_promise(async move {
                    Ok(match fetch(u).await {
                        Ok(b) => Uint8Array::from(b.as_slice()).into(),
                        Err(e) => JsValue::from_str(&e),
                    })
                })
            })
            .collect();
        let results = JsFuture::from(Promise::all(&promises))
            .await
            .map(|v| Array::from(&v));
        for (i, u) in chunk.iter().enumerate() {
            let r = match &results {
                Ok(a) => {
                    let v = a.get(i as u32);
                    match v.dyn_ref::<Uint8Array>() {
                        Some(b) => Ok(b.to_vec()),
                        None => Err(v.as_string().unwrap_or_else(|| "failed".into())),
                    }
                }
                Err(e) => Err(js_err(e.clone())),
            };
            if let Err(e) = &r {
                log::warn!("translations: GET {u}: {e}");
            }
            c.answer(u, r);
            done();
        }
    }
}

// Runs `job` until it asks for nothing the page has not fetched: each run records the URLs it lacked, they are
// fetched, and the job runs again over the larger set of responses.
async fn rounds<T>(
    c: &Client,
    r: &Report,
    mut job: impl FnMut() -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    let (mut done, mut total) = (0, 0);
    for _ in 0..MAX_ROUNDS {
        let out = job();
        let pending = c.take_pending();
        if pending.is_empty() {
            return out;
        }
        total += pending.len();
        r.files(done, total);
        fetch_all(c, pending, || {
            done += 1;
            r.files(done, total);
        })
        .await;
    }
    anyhow::bail!("the server's files did not settle after {MAX_ROUNDS} rounds")
}

// Lets the page draw (the build that follows holds the event loop for a while).
async fn yield_frame() {
    let p = Promise::new(&mut |res, _| {
        if let Some(w) = web_sys::window() {
            let _ = w.set_timeout_with_callback_and_timeout_and_arguments_0(&res, 60);
        }
    });
    let _ = JsFuture::from(p).await;
}

// Progress lines only (the rounds count the files), without the misses of a round that has not fetched everything.
struct Lines<'a>(&'a Report);

impl Progress for Lines<'_> {
    fn line(&self, s: &str) {
        if !s.contains(NOT_FETCHED) {
            self.0.line(s);
        }
    }
}

// th08.pak or th08.dat among the files picked in the page.
async fn vanilla() -> anyhow::Result<Vanilla> {
    let file = crate::sources::web_content_file()
        .ok_or_else(|| anyhow::anyhow!("no th08.dat or th08.pak among the selected files"))?;
    let buf = JsFuture::from(file.array_buffer())
        .await
        .map_err(|e| anyhow::anyhow!("{}: {}", file.name(), js_err(e)))?;
    Vanilla::from_bytes(&file.name(), Uint8Array::new(&buf).to_vec())
}

async fn install(code: &str, r: &Report) -> anyhow::Result<f64> {
    let config = Path::new(CONFIG);
    let v = vanilla().await?;
    let c = Client::new();
    let f = rounds(&c, r, || manage::fetch(&c, code, config, &Lines(r))).await?;
    r.state.lock().unwrap().building = true;
    (r.repaint)();
    yield_frame().await;
    let sum = manage::install_fetched(code, &f, &v, config, r)?;
    let files = vfs::files_under(&sum.out)?;
    let zip = crate::packs::write_zip(&files)?;
    let id = sum
        .out
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| format!("lang_{code}"));
    crate::packs::web::store(&id, zip)
        .await
        .map_err(|e| anyhow::anyhow!("browser storage: {e}"))?;
    Ok(sum.percent)
}

impl Manager {
    // Starts `job` on the page's event loop unless one is running; false = refused.
    fn start<F>(&self, what: &str, job: impl FnOnce(Report) -> F + 'static) -> bool
    where
        F: Future<Output = Option<Outcome>> + 'static,
    {
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
        spawn_local(async move {
            let state = r.state.clone();
            let repaint = r.repaint.clone();
            let out = job(r).await;
            let _ = vfs::remove_dir_all(CONFIG);
            let mut s = state.lock().unwrap();
            if out.is_some() {
                s.outcome = out;
            }
            s.busy = None;
            drop(s);
            repaint();
        });
        true
    }

    pub fn load_list(&self) -> bool {
        self.start("", |r| async move {
            let c = Client::new();
            match rounds(&c, &r, || manage::server_languages(&c)).await {
                Ok(l) => {
                    r.state.lock().unwrap().server = Some(l);
                    None
                }
                Err(e) => Some(Outcome::Failed {
                    lang: "thpatch.net".into(),
                    error: format!("{e:#}"),
                }),
            }
        })
    }

    // Download (or update) `code` and keep its pack in the site storage.
    pub fn install(&self, code: &str) -> bool {
        let code = code.to_string();
        self.start(&code.clone(), |r| async move {
            let res = install(&code, &r).await;
            r.state.lock().unwrap().generation += 1;
            Some(match res {
                Ok(percent) => Outcome::Installed {
                    lang: code,
                    percent,
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
        self.start(&code.clone(), |r| async move {
            let res = crate::packs::web::delete(&format!("lang_{code}")).await;
            r.state.lock().unwrap().generation += 1;
            Some(match res {
                Ok(()) => Outcome::Removed { lang: code },
                Err(error) => Outcome::Failed { lang: code, error },
            })
        })
    }
}

impl Manager {
    // The browser's manager: jobs build in memory, packs land in the site storage.
    pub fn browser() -> Self {
        Self::new(Path::new(CONFIG), || {})
    }
}
