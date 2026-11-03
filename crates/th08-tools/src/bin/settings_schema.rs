//! Port-only tool (no spec): prints the settings table `th08::settings::ENTRIES` as JSON for the Android Compose
//! launcher (android/, MATERIAL1), so its screen is generated from the table and cannot drift.
// usage: settings_schema [OUT.json] — stdout when no path is given.

use serde_json::{Value, json};
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

fn group_id(g: Group) -> &'static str {
    match g {
        Group::GameData => "game_data",
        Group::Display => "display",
        Group::Touch => "touch",
        Group::StartGame => "start_game",
        Group::Cheats => "cheats",
        Group::Tools => "tools",
    }
}

// `platforms`: the screens that show the entry; `persist`: saved to port.cfg (else launch.cfg, one start only).
fn entry(e: &Entry) -> Value {
    json!({
        "key": e.key,
        "label": e.label,
        "help": e.help,
        "kind": kind(e.kind),
        "default": e.default,
        "group": group_id(e.group),
        "audience": match e.audience { Audience::Player => "player", Audience::Dev => "dev" },
        "platforms": e.platforms.names(),
        "persist": e.persist,
    })
}

fn schema() -> Value {
    let groups: Vec<Value> = Group::ALL
        .iter()
        .map(|&g| json!({ "id": group_id(g), "title": g.title() }))
        .collect();
    let entries: Vec<Value> = ENTRIES.iter().map(entry).collect();
    json!({ "version": 2, "groups": groups, "entries": entries })
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
        assert_eq!(s("group"), group_id(e.group));
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
    }

    #[test]
    fn schema_round_trip() {
        let s = schema();
        let entries = s["entries"].as_array().unwrap();
        assert_eq!(entries.len(), ENTRIES.len());
        // One entry of each kind, from both audiences.
        for key in [
            "vsync", "scale", "rank", "ending", "game_dir", "font", "invuln", "turbo",
        ] {
            let i = ENTRIES.iter().position(|e| e.key == key).unwrap();
            round_trip(&ENTRIES[i], &entries[i]);
        }
        let text = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&text).unwrap(), s);
    }
}
