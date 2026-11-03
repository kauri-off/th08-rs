//! Port-only tool (no spec): prints the settings table `th08::settings::ENTRIES` as JSON for the Android Compose
//! launcher (android/, MATERIAL1), so its screen is generated from the table and cannot drift.
// usage: settings_schema [OUT.json] — stdout when no path is given.

use serde_json::{Value, json};
use th08::settings::{ENTRIES, Entry, Group, Kind};

// Platforms an entry means something on; the Compose launcher shows only the "android" ones.
fn platforms(e: &Entry) -> &'static [&'static str] {
    const ALL: &[&str] = &["desktop", "web", "android"];
    const NATIVE: &[&str] = &["desktop", "android"];
    match e.key {
        // Window size / fullscreen / the egui launcher switch: no meaning in an Android activity.
        "scale" | "fullscreen" | "launcher" => &["desktop", "web"],
        // Folder pickers and font files are desktop-only; Android has fixed folders and system CJK fonts.
        "game_dir" | "save_dir" | "font" | "soundfont" | "touch_mouse" => &["desktop"],
        // Written by the in-game layout editor, never edited as text.
        "touch_layout" => &[],
        // Tools that write files next to the binary or read host paths.
        "shot" | "shot_size" | "drawlog" | "anm" | "anm_int" | "anm_pos" | "anm_mode"
        | "slot_test" => &["desktop", "web"],
        _ if e.group == Group::Tools => NATIVE,
        _ => ALL,
    }
}

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

fn main() -> anyhow::Result<()> {
    let groups: Vec<Value> = Group::ALL
        .iter()
        .map(|&g| json!({ "id": group_id(g), "title": g.title(), "developer": g.developer() }))
        .collect();
    let entries: Vec<Value> = ENTRIES
        .iter()
        .map(|e| {
            json!({
                "key": e.key,
                "label": th08::launcher::label(e),
                "help": e.help,
                "kind": kind(e.kind),
                "default": e.default,
                "group": group_id(e.group),
                "platforms": platforms(e),
            })
        })
        .collect();
    let text = serde_json::to_string_pretty(
        &json!({ "version": 1, "groups": groups, "entries": entries }),
    )? + "\n";
    match std::env::args_os().nth(1) {
        Some(p) => std::fs::write(p, text)?,
        None => print!("{text}"),
    }
    Ok(())
}
