//! Prints the settings table (with ja / ru texts) as JSON for the Android Compose launcher's generated screen.
// usage: settings_schema [OUT.json] — stdout when no path is given.

use serde_json::{Value, json};
use th08::i18n::{self, Lang};
use th08::settings::{Audience, ENTRIES, Entry, Group, Kind};

fn kind(k: Kind) -> Value {
    match k {
        Kind::Bool => json!({ "type": "bool" }),
        Kind::Int { min, max } => json!({ "type": "int", "min": min, "max": max }),
        Kind::Enum(c) => json!({ "type": "enum", "choices": c }),
        Kind::Text(f) => json!({ "type": "text", "format": f }),
        Kind::Path => json!({ "type": "path" }),
        Kind::File => json!({ "type": "file" }),
    }
}

fn words(e: &Entry, lang: Lang) -> Value {
    e.words
        .iter()
        .map(|&(v, _)| json!({ "value": v, "label": i18n::word(lang, e, v).unwrap() }))
        .collect()
}

// The display texts of an entry in one language (same shape as the English fields).
fn entry_lang(e: &Entry, lang: Lang) -> Value {
    json!({
        "label": i18n::label(lang, e),
        "summary": i18n::summary(lang, e),
        "help": i18n::help(lang, e),
        "unit": i18n::unit(lang, e),
        "words": words(e, lang),
    })
}

// `summary`: the short row text, `help` the long one; `unit` / `words`: number suffix and display words by value.
// `platforms`: the screens that show the entry; `persist`: saved to port.cfg (else launch.cfg, one start only).
fn entry(e: &Entry) -> Value {
    json!({
        "key": e.key,
        "label": e.label,
        "summary": e.summary,
        "help": e.help,
        "unit": e.unit,
        "words": words(e, Lang::En),
        "kind": kind(e.kind),
        "default": e.default,
        "group": e.group.id(),
        "audience": match e.audience { Audience::Player => "player", Audience::Dev => "dev" },
        "platforms": e.platforms.names(),
        "persist": e.persist,
        "ja": entry_lang(e, Lang::Ja),
        "ru": entry_lang(e, Lang::Ru),
    })
}

fn schema() -> Value {
    let groups: Vec<Value> = Group::ALL
        .iter()
        .map(|&g| {
            json!({
                "id": g.id(),
                "title": g.title(),
                "ja": { "title": i18n::group_title(Lang::Ja, g) },
                "ru": { "title": i18n::group_title(Lang::Ru, g) },
            })
        })
        .collect();
    let entries: Vec<Value> = ENTRIES.iter().map(entry).collect();
    json!({
        "version": 3,
        "languages": Lang::ALL.map(Lang::code),
        "groups": groups,
        "entries": entries,
    })
}

fn main() -> anyhow::Result<()> {
    let text = serde_json::to_string_pretty(&schema())? + "\n";
    match std::env::args_os().nth(1) {
        Some(p) => std::fs::write(p, text)?,
        None => print!("{text}"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use th08::settings::Platforms;

    // Reads one exported entry back as Schema.kt does and compares it with the table row.
    fn round_trip(e: &Entry, j: &Value) {
        let s = |k: &str| j[k].as_str().unwrap();
        assert_eq!(
            (s("key"), s("label"), s("help"), s("default")),
            (e.key, e.label, e.help, e.default)
        );
        assert_eq!(s("group"), e.group.id());
        let k = &j["kind"];
        let back = match k["type"].as_str().unwrap() {
            "bool" => Kind::Bool,
            "int" => Kind::Int {
                min: k["min"].as_i64().unwrap(),
                max: k["max"].as_i64().unwrap(),
            },
            "enum" => match e.kind {
                Kind::Enum(c) => {
                    assert_eq!(k["choices"], json!(c));
                    e.kind
                }
                _ => panic!("{}: not an enum", e.key),
            },
            "text" => match e.kind {
                Kind::Text(f) => {
                    assert_eq!(k["format"], f);
                    e.kind
                }
                _ => panic!("{}: not text", e.key),
            },
            "path" => Kind::Path,
            "file" => Kind::File,
            t => panic!("kind {t}"),
        };
        assert_eq!(back, e.kind);
        let audience = match s("audience") {
            "player" => Audience::Player,
            "dev" => Audience::Dev,
            a => panic!("{a}"),
        };
        assert_eq!(audience, e.audience);
        let names: Vec<String> = serde_json::from_value(j["platforms"].clone()).unwrap();
        assert_eq!(
            Platforms::from_names(names.iter().map(String::as_str)),
            e.platforms
        );
        assert_eq!(j["persist"].as_bool().unwrap(), e.persist);
        assert_eq!((s("summary"), s("unit")), (e.summary, e.unit));
        let words: Vec<(&str, &str)> = j["words"]
            .as_array()
            .unwrap()
            .iter()
            .map(|w| (w["value"].as_str().unwrap(), w["label"].as_str().unwrap()))
            .collect();
        assert_eq!(words, e.words);
    }

    // The ja / ru objects of one exported entry (Schema.kt `Entry.texts`) against the i18n tables.
    fn lang_maps(e: &Entry, j: &Value) {
        for lang in Lang::TRANSLATED {
            let t = &j[lang.code()];
            let s = |k: &str| {
                t[k].as_str()
                    .unwrap_or_else(|| panic!("{}: {}.{k}", e.key, lang.code()))
            };
            assert_eq!(
                (s("label"), s("summary"), s("help"), s("unit")),
                (
                    i18n::label(lang, e),
                    i18n::summary(lang, e),
                    i18n::help(lang, e),
                    i18n::unit(lang, e)
                ),
                "{}: {}",
                e.key,
                lang.code()
            );
            let words = t["words"].as_array().unwrap();
            assert_eq!(
                words.len(),
                e.words.len(),
                "{}: {} words",
                e.key,
                lang.code()
            );
            for (w, &(v, _)) in words.iter().zip(e.words) {
                assert_eq!(w["value"], v);
                assert_eq!(w["label"].as_str(), i18n::word(lang, e, v));
            }
        }
    }

    // Every entry has a short summary in plain words; every enum choice has a display word.
    #[test]
    fn display_metadata() {
        for e in ENTRIES {
            let s = e.summary;
            assert!(
                !s.is_empty() && s.chars().count() <= 60,
                "{}: summary {s:?}",
                e.key
            );
            assert!(
                s.starts_with(|c: char| c.is_uppercase()),
                "{}: sentence case",
                e.key
            );
            assert!(
                !s.contains("--") && !s.ends_with('.'),
                "{}: plain summary",
                e.key
            );
            for (v, w) in e.words {
                assert!(!w.is_empty(), "{}: empty word for {v:?}", e.key);
                assert!(
                    e.check(v).is_ok() || v.is_empty(),
                    "{}: word for invalid {v:?}",
                    e.key
                );
            }
            if let Kind::Enum(c) = e.kind {
                for v in c {
                    assert!(
                        e.words.iter().any(|(x, _)| x == v),
                        "{}: no word for {v}",
                        e.key
                    );
                }
            }
        }
    }

    #[test]
    fn schema_round_trip() {
        let s = schema();
        let entries = s["entries"].as_array().unwrap();
        assert_eq!(entries.len(), ENTRIES.len());
        // One entry of each kind, from both audiences.
        for key in [
            "vsync",
            "scale",
            "rank",
            "ending",
            "game_dir",
            "font",
            "invuln",
            "turbo",
            "touch_move",
            "replay_speed",
            "language",
            "stage_frame",
        ] {
            let i = ENTRIES.iter().position(|e| e.key == key).unwrap();
            round_trip(&ENTRIES[i], &entries[i]);
        }
        for (g, j) in Group::ALL.iter().zip(s["groups"].as_array().unwrap()) {
            assert_eq!(j["id"], g.id());
            for lang in Lang::TRANSLATED {
                assert_eq!(j[lang.code()]["title"], i18n::group_title(lang, *g));
            }
        }
        assert_eq!(s["languages"], json!(["ja", "en", "ru"]));
        // Language maps of every entry.
        for (e, j) in ENTRIES.iter().zip(entries) {
            lang_maps(e, j);
        }
        let text = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&text).unwrap(), s);
    }
}
