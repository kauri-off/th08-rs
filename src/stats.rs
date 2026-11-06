//! Play statistics: an observer of the World after each calc, the append-only `stats.jsonl` and its summary.

use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use th08_core::types::SceneId;
use th08_core::world::World;

pub const FILE: &str = "stats.jsonl";
pub const VERSION: u64 = 1;
// Stage indices 0..=8 (1, 2, 3, 4A, 4B, 5, 6A, 6B, Extra).
pub const STAGES: usize = 9;
const LAST_RUNS: usize = 20;
const FAILED_CARDS: usize = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Mode {
    Story,
    Practice,
    Spell,
}

impl Mode {
    pub fn key(self) -> &'static str {
        match self {
            Mode::Story => "story",
            Mode::Practice => "practice",
            Mode::Spell => "spell",
        }
    }

    fn from_key(k: &str) -> Option<Mode> {
        [Mode::Story, Mode::Practice, Mode::Spell]
            .into_iter()
            .find(|m| m.key() == k)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndReason {
    // Back to the title or the result screen.
    Menu,
    Ending,
    // Retry or practice reload: the next run starts at once.
    Retry,
    // The window closed mid-run.
    Quit,
}

impl EndReason {
    pub fn key(self) -> &'static str {
        match self {
            EndReason::Menu => "menu",
            EndReason::Ending => "ending",
            EndReason::Retry => "retry",
            EndReason::Quit => "quit",
        }
    }

    fn from_key(k: &str) -> EndReason {
        match k {
            "ending" => EndReason::Ending,
            "retry" => EndReason::Retry,
            "quit" => EndReason::Quit,
            _ => EndReason::Menu,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Start {
        mode: Mode,
        shot: u8,
        diff: u8,
        stage: u8,
    },
    Stage {
        stage: u8,
    },
    Death {
        stage: u8,
    },
    Bomb {
        stage: u8,
    },
    Spell {
        card: u16,
        stage: u8,
        name: String,
    },
    SpellEnd {
        card: u16,
        captured: bool,
    },
    Continue {
        stage: u8,
    },
    End {
        score: u64,
        stage: u8,
        frames: u64,
        reason: EndReason,
    },
}

impl Event {
    pub fn to_json(&self, t: u64) -> Value {
        let mut v = match self {
            Event::Start {
                mode,
                shot,
                diff,
                stage,
            } => {
                json!({"ev": "start", "mode": mode.key(), "shot": shot, "diff": diff, "stage": stage})
            }
            Event::Stage { stage } => json!({"ev": "stage", "stage": stage}),
            Event::Death { stage } => json!({"ev": "death", "stage": stage}),
            Event::Bomb { stage } => json!({"ev": "bomb", "stage": stage}),
            Event::Spell { card, stage, name } => {
                json!({"ev": "spell", "card": card, "stage": stage, "name": name})
            }
            Event::SpellEnd { card, captured } => {
                json!({"ev": "spell_end", "card": card, "captured": captured})
            }
            Event::Continue { stage } => json!({"ev": "continue", "stage": stage}),
            Event::End {
                score,
                stage,
                frames,
                reason,
            } => json!({
                "ev": "end", "score": score, "stage": stage, "frames": frames, "reason": reason.key()
            }),
        };
        v["v"] = json!(VERSION);
        v["t"] = json!(t);
        v
    }

    // None for another version, an unknown event or a missing field.
    pub fn from_json(v: &Value) -> Option<(u64, Event)> {
        if v["v"].as_u64()? != VERSION {
            return None;
        }
        let t = v["t"].as_u64()?;
        let u8_of = |k: &str| v[k].as_u64().and_then(|n| u8::try_from(n).ok());
        let stage = || u8_of("stage");
        let ev = match v["ev"].as_str()? {
            "start" => Event::Start {
                mode: Mode::from_key(v["mode"].as_str()?)?,
                shot: u8_of("shot")?,
                diff: u8_of("diff")?,
                stage: stage()?,
            },
            "stage" => Event::Stage { stage: stage()? },
            "death" => Event::Death { stage: stage()? },
            "bomb" => Event::Bomb { stage: stage()? },
            "spell" => Event::Spell {
                card: u16::try_from(v["card"].as_u64()?).ok()?,
                stage: stage()?,
                name: v["name"].as_str().unwrap_or("").to_string(),
            },
            "spell_end" => Event::SpellEnd {
                card: u16::try_from(v["card"].as_u64()?).ok()?,
                captured: v["captured"].as_bool()?,
            },
            "continue" => Event::Continue { stage: stage()? },
            "end" => Event::End {
                score: v["score"].as_u64()?,
                stage: stage()?,
                frames: v["frames"].as_u64()?,
                reason: EndReason::from_key(v["reason"].as_str().unwrap_or("")),
            },
            _ => return None,
        };
        Some((t, ev))
    }
}

// The few World fields the recorder reads after a calc.
#[derive(Clone, Debug, Default, PartialEq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent World flags read per frame"
)]
pub struct Snap {
    pub scene: Option<SceneId>,
    // A game manager exists (a stage is loaded or loading).
    pub gm: bool,
    // Replay playback or an attract demo; also tool runs and cheats (score.dat not written).
    pub excluded: bool,
    pub mode: Option<Mode>,
    pub shot: u8,
    pub diff: u8,
    pub stage: u8,
    pub paused: bool,
    pub player: Option<PlayerSnap>,
    // The running card: id and vanilla name; the spell manager's "captured" bit.
    pub spell: Option<(u16, String)>,
    pub captured: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlayerSnap {
    pub deaths: i32,
    pub bombs: i32,
    pub continues: u8,
    pub score: u64,
}

impl Snap {
    pub fn of(w: &World) -> Snap {
        use th08_core::gamemgr::GameFlags;
        let g = &w.game;
        let mode = if g.spell_practice() {
            Mode::Spell
        } else if g.practice() {
            Mode::Practice
        } else {
            Mode::Story
        };
        let player = w.player.as_deref().map(|p| PlayerSnap {
            deaths: p.stats.deaths,
            bombs: p.stats.bombs_used,
            continues: p.stats.continues,
            score: u64::try_from(p.stats.score10).unwrap_or(0) * 10,
        });
        let enemies = w.enemies.as_deref();
        Snap {
            scene: Some(g.scene),
            gm: w.gm.is_some(),
            excluded: g.replay() || g.has(GameFlags::DEMO) || !g.persist,
            mode: Some(mode),
            shot: g.shot,
            diff: u8::try_from(g.difficulty.clamp(0, 4)).unwrap_or(0),
            stage: u8::try_from(g.stage.min(STAGES - 1)).unwrap_or(0),
            paused: g.paused != 0 || g.game_over != 0,
            player,
            spell: enemies.and_then(|e| {
                let s = e.g.spell.as_ref()?;
                let id = u16::try_from(s.id).ok()?;
                Some((id, th08_formats::sjis::decode(&s.name)))
            }),
            captured: enemies.is_some_and(|e| e.g.spell_captured),
        }
    }
}

struct Run {
    stage: u8,
    frames: u64,
    score: u64,
    last: Option<PlayerSnap>,
    card: Option<u16>,
}

// Turns the snapshot after each calc into events; reads only.
#[derive(Default)]
pub struct Recorder {
    run: Option<Run>,
    // A scene outside a game was seen since the last start.
    armed: bool,
}

fn in_run_scene(s: Option<SceneId>) -> bool {
    matches!(s, Some(SceneId::Game | SceneId::NextStage))
}

impl Recorder {
    pub fn observe(&mut self, s: &Snap) -> Vec<Event> {
        let mut out = Vec::new();
        if !in_run_scene(s.scene) {
            self.armed = true;
        }
        if let Some(r) = &mut self.run {
            if !in_run_scene(s.scene) {
                let reason = match s.scene {
                    Some(SceneId::Retry | SceneId::Reload) => EndReason::Retry,
                    Some(SceneId::Ending) => EndReason::Ending,
                    _ => EndReason::Menu,
                };
                self.end(reason, &mut out);
                return out;
            }
            if s.stage != r.stage && s.gm {
                r.stage = s.stage;
                out.push(Event::Stage { stage: s.stage });
            }
            if !s.paused && s.scene == Some(SceneId::Game) {
                r.frames += 1;
            }
            if let Some(p) = s.player {
                r.score = p.score;
                // A new player object or a fresh start resets the counters: no events then.
                if let Some(l) = r.last {
                    for _ in l.deaths.max(0)..p.deaths.max(0) {
                        out.push(Event::Death { stage: r.stage });
                    }
                    for _ in l.bombs.max(0)..p.bombs.max(0) {
                        out.push(Event::Bomb { stage: r.stage });
                    }
                    for _ in l.continues..p.continues {
                        out.push(Event::Continue { stage: r.stage });
                    }
                }
                r.last = Some(p);
            }
            let now = s.spell.as_ref().map(|c| c.0);
            if r.card.is_some() && r.card != now {
                out.push(Event::SpellEnd {
                    card: r.card.unwrap_or(0),
                    captured: s.captured,
                });
            }
            if let Some((id, name)) = &s.spell
                && r.card != now
            {
                out.push(Event::Spell {
                    card: *id,
                    stage: r.stage,
                    name: name.clone(),
                });
            }
            r.card = now;
            return out;
        }
        // A run starts on the first Game-scene frame of a live game (never in the middle of one we did not see begin).
        if self.armed && s.scene == Some(SceneId::Game) && s.gm {
            self.armed = false;
            if s.excluded {
                return out;
            }
            let mode = s.mode.unwrap_or(Mode::Story);
            out.push(Event::Start {
                mode,
                shot: s.shot,
                diff: s.diff,
                stage: s.stage,
            });
            self.run = Some(Run {
                stage: s.stage,
                frames: 0,
                score: s.player.map_or(0, |p| p.score),
                last: s.player,
                card: None,
            });
            // The first frame can already hold a card (spell practice).
            out.extend(self.observe_card(s));
        }
        out
    }

    fn observe_card(&mut self, s: &Snap) -> Vec<Event> {
        let Some(r) = &mut self.run else {
            return Vec::new();
        };
        let Some((id, name)) = &s.spell else {
            return Vec::new();
        };
        r.card = Some(*id);
        vec![Event::Spell {
            card: *id,
            stage: r.stage,
            name: name.clone(),
        }]
    }

    fn end(&mut self, reason: EndReason, out: &mut Vec<Event>) {
        let Some(r) = self.run.take() else { return };
        if let Some(card) = r.card {
            out.push(Event::SpellEnd {
                card,
                captured: false,
            });
        }
        out.push(Event::End {
            score: r.score,
            stage: r.stage,
            frames: r.frames,
            reason,
        });
    }

    // The window closes: a running run ends here.
    pub fn finish(&mut self) -> Vec<Event> {
        let mut out = Vec::new();
        self.end(EndReason::Quit, &mut out);
        out
    }
}

pub fn path(save: &Path) -> PathBuf {
    save.join(FILE)
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

// Appends one JSON line per event; a last line cut short by a crash is closed first, so it stays one bad line.
pub fn append(path: &Path, events: &[Event], t: u64) -> std::io::Result<()> {
    if events.is_empty() {
        return Ok(());
    }
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .read(true)
        .open(path)?;
    let len = f.metadata()?.len();
    let mut buf = String::new();
    if len > 0 {
        use std::io::{Read, Seek, SeekFrom};
        let mut last = [0u8];
        f.seek(SeekFrom::Start(len - 1))?;
        f.read_exact(&mut last)?;
        if last[0] != b'\n' {
            buf.push('\n');
        }
    }
    for e in events {
        buf.push_str(&e.to_json(t).to_string());
        buf.push('\n');
    }
    f.write_all(buf.as_bytes())
}

// Whatever drops the game (window close, back to the launcher, Android destroy) ends the run.
impl Drop for Log {
    fn drop(&mut self) {
        self.finish();
    }
}

// Every readable event in file order; bad, truncated or other-version lines are skipped.
pub fn parse(text: &str) -> Vec<(u64, Event)> {
    text.lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter_map(|v| Event::from_json(&v))
        .collect()
}

pub fn read(path: &Path) -> Vec<(u64, Event)> {
    std::fs::read(path)
        .map(|b| parse(&String::from_utf8_lossy(&b)))
        .unwrap_or_default()
}

// The game's recorder: the observer plus its file; writes are logged, never fatal.
pub struct Log {
    rec: Recorder,
    path: PathBuf,
    failed: bool,
}

impl Log {
    pub fn new(save: &Path) -> Log {
        Log {
            rec: Recorder::default(),
            path: path(save),
            failed: false,
        }
    }

    pub fn frame(&mut self, w: &World) {
        let ev = self.rec.observe(&Snap::of(w));
        self.write(&ev);
    }

    pub fn finish(&mut self) {
        let ev = self.rec.finish();
        self.write(&ev);
    }

    fn write(&mut self, ev: &[Event]) {
        if ev.is_empty() {
            return;
        }
        if let Err(e) = append(&self.path, ev, unix_now())
            && !self.failed
        {
            self.failed = true;
            log::warn!("stats: {}: {e}", self.path.display());
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RunInfo {
    pub t: u64,
    pub mode: Option<Mode>,
    pub shot: u8,
    pub diff: u8,
    pub stage: u8,
    pub score: u64,
    pub frames: u64,
    pub deaths: u32,
    pub bombs: u32,
    pub continues: u32,
    // None: the file ends inside the run, or a new start came first (crash).
    pub reason: Option<EndReason>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CardStat {
    pub card: u16,
    pub name: String,
    pub attempts: u32,
    pub captures: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Summary {
    pub frames: u64,
    pub runs: u32,
    // (shot, diff) → runs, best score.
    pub by_pair: BTreeMap<(u8, u8), (u32, u64)>,
    // Per stage: deaths, bombs.
    pub per_stage: [(u32, u32); STAGES],
    // Most failed first.
    pub failed: Vec<CardStat>,
    // Newest first.
    pub last: Vec<RunInfo>,
}

pub fn summarize(events: &[(u64, Event)]) -> Summary {
    let mut s = Summary::default();
    let mut runs: Vec<RunInfo> = Vec::new();
    let mut cur: Option<RunInfo> = None;
    let mut cards: BTreeMap<u16, CardStat> = BTreeMap::new();
    let close = |r: RunInfo, runs: &mut Vec<RunInfo>, s: &mut Summary| {
        s.frames += r.frames;
        s.runs += 1;
        let e = s.by_pair.entry((r.shot, r.diff)).or_default();
        e.0 += 1;
        e.1 = e.1.max(r.score);
        runs.push(r);
    };
    for (t, e) in events {
        match e {
            Event::Start {
                mode,
                shot,
                diff,
                stage,
            } => {
                if let Some(r) = cur.take() {
                    close(r, &mut runs, &mut s);
                }
                cur = Some(RunInfo {
                    t: *t,
                    mode: Some(*mode),
                    shot: *shot,
                    diff: *diff,
                    stage: *stage,
                    ..Default::default()
                });
            }
            Event::Stage { stage } => {
                if let Some(r) = &mut cur {
                    r.stage = *stage;
                }
            }
            Event::Death { stage } => {
                s.per_stage[usize::from(*stage).min(STAGES - 1)].0 += 1;
                if let Some(r) = &mut cur {
                    r.deaths += 1;
                }
            }
            Event::Bomb { stage } => {
                s.per_stage[usize::from(*stage).min(STAGES - 1)].1 += 1;
                if let Some(r) = &mut cur {
                    r.bombs += 1;
                }
            }
            Event::Continue { .. } => {
                if let Some(r) = &mut cur {
                    r.continues += 1;
                }
            }
            Event::Spell { card, name, .. } => {
                let c = cards.entry(*card).or_insert_with(|| CardStat {
                    card: *card,
                    ..Default::default()
                });
                c.attempts += 1;
                if !name.is_empty() {
                    c.name.clone_from(name);
                }
            }
            Event::SpellEnd { card, captured } => {
                if *captured && let Some(c) = cards.get_mut(card) {
                    c.captures += 1;
                }
            }
            Event::End {
                score,
                stage,
                frames,
                reason,
            } => {
                if let Some(mut r) = cur.take() {
                    r.score = *score;
                    r.stage = *stage;
                    r.frames = *frames;
                    r.reason = Some(*reason);
                    close(r, &mut runs, &mut s);
                }
            }
        }
    }
    if let Some(r) = cur.take() {
        close(r, &mut runs, &mut s);
    }
    let mut failed: Vec<CardStat> = cards
        .into_values()
        .filter(|c| c.attempts > c.captures)
        .collect();
    failed.sort_by(|a, b| {
        (b.attempts - b.captures)
            .cmp(&(a.attempts - a.captures))
            .then(a.card.cmp(&b.card))
    });
    failed.truncate(FAILED_CARDS);
    s.failed = failed;
    runs.reverse();
    runs.truncate(LAST_RUNS);
    s.last = runs;
    s
}

pub fn format_time(frames: u64) -> String {
    let secs = frames / 60;
    format!("{}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
}

pub fn summary_json(s: &Summary) -> Value {
    let run = |r: &RunInfo| {
        json!({
            "t": r.t, "mode": r.mode.map_or("", Mode::key), "shot": r.shot, "diff": r.diff,
            "stage": r.stage, "score": r.score, "frames": r.frames, "deaths": r.deaths, "bombs": r.bombs,
            "continues": r.continues, "reason": r.reason.map_or("", EndReason::key),
        })
    };
    json!({
        "frames": s.frames,
        "runs": s.runs,
        "pairs": s.by_pair.iter().map(|((shot, diff), (n, best))| {
            json!({"shot": shot, "diff": diff, "runs": n, "best": best})
        }).collect::<Vec<_>>(),
        "stages": s.per_stage.iter().map(|(d, b)| json!({"deaths": d, "bombs": b})).collect::<Vec<_>>(),
        "failed": s.failed.iter().map(|c| {
            json!({"card": c.card, "name": c.name, "attempts": c.attempts, "captures": c.captures})
        }).collect::<Vec<_>>(),
        "last": s.last.iter().map(run).collect::<Vec<_>>(),
    })
}

// The Android launcher's Stats screen (Stats.kt): summary and score.dat view in one JSON reply. shot / rank / stage
// are the value texts of the `player`, `rank` and `stage` settings.
#[cfg(not(target_arch = "wasm32"))]
pub mod bridge {
    use crate::replays::{rank_key, shot_key, stage_key};
    use crate::scoreview;
    use serde_json::{Value, json};
    use std::path::PathBuf;

    fn keys(v: &mut Value) {
        let n = |v: &Value, k: &str| v[k].as_u64().unwrap_or(0);
        let shot = shot_key(u8::try_from(n(v, "shot")).unwrap_or(u8::MAX)).unwrap_or("");
        let rank = rank_key(u8::try_from(n(v, "diff")).unwrap_or(u8::MAX)).unwrap_or("");
        v["shot_key"] = json!(shot);
        v["rank_key"] = json!(rank);
        if v.get("stage").is_some_and(Value::is_u64) {
            let stage = usize::try_from(n(v, "stage")).unwrap_or(usize::MAX);
            v["stage_key"] = json!(stage_key(stage).unwrap_or(""));
        }
    }

    pub fn run(r: &Value) -> Result<Value, String> {
        let dir = PathBuf::from(r["dir"].as_str().ok_or("no dir")?);
        let game = r["game"].as_str().map(PathBuf::from);
        let mut s = crate::settings::Settings::game_environment(crate::settings::PlatformDirs {
            config: Some(dir.clone()),
            save: Some(dir.clone()),
            soundfonts: Vec::new(),
        });
        // The launcher's resolved language: `auto` would ask the device locale through the game process's JNI context.
        if let Some(l) = r["lang"].as_str().and_then(crate::i18n::Lang::from_code) {
            s.set_from(crate::settings::Origin::Cli, "language", l.code())?;
        }
        let ecl = game
            .as_deref()
            .map(scoreview::ecl_names)
            .unwrap_or_default();
        let pack = crate::packs::select(&s);
        let db = pack.as_ref().map(|m| &m.text.db);
        let sum = super::summarize(&super::read(&super::path(&dir)));
        let sd = scoreview::read(&dir).map(|sd| scoreview::view(&sd, db, &ecl));
        let names: Vec<String> = (0..th08_core::scoredat::CARDS)
            .map(|i| match &sd {
                Ok(v) => v.cards[i].name.clone(),
                Err(_) => scoreview::card_name(u16::try_from(i).unwrap_or(u16::MAX), db, &ecl, b"")
                    .unwrap_or_default(),
            })
            .collect();
        let mut st = super::summary_json(&sum);
        for k in ["pairs", "last"] {
            if let Some(a) = st[k].as_array_mut() {
                a.iter_mut().for_each(keys);
            }
        }
        if let Some(a) = st["failed"].as_array_mut() {
            for c in a {
                let id = usize::try_from(c["card"].as_u64().unwrap_or(0)).unwrap_or(0);
                if let Some(n) = names.get(id).filter(|n| !n.is_empty()) {
                    c["name"] = json!(n);
                }
            }
        }
        let mut out = json!({
            "stats": st,
            "on": s.stats,
            "stage_keys": (0..super::STAGES).map(|i| stage_key(i).unwrap_or("")).collect::<Vec<_>>(),
            "shot_keys": (0..scoreview::SHOTS)
                .map(|i| shot_key(u8::try_from(i).unwrap_or(0)).unwrap_or(""))
                .collect::<Vec<_>>(),
            "rank_keys": (0..5u8).map(|i| rank_key(i).unwrap_or("")).collect::<Vec<_>>(),
        });
        match sd {
            Ok(v) => {
                let mut j = scoreview::view_json(&v);
                if let Some(a) = j["scores"].as_array_mut() {
                    a.iter_mut().for_each(keys);
                    for t in a.iter_mut() {
                        if let Some(es) = t["entries"].as_array_mut() {
                            for e in es {
                                let st = e["stage"].as_u64().unwrap_or(0);
                                e["stage_key"] = json!(
                                    usize::try_from(st).ok().and_then(stage_key).unwrap_or("")
                                );
                            }
                        }
                    }
                }
                out["scoredat"] = j;
            }
            Err(e) => out["sd_error"] = json!(e),
        }
        Ok(out)
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
    pub extern "system" fn Java_org_th08_port_Stats_call<'c>(
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

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("th08-stats-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn game(stage: u8, p: PlayerSnap) -> Snap {
        Snap {
            scene: Some(SceneId::Game),
            gm: true,
            mode: Some(Mode::Story),
            shot: 4,
            diff: 1,
            stage,
            player: Some(p),
            ..Default::default()
        }
    }

    fn scene(s: SceneId) -> Snap {
        Snap {
            scene: Some(s),
            ..Default::default()
        }
    }

    fn feed(r: &mut Recorder, snaps: &[Snap]) -> Vec<Event> {
        snaps.iter().flat_map(|s| r.observe(s)).collect()
    }

    // A story run: start, a bomb and a death on stage 1, a failed and a captured card, next stage, continue, game over.
    #[test]
    fn scripted_run() {
        let p = |deaths, bombs, continues, score| PlayerSnap {
            deaths,
            bombs,
            continues,
            score,
        };
        let card = |id: u16, cap: bool, s: Snap| Snap {
            spell: Some((id, format!("card {id}"))),
            captured: cap,
            ..s
        };
        let snaps = vec![
            scene(SceneId::Menu),
            game(0, p(0, 0, 0, 0)),
            game(0, p(0, 0, 0, 100)),
            game(0, p(0, 1, 0, 200)),
            Snap {
                paused: true,
                ..game(0, p(0, 1, 0, 200))
            },
            card(3, false, game(0, p(0, 1, 0, 300))),
            card(3, false, game(0, p(1, 1, 0, 300))),
            game(0, p(1, 1, 0, 400)),
            card(5, false, game(0, p(1, 1, 0, 500))),
            card(5, false, game(0, p(1, 1, 0, 600))),
            // Spell_End sets the captured bit as the card goes; it stays until the next card starts.
            Snap {
                captured: true,
                ..game(0, p(1, 1, 0, 700))
            },
            Snap {
                gm: false,
                player: None,
                ..scene(SceneId::NextStage)
            },
            // The next stage's player: the counters carry over.
            game(1, p(1, 1, 0, 700)),
            game(1, p(2, 1, 0, 800)),
            game(1, p(2, 1, 1, 1)),
            game(1, p(2, 1, 1, 50)),
            scene(SceneId::Result),
            scene(SceneId::Menu),
        ];
        let ev = feed(&mut Recorder::default(), &snaps);
        assert_eq!(
            ev,
            vec![
                Event::Start {
                    mode: Mode::Story,
                    shot: 4,
                    diff: 1,
                    stage: 0
                },
                Event::Bomb { stage: 0 },
                Event::Spell {
                    card: 3,
                    stage: 0,
                    name: "card 3".into()
                },
                Event::Death { stage: 0 },
                Event::SpellEnd {
                    card: 3,
                    captured: false
                },
                Event::Spell {
                    card: 5,
                    stage: 0,
                    name: "card 5".into()
                },
                Event::SpellEnd {
                    card: 5,
                    captured: true
                },
                Event::Stage { stage: 1 },
                Event::Death { stage: 1 },
                Event::Continue { stage: 1 },
                Event::End {
                    score: 50,
                    stage: 1,
                    frames: 12,
                    reason: EndReason::Menu
                },
            ]
        );
    }

    // Replays, demos and runs already going when the observer started are not recorded; retry ends one run and
    // starts the next; closing the window ends the run.
    #[test]
    fn excluded_retry_quit() {
        let p = PlayerSnap::default();
        let mut r = Recorder::default();
        let replay = Snap {
            excluded: true,
            ..game(0, p)
        };
        assert!(feed(&mut r, &[scene(SceneId::Menu), replay.clone(), replay]).is_empty());
        let mut r = Recorder::default();
        assert!(feed(&mut r, &[game(0, p), game(0, p)]).is_empty());
        let mut r = Recorder::default();
        let ev = feed(
            &mut r,
            &[
                scene(SceneId::Menu),
                game(0, p),
                scene(SceneId::Retry),
                game(0, p),
            ],
        );
        assert!(matches!(
            ev[1],
            Event::End {
                reason: EndReason::Retry,
                ..
            }
        ));
        assert!(matches!(ev[2], Event::Start { .. }));
        let spell = Snap {
            spell: Some((7, String::new())),
            ..game(0, p)
        };
        r.observe(&spell);
        let end = r.finish();
        assert_eq!(
            end[0],
            Event::SpellEnd {
                card: 7,
                captured: false
            }
        );
        assert!(matches!(
            end[1],
            Event::End {
                reason: EndReason::Quit,
                ..
            }
        ));
        assert!(r.finish().is_empty());
    }

    fn sample() -> Vec<Event> {
        let start = |shot, diff| Event::Start {
            mode: Mode::Story,
            shot,
            diff,
            stage: 0,
        };
        let end = |score, frames| Event::End {
            score,
            stage: 2,
            frames,
            reason: EndReason::Menu,
        };
        let spell = |card| Event::Spell {
            card,
            stage: 0,
            name: format!("c{card}"),
        };
        vec![
            start(0, 1),
            Event::Death { stage: 0 },
            Event::Bomb { stage: 2 },
            spell(1),
            Event::SpellEnd {
                card: 1,
                captured: false,
            },
            spell(2),
            Event::SpellEnd {
                card: 2,
                captured: true,
            },
            end(1000, 3600),
            start(0, 1),
            spell(1),
            Event::SpellEnd {
                card: 1,
                captured: false,
            },
            Event::Continue { stage: 0 },
            end(500, 60),
            start(4, 3),
            Event::Death { stage: 0 },
        ]
    }

    #[test]
    fn aggregation() {
        let ev: Vec<(u64, Event)> = sample()
            .into_iter()
            .enumerate()
            .map(|(i, e)| (i as u64, e))
            .collect();
        let s = summarize(&ev);
        assert_eq!(s.runs, 3);
        assert_eq!(s.frames, 3660);
        assert_eq!(format_time(s.frames), "0:01:01");
        assert_eq!(s.by_pair[&(0, 1)], (2, 1000));
        assert_eq!(s.by_pair[&(4, 3)], (1, 0));
        assert_eq!(s.per_stage[0], (2, 0));
        assert_eq!(s.per_stage[2], (0, 1));
        assert_eq!(
            s.failed,
            vec![CardStat {
                card: 1,
                name: "c1".into(),
                attempts: 2,
                captures: 0
            }]
        );
        assert_eq!(s.last.len(), 3);
        assert_eq!((s.last[0].shot, s.last[0].reason), (4, None));
        assert_eq!((s.last[1].score, s.last[1].continues), (500, 1));
        assert_eq!(s.last[2].deaths, 1);
        let j = summary_json(&s);
        assert_eq!(j["runs"], 3);
        assert_eq!(j["failed"][0]["name"], "c1");
    }

    // A crash mid-write leaves a cut last line: it is skipped, and the next append starts on a line of its own.
    #[test]
    fn truncated_file() {
        let d = tmp("trunc");
        let p = path(&d);
        let ev = sample();
        append(&p, &ev[..3], 10).unwrap();
        let mut bytes = std::fs::read(&p).unwrap();
        bytes.truncate(bytes.len() - 7);
        bytes.extend(b"\xff\xfe");
        std::fs::write(&p, &bytes).unwrap();
        assert_eq!(read(&p).len(), 2);
        append(&p, &ev[3..], 11).unwrap();
        let got = read(&p);
        assert_eq!(got.len(), 2 + ev.len() - 3);
        assert_eq!(got[2], (11, ev[3].clone()));
        std::fs::write(
            &p,
            "{\"v\":2,\"t\":1,\"ev\":\"death\",\"stage\":0}\n{\"v\":1,\"t\":1,\"ev\":\"nope\"}\n",
        )
        .unwrap();
        assert!(read(&p).is_empty());
        assert!(read(&d.join("missing")).is_empty());
        for e in &ev {
            assert_eq!(
                Event::from_json(&e.to_json(5)).as_ref(),
                Some(&(5, e.clone()))
            );
        }
    }

    #[test]
    fn bridge_reply() {
        let d = tmp("bridge");
        append(&path(&d), &sample(), 3).unwrap();
        std::fs::copy(
            "tests/data/score_musicroom_practice.dat",
            d.join("score.dat"),
        )
        .unwrap();
        let req = json!({"dir": d.display().to_string(), "lang": "en"}).to_string();
        let r: Value = serde_json::from_str(&bridge::handle(&req)).unwrap();
        assert_eq!(r["stats"]["runs"], 3);
        assert_eq!(r["stats"]["pairs"][0]["shot_key"], "reimu&yukari");
        assert_eq!(r["stats"]["failed"][0]["name"], "c1");
        assert_eq!(r["scoredat"]["scores"][0]["entries"][0]["stage"], 99);
        assert_eq!(r["stage_keys"][8], "ex");
        let r: Value = serde_json::from_str(&bridge::handle("{}")).unwrap();
        assert!(r["error"].is_string());
    }
}
