//! score.dat viewer: spell-card history per shot, high-score tables and clear flags, read-only.

use serde_json::{Value, json};
use std::path::Path;
use th08_core::scoredat::{C_ATTEMPTS, C_CAPTURES, C_PRACTICE, CARDS, ScoreDat};
use th08_core::textref::{SpellField, TextRef};
use th08_render::textdb::TextDb;

pub const FILE: &str = "score.dat";
// Shot types 0..12; index 12 of the counters is the total.
pub const SHOTS: usize = 12;
// CLRD final-stage bits.
const FINAL_A: u16 = 0x8000;
const FINAL_B: u16 = 0x4000;
// HSCR stage byte of a cleared run.
pub const STAGE_CLEARED: u8 = 99;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Card {
    pub id: u16,
    pub name: String,
    // [shot or 12] (captures, attempts), story and spell practice.
    pub story: Vec<(u32, u32)>,
    pub practice: Vec<(u32, u32)>,
}

impl Card {
    pub fn attempted(&self) -> bool {
        self.story[SHOTS].1 != 0 || self.practice[SHOTS].1 != 0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Score {
    pub name: String,
    pub score: u64,
    pub stage: u8,
    pub date: String,
    pub continues: u8,
    pub slowdown: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct View {
    pub cards: Vec<Card>,
    // (difficulty, shot) → entries, best first.
    pub scores: Vec<((u8, u8), Vec<Score>)>,
    // [shot][difficulty].
    pub clears: Vec<[Clear; 5]>,
}

// CLRD masks of one shot and difficulty: any clear, without continues.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Clear {
    pub any: u16,
    pub nc: u16,
}

impl Clear {
    fn of(any: u16, nc: u16) -> Clear {
        Clear { any, nc }
    }

    // "A B", "A*" (no continue), "-".
    pub fn label(self) -> String {
        let part = |on: bool, nc: bool, c: char| match (on, nc) {
            (_, true) => Some(format!("{c}*")),
            (true, false) => Some(c.to_string()),
            _ => None,
        };
        let bit = |m: u16, f: u16| m & f != 0;
        let v: Vec<String> = [(FINAL_A, 'A'), (FINAL_B, 'B')]
            .into_iter()
            .filter_map(|(f, c)| part(bit(self.any, f), bit(self.nc, f), c))
            .collect();
        if v.is_empty() {
            "-".into()
        } else {
            v.join(" ")
        }
    }
}

// Vanilla card names from the game's ECL (spell block, op 122), by card id; None where no file names the card.
pub fn ecl_names(dir: &Path) -> Vec<Option<String>> {
    let mut out = vec![None; CARDS];
    let Some(view) = content(dir) else {
        return out;
    };
    let names: Vec<String> = view
        .present()
        .names()
        .iter()
        .filter(|n| n.to_ascii_lowercase().ends_with(".ecl"))
        .cloned()
        .collect();
    for n in names {
        let Some(ecl) = view
            .read(&n)
            .ok()
            .and_then(|d| th08_formats::ecl::parse(d).ok())
        else {
            continue;
        };
        for (id, name) in spell_blocks(&ecl) {
            if let Some(slot) = usize::try_from(id).ok().and_then(|i| out.get_mut(i))
                && slot.is_none()
                && !name.is_empty()
            {
                *slot = Some(th08_formats::sjis::decode(&name));
            }
        }
    }
    out
}

fn content(dir: &Path) -> Option<th08_core::content::LogicView> {
    use th08_core::content::{CONTENT_FILES, CONTENT_PACK, Content, LogicView};
    let path = CONTENT_FILES
        .iter()
        .map(|n| crate::sources::find_file(dir, n))
        .find(|p| p.is_file())?;
    let raw = std::fs::read(&path).ok()?;
    let pack = path
        .file_name()
        .is_some_and(|f| f.to_string_lossy().eq_ignore_ascii_case(CONTENT_PACK));
    let c = if pack {
        Content::Pack(th08_formats::pack::Pack::from_bytes(raw).ok()?)
    } else {
        Content::Archive(th08_formats::archive::Archive::from_bytes(raw).ok()?)
    };
    Some(LogicView::new(c))
}

// Every op-122 block of every sub, in file order.
fn spell_blocks(ecl: &th08_formats::ecl::Ecl) -> Vec<(i16, Vec<u8>)> {
    let mut out = Vec::new();
    for &start in &ecl.subs {
        let mut off = start;
        while let Some(ins) = ecl.ins(off) {
            if ins.op == -1 || off + ins.size > ecl.data.len() {
                break;
            }
            if ins.op == 122 {
                let b = th08_formats::ecl::SpellBlock::parse(ecl.bytes(&ins));
                out.push((b.id, b.name));
            }
            off += ins.size;
        }
    }
    out
}

fn cstr(b: &[u8]) -> &[u8] {
    &b[..b.iter().position(|&c| c == 0).unwrap_or(b.len())]
}

// The shown name of a card: the pack's translation, the game data's name, the name score.dat keeps, else None.
pub fn card_name(
    id: u16,
    db: Option<&TextDb>,
    ecl: &[Option<String>],
    stored: &[u8],
) -> Option<String> {
    let key = TextRef::Spell {
        card: id,
        field: SpellField::Name,
    };
    if let Some(t) = db.and_then(|d| d.get(&key)) {
        return Some(t.to_string());
    }
    if let Some(Some(n)) = ecl.get(usize::from(id)) {
        return Some(n.clone());
    }
    let s = cstr(stored);
    (!s.is_empty()).then(|| th08_formats::sjis::decode(s))
}

pub fn view(sd: &ScoreDat, db: Option<&TextDb>, ecl: &[Option<String>]) -> View {
    let cards = sd
        .catk
        .iter()
        .enumerate()
        .take(CARDS)
        .map(|(i, c)| {
            let id = u16::try_from(i).unwrap_or(u16::MAX);
            let pairs = |base: usize| {
                (0..=SHOTS)
                    .map(|k| {
                        (
                            c.counters[base + C_CAPTURES][k],
                            c.counters[base + C_ATTEMPTS][k],
                        )
                    })
                    .collect()
            };
            Card {
                id,
                name: card_name(id, db, ecl, &c.name).unwrap_or_default(),
                story: pairs(0),
                practice: pairs(C_PRACTICE),
            }
        })
        .collect();
    let mut scores: std::collections::BTreeMap<(u8, u8), Vec<Score>> =
        std::collections::BTreeMap::new();
    for h in sd.hscr.iter().filter(|h| !h.is_placeholder()) {
        let k = (
            u8::try_from(h.difficulty()).unwrap_or(u8::MAX),
            u8::try_from(h.shot()).unwrap_or(u8::MAX),
        );
        scores.entry(k).or_default().push(Score {
            name: h.name().trim_end().to_string(),
            score: u64::from(h.score()) * 10,
            stage: h.stage(),
            date: h.date(),
            continues: h.continues(),
            slowdown: h.slowdown(),
        });
    }
    for v in scores.values_mut() {
        v.sort_by_key(|e| std::cmp::Reverse(e.score));
    }
    let clears = sd
        .clrd
        .iter()
        .take(SHOTS)
        .map(|c| std::array::from_fn(|d| Clear::of(c.any[d], c.nc[d])))
        .collect();
    View {
        cards,
        scores: scores.into_iter().collect(),
        clears,
    }
}

pub fn read(save: &Path) -> Result<ScoreDat, String> {
    let p = save.join(FILE);
    let data = std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display()))?;
    ScoreDat::parse(&data, None).map_err(|e| format!("{}: {e}", p.display()))
}

pub fn view_json(v: &View) -> Value {
    let pairs = |p: &[(u32, u32)]| p.iter().map(|(c, a)| json!([c, a])).collect::<Vec<_>>();
    json!({
        "cards": v.cards.iter().map(|c| json!({
            "id": c.id, "name": c.name, "story": pairs(&c.story), "practice": pairs(&c.practice),
        })).collect::<Vec<_>>(),
        "scores": v.scores.iter().map(|((d, s), l)| json!({
            "diff": d, "shot": s,
            "entries": l.iter().map(|e| json!({
                "name": e.name, "score": e.score, "stage": e.stage, "date": e.date,
                "continues": e.continues, "slowdown": e.slowdown,
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "clears": v.clears.iter().map(|row| row.iter().map(|c| c.label()).collect::<Vec<_>>())
            .collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const REAL: &[u8] = include_bytes!("../tests/data/score_musicroom_practice.dat");

    #[test]
    fn real_score_dat() {
        let sd = ScoreDat::parse(REAL, None).unwrap();
        let v = view(&sd, None, &[]);
        assert_eq!(v.cards.len(), CARDS);
        assert_eq!(v.clears.len(), SHOTS);
        let tried: Vec<&Card> = v.cards.iter().filter(|c| c.attempted()).collect();
        assert!(!tried.is_empty());
        for c in &tried {
            let (cap, att) = c.story[SHOTS];
            assert!(cap <= att || c.practice[SHOTS].1 > 0, "{c:?}");
            let per_shot: u32 = c.story[..SHOTS].iter().map(|p| p.1).sum();
            assert!(per_shot <= att, "{c:?}");
        }
        let c3 = &v.cards[3];
        assert_eq!(c3.name, "灯符「ファイヤフライフェノメノン」");
        assert_eq!((c3.story[SHOTS], c3.practice[SHOTS]), ((4, 4), (0, 2)));
        assert_eq!(tried.len(), 62);
        assert_eq!(v.scores.len(), 2);
        assert_eq!(v.scores[0].0, (0, 0));
        assert_eq!(
            (v.scores[0].1[0].score, v.scores[0].1[0].stage),
            (80_752_020, STAGE_CLEARED)
        );
        assert_eq!(v.clears[0][0].label(), "A");
        assert_eq!(v.clears[0][1].label(), "B*");
        assert!(!v.scores.is_empty());
        for (_, l) in &v.scores {
            assert!(l.windows(2).all(|w| w[0].score >= w[1].score));
        }
        let j = view_json(&v);
        assert_eq!(j["cards"].as_array().unwrap().len(), CARDS);
        // The viewer only reads: the file round-trips unchanged through the parser.
        assert_eq!(ScoreDat::parse(REAL, None).unwrap(), sd);
    }

    #[test]
    fn names_by_priority() {
        let mut db = TextDb::new();
        db.insert("spell/1/name".into(), "Firefly".into());
        let ecl = vec![Some("ecl0".into()), Some("ecl1".into()), None];
        let sjis = th08_formats::sjis::encode("蛍火").0;
        assert_eq!(
            card_name(1, Some(&db), &ecl, b"").as_deref(),
            Some("Firefly")
        );
        assert_eq!(card_name(0, Some(&db), &ecl, b"x").as_deref(), Some("ecl0"));
        assert_eq!(card_name(2, None, &ecl, &sjis).as_deref(), Some("蛍火"));
        assert_eq!(card_name(3, None, &ecl, &[0, 1]), None);
    }

    #[test]
    fn clear_labels() {
        assert_eq!(Clear::of(1, 1).label(), "-");
        assert_eq!(Clear::of(0x8001, 1).label(), "A");
        assert_eq!(Clear::of(0xC001, 0x4001).label(), "A B*");
    }

    #[test]
    fn game_data_names() {
        let Some(dir) = std::env::var_os("TH08_DIR").map(std::path::PathBuf::from) else {
            eprintln!("skipped: no game data (TH08_DIR unset)");
            return;
        };
        let n = ecl_names(&dir);
        assert!(n.iter().filter(|x| x.is_some()).count() > 100);
    }
}
