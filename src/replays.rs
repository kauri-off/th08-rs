//! Replay browser: lists, imports, exports and deletes the .rpy files of the save dir's replay folder.

use crate::i18n::{self, Lang};
use crate::settings::{self, Kind};
use std::path::{Path, PathBuf};

// What the browser shows of a replay's info block.
#[derive(Clone, Debug, PartialEq)]
pub struct Info {
    pub shot: u8,
    pub difficulty: u8,
    // The last stage with a record (0..=8).
    pub stage: Option<usize>,
    pub score: u64,
    // "%m/%d" as recorded; the file has no year.
    pub date: String,
    pub name: String,
    pub slowdown: f32,
    pub practice: bool,
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub file: String,
    pub path: PathBuf,
    // Modification time in seconds since the Unix epoch; 0 = unknown.
    pub modified: u64,
    pub size: u64,
    // Err = damaged or unplayable, with the reason.
    pub info: Result<Info, String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sort {
    #[default]
    Date,
    Score,
}

pub fn dir(save: &Path) -> PathBuf {
    save.join("replay")
}

pub fn info(data: &[u8]) -> Result<Info, String> {
    let r = th08_core::replay::parse(data, None).map_err(|e| e.to_string())?;
    Ok(Info {
        shot: r.shot(),
        difficulty: r.difficulty(),
        stage: r.stages.iter().rposition(Option::is_some),
        score: u64::from(r.score()) * 10,
        date: r.date(),
        name: r.name().trim_end().to_string(),
        slowdown: r.slowdown(),
        practice: r.practice(),
    })
}

fn read_limited(path: &Path) -> Result<Vec<u8>, String> {
    let len = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    if len > crate::saves::MAX_REPLAY {
        return Err(format!(
            "larger than {} MiB",
            crate::saves::MAX_REPLAY >> 20
        ));
    }
    std::fs::read(path).map_err(|e| e.to_string())
}

fn entry(path: PathBuf) -> Entry {
    let meta = std::fs::metadata(&path).ok();
    let modified = meta
        .as_ref()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    Entry {
        file: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        size: meta.map_or(0, |m| m.len()),
        modified,
        info: read_limited(&path).and_then(|d| info(&d)),
        path,
    }
}

// The *.rpy files of `dir`; a missing folder lists nothing.
pub fn list(dir: &Path) -> Vec<Entry> {
    let mut v: Vec<Entry> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| {
            let e = e.ok()?;
            e.file_type().ok()?.is_file().then_some(())?;
            let p = e.path();
            p.extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("rpy"))
                .then_some(p)
        })
        .map(entry)
        .collect();
    sort(&mut v, Sort::Date);
    v
}

// Newest / highest first; damaged files last; then by file name.
pub fn sort(v: &mut [Entry], by: Sort) {
    v.sort_by(|a, b| {
        let key = |e: &Entry| match (by, &e.info) {
            (_, Err(_)) => (1, 0),
            (Sort::Date, Ok(_)) => (0, e.modified),
            (Sort::Score, Ok(i)) => (0, i.score),
        };
        let (ka, kb) = (key(a), key(b));
        ka.0.cmp(&kb.0)
            .then(kb.1.cmp(&ka.1))
            .then_with(|| a.file.cmp(&b.file))
    });
}

// A plain .rpy name from any file name: other characters become '_', the stem is cut to fit the game's limit.
pub fn clean_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or("");
    let stem = if base.len() >= 4 && base.is_char_boundary(base.len() - 4) {
        let (s, x) = base.split_at(base.len() - 4);
        if x.eq_ignore_ascii_case(".rpy") {
            s
        } else {
            base
        }
    } else {
        base
    };
    let mut s: String = stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let s2 = s.trim_start_matches('.').to_string();
    s = if s2.is_empty() { "replay".into() } else { s2 };
    s.truncate(52);
    format!("{s}.rpy")
}

// `name` if free in `dir`, else `stem_2.rpy`, `stem_3.rpy`, … (case-insensitive, for Windows and Android).
pub fn free_name(dir: &Path, name: &str) -> String {
    let taken: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| Some(e.ok()?.file_name().to_string_lossy().to_lowercase()))
        .collect();
    let free = |n: &str| !taken.contains(&n.to_lowercase());
    if free(name) {
        return name.to_string();
    }
    let stem = &name[..name.len() - 4];
    (2..)
        .map(|i| format!("{stem}_{i}.rpy"))
        .find(|n| free(n))
        .expect("a free name")
}

// Validates `data` and copies it into `dir` under a free name; returns that name.
pub fn import(dir: &Path, name: &str, data: &[u8]) -> Result<String, String> {
    if data.len() as u64 > crate::saves::MAX_REPLAY {
        return Err(format!(
            "larger than {} MiB",
            crate::saves::MAX_REPLAY >> 20
        ));
    }
    if let Some(e) = crate::saves::replay_error(data) {
        return Err(e.to_string());
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let n = free_name(dir, &clean_name(name));
    let p = dir.join(&n);
    crate::platform::write_atomic(&p, data).map_err(|e| format!("{}: {e}", p.display()))?;
    Ok(n)
}

pub fn import_file(dir: &Path, src: &Path) -> Result<String, String> {
    let data = read_limited(src)?;
    let n = import(
        dir,
        &src.file_name().unwrap_or_default().to_string_lossy(),
        &data,
    )?;
    if let Ok(t) = std::fs::metadata(src).and_then(|m| m.modified()) {
        let _ = std::fs::File::options()
            .write(true)
            .open(dir.join(&n))
            .and_then(|f| f.set_modified(t));
    }
    Ok(n)
}

// Copies the replay into `dest` under a free name; returns the new path.
pub fn export(src: &Path, dest: &Path) -> Result<PathBuf, String> {
    let name = src.file_name().unwrap_or_default().to_string_lossy();
    let p = dest.join(free_name(dest, &clean_name(&name)));
    std::fs::copy(src, &p).map_err(|e| format!("{}: {e}", p.display()))?;
    Ok(p)
}

pub fn delete(path: &Path) -> Result<(), String> {
    std::fs::remove_file(path).map_err(|e| format!("{}: {e}", path.display()))
}

// The value text of an enum setting at index `i` ("reimu&yukari", "lunatic", "4a").
fn enum_value(key: &str, i: usize) -> Option<&'static str> {
    match settings::find(key)?.kind {
        Kind::Enum(v) => v.get(i).copied(),
        _ => None,
    }
}

pub fn shot_key(shot: u8) -> Option<&'static str> {
    enum_value("player", shot.into())
}

pub fn rank_key(difficulty: u8) -> Option<&'static str> {
    enum_value("rank", difficulty.into())
}

pub fn stage_key(stage: usize) -> Option<&'static str> {
    enum_value("stage", stage)
}

// The launcher word of an enum setting's value, or `?n` for a value out of range.
fn word(lang: Lang, key: &str, v: Option<&'static str>, n: usize) -> String {
    v.and_then(|v| settings::find(key).and_then(|e| i18n::word(lang, e, v)))
        .map_or_else(|| format!("?{n}"), String::from)
}

pub fn shot_label(lang: Lang, shot: u8) -> String {
    word(lang, "player", shot_key(shot), shot.into())
}

pub fn rank_label(lang: Lang, d: u8) -> String {
    word(lang, "rank", rank_key(d), d.into())
}

pub fn stage_label(lang: Lang, s: Option<usize>) -> String {
    s.map_or_else(|| "-".into(), |s| word(lang, "stage", stage_key(s), s))
}

// The Android launcher's replay list (Replays.kt): one JSON request with an op and the replay folder.
#[cfg(not(target_arch = "wasm32"))]
pub mod bridge {
    use serde_json::{Value, json};
    use std::path::{Path, PathBuf};

    fn entry_json(e: &super::Entry) -> Value {
        let mut v = json!({
            "file": e.file,
            "path": e.path.display().to_string(),
            "modified": e.modified,
            "size": e.size,
        });
        match &e.info {
            Ok(i) => {
                v["shot"] = json!(super::shot_key(i.shot).unwrap_or(""));
                v["rank"] = json!(super::rank_key(i.difficulty).unwrap_or(""));
                v["stage"] = json!(i.stage.and_then(super::stage_key).unwrap_or(""));
                v["score"] = json!(i.score);
                v["date"] = json!(i.date);
                v["name"] = json!(i.name);
                v["slowdown"] = json!(i.slowdown);
                v["practice"] = json!(i.practice);
            }
            Err(err) => v["error"] = json!(err),
        }
        v
    }

    // Only a plain file name inside the folder.
    fn file_in(dir: &Path, r: &Value) -> Result<PathBuf, String> {
        let f = r["file"].as_str().ok_or("no file")?;
        if f.is_empty() || f.contains(['/', '\\']) || f.starts_with('.') {
            return Err(format!("bad file name {f:?}"));
        }
        Ok(dir.join(f))
    }

    fn run(r: &Value) -> Result<Value, String> {
        let dir = PathBuf::from(r["dir"].as_str().ok_or("no dir")?);
        match r["op"].as_str().unwrap_or("") {
            "list" => {
                let mut v = super::list(&dir);
                if r["sort"].as_str() == Some("score") {
                    super::sort(&mut v, super::Sort::Score);
                }
                Ok(json!({"replays": v.iter().map(entry_json).collect::<Vec<_>>()}))
            }
            "import" => {
                let src = PathBuf::from(r["src"].as_str().ok_or("no src")?);
                let data = super::read_limited(&src)?;
                let name = r["name"].as_str().unwrap_or("replay.rpy");
                Ok(json!({"file": super::import(&dir, name, &data)?}))
            }
            "delete" => {
                super::delete(&file_in(&dir, r)?)?;
                Ok(json!({}))
            }
            op => Err(format!("unknown op {op:?}")),
        }
    }

    pub fn handle(req: &str) -> String {
        let r = serde_json::from_str(req)
            .map_err(|e| format!("bad request: {e}"))
            .and_then(|r| run(&r));
        match r {
            Ok(v) => v.to_string(),
            Err(e) => json!({"error": e}).to_string(),
        }
    }

    #[cfg(target_os = "android")]
    #[unsafe(no_mangle)]
    pub extern "system" fn Java_org_th08_port_Replays_call<'c>(
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use th08_core::replay::{Replay, StageRecord};

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("th08-replays-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn rpy(shot: u8, score10: u32, stages: &[usize]) -> Vec<u8> {
        let mut r = Replay::default();
        r.info[0x6A - 0x68] = shot;
        r.info[0x6B - 0x68] = 3;
        r.info[0x6C - 0x68..0x71 - 0x68].copy_from_slice(b"11/05");
        r.info[0x72 - 0x68..0x7A - 0x68].copy_from_slice(b"KAURI   ");
        r.info[0xB0 - 0x68..0xB4 - 0x68].copy_from_slice(&score10.to_le_bytes());
        r.info[0x118 - 0x68..0x11C - 0x68].copy_from_slice(&1.5f32.to_bits().to_le_bytes());
        for &s in stages {
            r.stages[s] = Some(StageRecord {
                inputs: vec![0, 1],
                ..Default::default()
            });
        }
        r.encode(&mut th08_core::rng::Rng::seeded(1))
    }

    fn set_mtime(p: &Path, secs: u64) {
        let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs);
        std::fs::File::options()
            .write(true)
            .open(p)
            .unwrap()
            .set_modified(t)
            .unwrap();
    }

    #[test]
    fn header_fields() {
        let i = info(&rpy(4, 12_345, &[0, 1, 2, 3])).unwrap();
        assert_eq!(
            i,
            Info {
                shot: 4,
                difficulty: 3,
                stage: Some(3),
                score: 123_450,
                date: "11/05".into(),
                name: "KAURI".into(),
                slowdown: 1.5,
                practice: false,
            }
        );
        assert_eq!(shot_label(Lang::En, 4), "Reimu");
        assert_eq!(rank_label(Lang::En, 3), "Lunatic");
        assert_eq!(stage_label(Lang::En, Some(3)), "Stage 4A");
        assert_eq!(shot_label(Lang::En, 40), "?40");
    }

    #[test]
    fn listing_sorting_damaged() {
        let d = tmp("list");
        std::fs::write(d.join("th8_01.rpy"), rpy(0, 100, &[0])).unwrap();
        std::fs::write(d.join("th8_02.RPY"), rpy(1, 900, &[0, 1])).unwrap();
        let mut bad = rpy(2, 5000, &[0]);
        bad[0x70] ^= 1;
        std::fs::write(d.join("broken.rpy"), &bad).unwrap();
        std::fs::write(d.join("short.rpy"), b"T8RP").unwrap();
        std::fs::write(d.join("notes.txt"), b"x").unwrap();
        std::fs::create_dir(d.join("sub.rpy")).unwrap();
        set_mtime(&d.join("th8_01.rpy"), 2_000_000);
        set_mtime(&d.join("th8_02.RPY"), 1_000_000);
        let names = |v: &[Entry]| v.iter().map(|e| e.file.clone()).collect::<Vec<_>>();
        let mut v = list(&d);
        assert_eq!(
            names(&v),
            ["th8_01.rpy", "th8_02.RPY", "broken.rpy", "short.rpy"]
        );
        assert!(v[2].info.as_ref().unwrap_err().contains("checksum"));
        assert!(v[3].info.is_err());
        sort(&mut v, Sort::Score);
        assert_eq!(
            names(&v),
            ["th8_02.RPY", "th8_01.rpy", "broken.rpy", "short.rpy"]
        );
        assert!(list(&d.join("missing")).is_empty());
    }

    #[test]
    fn import_names_and_validation() {
        let d = tmp("import");
        let src = tmp("import-src");
        let r = rpy(0, 1, &[0]);
        let rd = dir(&d);
        assert_eq!(import(&rd, "th8_ud0001.rpy", &r).unwrap(), "th8_ud0001.rpy");
        assert_eq!(
            import(&rd, "TH8_UD0001.rpy", &r).unwrap(),
            "TH8_UD0001_2.rpy"
        );
        assert_eq!(
            import(&rd, "th8_ud0001.rpy", &r).unwrap(),
            "th8_ud0001_3.rpy"
        );
        assert_eq!(
            import(&rd, "../my run (1).RPY", &r).unwrap(),
            "my_run__1_.rpy"
        );
        assert_eq!(import(&rd, "", &r).unwrap(), "replay.rpy");
        assert!(import(&rd, "x.rpy", b"T8RP garbage").is_err());
        assert!(!rd.join("x.rpy").exists());
        std::fs::write(src.join("sent.rpy"), &r).unwrap();
        set_mtime(&src.join("sent.rpy"), 1_234_567);
        assert_eq!(import_file(&rd, &src.join("sent.rpy")).unwrap(), "sent.rpy");
        let e = list(&rd)
            .into_iter()
            .find(|e| e.file == "sent.rpy")
            .unwrap();
        assert_eq!(e.modified, 1_234_567);
        let out = tmp("export");
        let p = export(&rd.join("sent.rpy"), &out).unwrap();
        assert_eq!(p, out.join("sent.rpy"));
        assert_eq!(
            export(&rd.join("sent.rpy"), &out).unwrap(),
            out.join("sent_2.rpy")
        );
        assert_eq!(std::fs::read(&p).unwrap(), r);
        delete(&p).unwrap();
        assert!(!p.exists());
    }

    #[test]
    fn bridge_ops() {
        let d = tmp("bridge");
        let src = d.join("in.rpy");
        std::fs::write(&src, rpy(5, 7, &[0, 8])).unwrap();
        let rd = d.join("replay");
        let req = |v: serde_json::Value| {
            serde_json::from_str::<serde_json::Value>(&bridge::handle(&v.to_string())).unwrap()
        };
        let dir = rd.display().to_string();
        let r = req(
            serde_json::json!({"op": "import", "dir": dir, "src": src.display().to_string(), "name": "in.rpy"}),
        );
        assert_eq!(r["file"], "in.rpy");
        let r = req(serde_json::json!({"op": "list", "dir": dir, "sort": "score"}));
        let e = &r["replays"][0];
        assert_eq!(
            (&e["shot"], &e["rank"], &e["stage"], &e["score"]),
            (
                &serde_json::json!("yukari"),
                &serde_json::json!("lunatic"),
                &serde_json::json!("ex"),
                &serde_json::json!(70)
            )
        );
        let r = req(serde_json::json!({"op": "delete", "dir": dir, "file": "../in.rpy"}));
        assert!(r["error"].is_string());
        req(serde_json::json!({"op": "delete", "dir": dir, "file": "in.rpy"}));
        assert!(!rd.join("in.rpy").exists());
        assert!(src.exists());
    }
}
