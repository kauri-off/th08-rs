//! Launcher and port-menu languages: ja / ru string tables, system locale detection and the egui CJK fallback font.

use crate::settings::{ENTRIES, Entry, Group};
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Lang {
    Ja,
    En,
    Ru,
}

impl Lang {
    pub const ALL: [Lang; 3] = [Lang::Ja, Lang::En, Lang::Ru];
    // Languages with a string table; English is the source text.
    pub const TRANSLATED: [Lang; 2] = [Lang::Ja, Lang::Ru];

    pub fn code(self) -> &'static str {
        match self {
            Lang::Ja => "ja",
            Lang::En => "en",
            Lang::Ru => "ru",
        }
    }

    pub fn from_code(c: &str) -> Option<Lang> {
        Lang::ALL.into_iter().find(|l| l.code() == c)
    }

    // BCP 47 (`ja-JP`, navigator.language) or POSIX (`ru_RU.UTF-8`) tag: ja / ru, everything else English.
    pub fn from_locale(tag: &str) -> Lang {
        let primary = tag
            .split(['-', '_', '.', '@'])
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        match primary.as_str() {
            "ja" => Lang::Ja,
            "ru" => Lang::Ru,
            _ => Lang::En,
        }
    }
}

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
fn os_locale() -> Option<String> {
    sys_locale::get_locale()
}

#[cfg(target_arch = "wasm32")]
fn os_locale() -> Option<String> {
    web_sys::window()?.navigator().language()
}

// The `:game` process's `Locale.getDefault()` (BCP 47), the same device locale the Compose launcher resolves `auto`
// with; read here rather than handed over so it survives Android restarting the game process alone.
#[cfg(target_os = "android")]
fn os_locale() -> Option<String> {
    use jni::objects::JString;
    use jni::{JavaVM, jni_sig, jni_str};
    let vm_ptr = ndk_context::android_context().vm();
    if vm_ptr.is_null() {
        return None;
    }
    // SAFETY: the process's JavaVM from ndk-context, valid for the lifetime of the process.
    let vm = unsafe { JavaVM::from_raw(vm_ptr.cast()) };
    let r = vm.attach_current_thread(|env| -> jni::errors::Result<String> {
        let r = (|| {
            let loc = env
                .call_static_method(
                    jni_str!("java/util/Locale"),
                    jni_str!("getDefault"),
                    jni_sig!("()Ljava/util/Locale;"),
                    &[],
                )?
                .l()?;
            let tag = env
                .call_method(
                    &loc,
                    jni_str!("toLanguageTag"),
                    jni_sig!("()Ljava/lang/String;"),
                    &[],
                )?
                .l()?;
            env.cast_local::<JString>(tag)?.try_to_string(env)
        })();
        if env.exception_check() {
            env.exception_clear();
        }
        r
    });
    r.inspect_err(|e| log::warn!("system locale via JNI failed: {e}"))
        .ok()
}

// The `language` setting resolved: None (auto) = the system locale.
pub fn resolve(pref: Option<Lang>) -> Lang {
    pref.unwrap_or_else(|| os_locale().map_or(Lang::En, |t| Lang::from_locale(&t)))
}

// Launcher / port-menu strings: id → English. `{name}` placeholders are filled by `ui_fmt`.
pub const UI: &[(&str, &str)] = &[
    ("title", "th08 — Touhou 8 port"),
    ("play", "Play"),
    ("reset", "Reset to defaults"),
    ("quit", "Quit"),
    ("back", "Back"),
    ("developer", "Developer"),
    (
        "developer_hint",
        "show cheats and tools (used for this run only, never saved)",
    ),
    ("select_folder_first", "select the game folder first"),
    ("fix_values", "fix the marked values"),
    ("dev_active", "Developer settings active: {list}"),
    (
        "cheats_nosave",
        "Cheats on: {list}. Scores and replays are not saved.",
    ),
    ("game_folder", "Game folder:"),
    ("browse", "Browse…"),
    ("folder_hint", "folder with th08.dat or th08.pak"),
    ("bundled", "bundled"),
    (
        "bundled_note",
        "This build uses its own bundled game files; a folder chosen here is used for this run only.",
    ),
    ("no_files", "no files"),
    ("dat_found", "game files found"),
    (
        "dat_missing",
        "game files missing: the folder needs th08.dat or th08.pak, and thbgm.dat or thbgm.pak for the music",
    ),
    (
        "waiting_shift",
        "Starting the game…  hold Shift to open the settings screen",
    ),
    ("loading", "Loading th08.dat…"),
    ("start_failed", "The game could not start"),
    ("last_failed", "The last run ended with an error:"),
    ("copy_report", "Copy report"),
    ("open_folder", "Open folder"),
    ("dismiss", "Dismiss"),
    ("update_available", "Version {version} is available"),
    ("update_notes", "What is new"),
    ("update_download", "Download"),
    ("update_install", "Update"),
    ("update_cancel", "Cancel"),
    (
        "update_downloading",
        "Downloading version {version}… {progress}",
    ),
    ("update_verifying", "Checking the download…"),
    (
        "update_installing",
        "Installing… the launcher closes now and the game starts again when the update is in.",
    ),
    ("update_fail_network", "Download failed: {err}"),
    (
        "update_fail_sums",
        "The release has no checksum for the installer; use Download.",
    ),
    (
        "update_fail_hash",
        "The download does not match its SHA-256 checksum and was deleted.",
    ),
    (
        "update_fail_install",
        "The installer could not start: {err}",
    ),
    ("update_cancelled", "The update was cancelled."),
    ("save_failed", "Could not save {name}: {err}"),
    ("not_set", "(not set)"),
    ("changed", "changed"),
    ("set_by", "set by {source}"),
    ("saved", "saved"),
    ("port_menu", "Port menu"),
    ("paused", "the game is paused"),
    ("resume", "Resume"),
    ("paused_title", "Pause"),
    ("volume", "Volume"),
    ("quit_title", "Quit to title…"),
    ("stage_line", "Stage {stage} · {difficulty}"),
    ("settings_heading", "Settings"),
    ("settings_caption", "Port options"),
    ("retry_title", "Give up and retry…"),
    ("quit_ask", "Quit to title?"),
    ("retry_ask", "Give up and retry?"),
    ("quit_note", "This run ends here. Its replay is not saved."),
    (
        "retry_note",
        "This run starts again from the beginning. Its replay is not saved.",
    ),
    ("keep_playing", "Keep playing"),
    ("quit_do", "Quit to title"),
    ("retry_do", "Retry"),
    ("quick_settings", "Quick settings"),
    ("help_resume", "Close this menu and return to the stage."),
    ("help_screenshot", "Save a PNG picture of the paused game."),
    (
        "help_settings",
        "Sound, display, accessibility and language options.",
    ),
    ("help_touch_layout", "Move and resize the touch controls."),
    (
        "help_speed",
        "Game speed: slow motion for practice, or the replay speed.",
    ),
    ("help_retry", "Give up this run and start it again."),
    ("help_quit", "End this run and go back to the title screen."),
    ("help_keep", "Go back to the pause menu."),
    ("help_play", "Start the game with these settings."),
    ("reset_page", "Reset this page…"),
    ("reset_page_ask", "Reset this page? Press again"),
    (
        "help_reset_page",
        "Set every option on this page back to its default.",
    ),
    ("word_off", "Off"),
    ("word_on", "On"),
    ("device_keyboard", "Keyboard"),
    ("device_touch", "Touch"),
    ("hint_move", "move"),
    ("hint_change", "change"),
    ("hint_select", "select"),
    ("hint_back", "back"),
    ("hint_resume", "resume"),
    ("hint_tabs", "tabs"),
    ("hint_quit", "quit"),
    ("launcher_title", "Imperishable Night"),
    ("launcher_caption", "Touhou 8 port · {version}"),
    ("change", "Change…"),
    ("more_title", "More"),
    ("touch_layout", "Touch layout"),
    ("version", "ver {version}"),
    ("touch_edit", "Touch controls layout"),
    (
        "touch_hint",
        "Drag a control to move it; pinch or the slider resizes it",
    ),
    ("touch_landscape", "Landscape layout"),
    ("touch_portrait", "Portrait layout"),
    ("touch_size", "Size"),
    ("touch_opacity", "Opacity"),
    ("touch_shown", "Shown"),
    ("touch_reset", "Reset layout"),
    ("touch_mirror", "Mirror"),
    ("touch_move", "MOVE"),
    ("touch_shot", "SHOT"),
    ("touch_bomb", "BOMB"),
    ("touch_focus", "FOCUS"),
    ("touch_pause", "PAUSE"),
    ("touch_skip", "SKIP"),
    ("touch_auto", "AUTO"),
    ("touch_menu", "MENU"),
    ("touch_ok", "OK"),
    ("touch_back", "BACK"),
    ("touch_next", "NEXT"),
    ("replay_paused", "Paused"),
    ("replay_rate", "{rate}×"),
    ("cancel", "Cancel"),
    ("save", "Save"),
    ("saves_title", "Saves"),
    (
        "saves_hint",
        "Scores, replays and game options in one zip, with your settings; an import keeps a zip of the current \
         files first.",
    ),
    ("saves_export", "Export saves…"),
    ("saves_import", "Import saves…"),
    ("saves_port_cfg", "Also import the settings"),
    (
        "saves_port_cfg_hint",
        "port.cfg rows from the zip; off by default, as folder paths in it may not fit this device",
    ),
    ("saves_exported", "{n} files saved to {path}"),
    ("saves_imported", "{n} files imported"),
    (
        "saves_imported_backup",
        "{n} files imported; the previous files are in {backup}",
    ),
    ("saves_export_failed", "Export failed: {err}"),
    (
        "saves_import_failed",
        "Import failed, nothing changed: {err}",
    ),
    ("tr_title", "Translations"),
    ("tr_installed", "Installed:"),
    (
        "tr_none",
        "no translation installed: the game shows the original Japanese text",
    ),
    ("tr_missing", "{code} (not installed: original text)"),
    ("tr_server", "Available from Touhou Patch Center:"),
    ("tr_load", "Show the languages"),
    ("tr_loading", "Loading the language list…"),
    ("tr_filter", "filter"),
    ("tr_install", "Download"),
    ("tr_update", "Update"),
    ("tr_remove", "Remove"),
    ("tr_up_to_date", "up to date"),
    ("tr_translated", "{pct}% translated"),
    ("tr_estimate", "about {pct}% of the texts"),
    ("tr_on_top", "builds on {base}"),
    ("tr_working", "{lang}: {done} / {total} files"),
    ("tr_building", "{lang}: building the pack…"),
    ("tr_done", "{lang} installed: {pct}% translated"),
    ("tr_removed", "{lang} removed"),
    ("tr_failed", "{lang}: {err}"),
    (
        "tr_needs_data",
        "select the game folder first: the import reads th08.dat or th08.pak",
    ),
    (
        "tr_attribution",
        "Translations by Touhou Patch Center (thpatch.net) and its contributors, CC BY-SA 4.0; the game text \
         is by Team Shanghai Alice. Packs are built on this device into {dir}, for personal use.",
    ),
    ("tr_site_storage", "this browser's site storage"),
    ("keys_press", "press a key…"),
    ("keys_add", "add a key"),
    ("keys_conflict", "also bound to: {list}"),
    ("keys_original", "Original keys"),
    ("keys_wasd", "WASD + J / K / L"),
    (
        "keys_hint",
        "Click a key to remove it. A key in two rows sends both inputs. F1–F12 and Alt stay port keys.",
    ),
    ("pad_none", "No gamepad connected"),
    ("pad_connected", "Gamepad: {name}"),
    ("pad_press", "press a button on the pad… {s} s"),
    ("pad_add", "add a button"),
    ("pad_defaults", "Defaults (th08.cfg)"),
    (
        "pad_mode_cfg",
        "Mapping: the game's own pad config (th08.cfg)",
    ),
    (
        "pad_mode_port",
        "Mapping: the rows below (th08.cfg's pad buttons are ignored)",
    ),
    ("pad_stick", "Left stick"),
    ("pad_dpad", "D-pad"),
    (
        "pad_hint",
        "While every row is at its default, the game maps the pad with th08.cfg and its in-game Key Config, as \
         before. Change a row and these rows map the pad instead: th08.cfg's pad buttons and its Shot Slow option \
         no longer apply. The d-pad always steers. Select opens the port menu.",
    ),
    ("screenshot", "Screenshot"),
    ("screenshot_saved", "Screenshot saved: {path}"),
    ("screenshot_failed", "Screenshot failed: {err}"),
    ("slow_motion", "Speed"),
    (
        "slow_nosave",
        "Slow motion was used: score.dat and replays are not saved until the game is restarted.",
    ),
    ("now_playing", "Now playing: {title}"),
    ("tab_settings", "Settings"),
    ("tab_replays", "Replays"),
    ("rp_import", "Import replays…"),
    ("rp_refresh", "Refresh"),
    ("rp_sort", "Sort by"),
    ("rp_sort_date", "date"),
    ("rp_sort_score", "score"),
    (
        "rp_hint",
        "The replays in the save folder. Drop .rpy files on this window to import them.",
    ),
    ("rp_none", "No replays in {dir}"),
    ("rp_drop", "Drop to import the replays"),
    ("rp_detail", "{file} · {date} · slowdown {slow}%"),
    ("rp_practice", "practice"),
    ("rp_damaged", "damaged: {err}"),
    ("rp_watch", "Watch"),
    ("rp_export", "Export…"),
    ("rp_delete", "Delete"),
    ("rp_delete_confirm", "Delete {file}? This cannot be undone."),
    ("rp_deleted", "{file} deleted"),
    ("rp_imported", "{n} replays imported"),
    ("rp_import_failed", "{file} not imported: {err}"),
    ("rp_exported", "Saved to {path}"),
    ("rp_export_failed", "Export failed: {err}"),
    ("rp_no_save", "No save folder: replays cannot be imported"),
    ("rp_video", "Export video…"),
    (
        "rp_video_hint",
        "Renders the replay with its music and sound effects into an .mp4 or .mkv file (needs ffmpeg).",
    ),
    ("rp_video_title", "Export video: {file}"),
    ("rp_video_scale", "Size"),
    ("rp_video_inputs", "Input display"),
    ("rp_video_hitboxes", "Hitboxes"),
    ("rp_video_save", "Save as…"),
    (
        "rp_video_running",
        "Exporting {file}: frame {done} of about {total}",
    ),
    ("rp_video_done", "Video saved to {path}"),
    ("rp_video_failed", "Video export failed: {err}"),
    ("rp_video_cancelled", "Video export cancelled"),
    (
        "rp_video_no_ffmpeg",
        "Video export needs ffmpeg: install it (ffmpeg.org) so that the ffmpeg command is on the PATH.",
    ),
    ("tab_stats", "Stats"),
    (
        "st_off",
        "Play statistics are off (Game data › Play statistics): new runs are not recorded.",
    ),
    ("st_no_save", "No save folder: nothing is recorded."),
    ("st_playtime", "Play time {time} · {runs} runs"),
    (
        "st_none",
        "No runs recorded yet; every game from now on is.",
    ),
    ("st_by_pair", "Runs by character and difficulty"),
    ("st_shot", "Character"),
    ("st_diff", "Difficulty"),
    ("st_runs", "Runs"),
    ("st_best", "Best score"),
    ("st_per_stage", "Deaths and bombs per stage"),
    ("st_stage", "Stage"),
    ("st_deaths", "Deaths"),
    ("st_bombs", "Bombs"),
    ("st_failed", "Most failed spell cards"),
    ("st_card", "Spell card"),
    ("st_failures", "Failed"),
    ("st_capture_rate", "Captured / attempts"),
    ("st_last", "Last 20 runs"),
    ("st_date", "Date"),
    ("st_mode", "Mode"),
    ("st_score", "Score"),
    ("st_time", "Time"),
    ("st_end", "End"),
    (
        "st_run_detail",
        "{deaths} deaths · {bombs} bombs · {continues} continues",
    ),
    ("st_mode_story", "Game"),
    ("st_mode_practice", "Practice"),
    ("st_mode_spell", "Spell practice"),
    ("st_end_menu", "finished"),
    ("st_end_ending", "ending"),
    ("st_end_retry", "retry"),
    ("st_end_quit", "window closed"),
    ("st_end_none", "unfinished"),
    ("st_scoredat", "score.dat"),
    ("st_sd_error", "score.dat cannot be read: {err}"),
    ("st_cards", "Spell card history (captured / attempts)"),
    ("st_all", "All characters"),
    ("st_cards_all", "Show cards never attempted"),
    ("st_no", "No."),
    ("st_story", "Game"),
    ("st_practice", "Spell practice"),
    ("st_card_no", "Card No. {n}"),
    ("st_scores", "High scores"),
    ("st_empty", "None yet"),
    ("st_cleared", "All clear"),
    ("st_clears", "Clear flags"),
    (
        "st_clears_hint",
        "A / B: final stage A or B cleared; * also without continues.",
    ),
];

type Table = HashMap<String, String>;

fn source(lang: Lang) -> Option<&'static str> {
    match lang {
        Lang::Ja => Some(include_str!("../i18n/ja.toml")),
        Lang::Ru => Some(include_str!("../i18n/ru.toml")),
        Lang::En => None,
    }
}

// `[entry.scale] label = …` → `entry.scale.label`; nested tables (words) join the same way.
fn flatten(prefix: &str, t: &toml::Table, out: &mut Table) {
    for (k, v) in t {
        let id = if prefix.is_empty() {
            k.clone()
        } else {
            format!("{prefix}.{k}")
        };
        match v {
            toml::Value::Table(sub) => flatten(&id, sub, out),
            toml::Value::String(s) => {
                out.insert(id, s.clone());
            }
            other => panic!("i18n: {id} is {other:?}, expected text"),
        }
    }
}

fn parse(text: &str) -> Table {
    let t: toml::Table = text.parse().expect("i18n table");
    let mut out = Table::new();
    flatten("", &t, &mut out);
    out
}

fn table(lang: Lang) -> Option<&'static Table> {
    static JA: OnceLock<Table> = OnceLock::new();
    static RU: OnceLock<Table> = OnceLock::new();
    let cell = match lang {
        Lang::Ja => &JA,
        Lang::Ru => &RU,
        Lang::En => return None,
    };
    Some(cell.get_or_init(|| parse(source(lang).unwrap())))
}

// The translation of `id`, or the English text when the table has none.
pub fn tr<'a>(lang: Lang, id: &str, en: &'a str) -> &'a str {
    match table(lang).and_then(|t| t.get(id)) {
        Some(s) => s.as_str(),
        None => en,
    }
}

pub fn ui(lang: Lang, id: &str) -> &'static str {
    let en = UI
        .iter()
        .find(|(k, _)| *k == id)
        .unwrap_or_else(|| panic!("i18n: no UI string {id}"))
        .1;
    tr(lang, &format!("ui.{id}"), en)
}

// `ui` with `{name}` placeholders replaced.
pub fn ui_fmt(lang: Lang, id: &str, args: &[(&str, &str)]) -> String {
    let mut s = ui(lang, id).to_string();
    for (k, v) in args {
        s = s.replace(&format!("{{{k}}}"), v);
    }
    s
}

pub fn group_title(lang: Lang, g: Group) -> &'static str {
    tr(lang, &format!("group.{}", g.id()), g.title())
}

pub fn label(lang: Lang, e: &Entry) -> &'static str {
    tr(lang, &format!("entry.{}.label", e.key), e.label)
}

pub fn summary(lang: Lang, e: &Entry) -> &'static str {
    tr(lang, &format!("entry.{}.summary", e.key), e.summary)
}

pub fn help(lang: Lang, e: &Entry) -> &'static str {
    tr(lang, &format!("entry.{}.help", e.key), e.help)
}

// Only units with words (" frames") are translated; symbols ("%", "×") stay.
fn unit_translated(e: &Entry) -> bool {
    e.unit.chars().any(char::is_alphabetic)
}

pub fn unit(lang: Lang, e: &Entry) -> &'static str {
    tr(lang, &format!("entry.{}.unit", e.key), e.unit)
}

// Display word of `value` (`Entry::words`), None when the entry has none for it.
pub fn word(lang: Lang, e: &Entry, value: &str) -> Option<&'static str> {
    let (_, en) = e.words.iter().find(|(v, _)| *v == value)?;
    Some(tr(lang, &format!("entry.{}.words.{value}", e.key), en))
}

// Every id a translated table must hold, with its English text.
pub fn source_strings() -> Vec<(String, &'static str)> {
    let mut v: Vec<(String, &'static str)> =
        UI.iter().map(|(k, en)| (format!("ui.{k}"), *en)).collect();
    v.extend(
        Group::ALL
            .iter()
            .map(|g| (format!("group.{}", g.id()), g.title())),
    );
    for e in ENTRIES {
        let k = e.key;
        v.push((format!("entry.{k}.label"), e.label));
        v.push((format!("entry.{k}.summary"), e.summary));
        v.push((format!("entry.{k}.help"), e.help));
        if unit_translated(e) {
            v.push((format!("entry.{k}.unit"), e.unit));
        }
        for (val, w) in e.words {
            v.push((format!("entry.{k}.words.{val}"), *w));
        }
    }
    v
}

// The CJK face egui falls back to after its default fonts (Japanese text and the 日本語 choice in every language).
#[cfg(not(target_arch = "wasm32"))]
fn cjk_font(font: Option<&str>) -> Option<egui::FontData> {
    th08_render::text::system_font(font).map(egui::FontData::from_static)
}

// Web: a Noto Sans CJK JP subset (OFL) holding the glyphs of ja.toml; `tools/i18n_font_subset` makes it.
#[cfg(target_arch = "wasm32")]
fn cjk_font(_: Option<&str>) -> Option<egui::FontData> {
    Some(egui::FontData::from_static(include_bytes!(
        "../i18n/fonts/NotoSansJP-launcher.otf"
    )))
}

// `font` = the `font` setting, tried before the system font paths.
pub fn install_fonts(ctx: &egui::Context, font: Option<&str>) {
    let mut fonts = egui::FontDefinitions::default();
    match cjk_font(font) {
        Some(data) => {
            fonts
                .font_data
                .insert("cjk".into(), std::sync::Arc::new(data));
            for fam in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts.families.entry(fam).or_default().push("cjk".into());
            }
        }
        None => log::warn!(
            "no CJK font found (set TH08_FONT); Japanese help text falls back to the serif face"
        ),
    }
    crate::theme::add_serif(&mut fonts);
    ctx.set_fonts(fonts);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn placeholders(s: &str) -> BTreeSet<&str> {
        s.match_indices('{')
            .filter_map(|(i, _)| Some(&s[i + 1..i + 1 + s[i + 1..].find('}')?]))
            .collect()
    }

    // Every source id is translated in ja and ru, with the same placeholders, and no table holds a stale id.
    #[test]
    fn tables_complete() {
        let src = source_strings();
        let ids: BTreeSet<&str> = src.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(ids.len(), src.len(), "duplicate source id");
        for lang in Lang::TRANSLATED {
            let t = parse(source(lang).unwrap());
            let missing: Vec<&str> = ids
                .iter()
                .copied()
                .filter(|k| !t.contains_key(*k))
                .collect();
            let stale: Vec<&String> = t.keys().filter(|k| !ids.contains(k.as_str())).collect();
            assert!(missing.is_empty(), "{}: missing {missing:?}", lang.code());
            assert!(stale.is_empty(), "{}: stale {stale:?}", lang.code());
            for (id, en) in &src {
                let s = &t[id];
                assert!(!s.trim().is_empty(), "{}: {id} empty", lang.code());
                assert_eq!(
                    placeholders(s),
                    placeholders(en),
                    "{}: {id} placeholders",
                    lang.code()
                );
            }
        }
    }

    // The web CJK subset and the serif face hold every non-ASCII glyph of the tables and the language names.
    #[test]
    fn web_font_covers_tables() {
        use ab_glyph::Font;
        for data in [
            &include_bytes!("../i18n/fonts/NotoSansJP-launcher.otf")[..],
            crate::theme::SERIF_FONT,
        ] {
            let font = ab_glyph::FontRef::try_from_slice(data).unwrap();
            let words = ENTRIES.iter().flat_map(|e| e.words.iter().map(|(_, w)| *w));
            let missing: String = [source(Lang::Ja).unwrap(), source(Lang::Ru).unwrap()]
                .into_iter()
                .chain(words)
                .chain(UI.iter().map(|(_, en)| *en))
                .flat_map(str::chars)
                .filter(|c| !c.is_ascii() && font.glyph_id(*c).0 == 0)
                .collect();
            assert!(
                missing.is_empty(),
                "rerun tools/i18n_font_subset: {missing}"
            );
        }
    }

    #[test]
    fn locale_tags() {
        assert_eq!(Lang::from_locale("ja-JP"), Lang::Ja);
        assert_eq!(Lang::from_locale("ja"), Lang::Ja);
        assert_eq!(Lang::from_locale("ru_RU.UTF-8"), Lang::Ru);
        assert_eq!(Lang::from_locale("RU"), Lang::Ru);
        assert_eq!(Lang::from_locale("en-US"), Lang::En);
        assert_eq!(Lang::from_locale("de_DE@euro"), Lang::En);
        assert_eq!(Lang::from_locale("C"), Lang::En);
        assert_eq!(Lang::from_locale(""), Lang::En);
        assert_eq!(resolve(Some(Lang::Ru)), Lang::Ru);
    }

    #[test]
    fn lookups() {
        let e = crate::settings::find("rank").unwrap();
        assert_eq!(label(Lang::En, e), "Difficulty");
        assert_eq!(label(Lang::Ja, e), "難易度");
        assert_eq!(word(Lang::En, e, "hard"), Some("Hard"));
        assert_eq!(word(Lang::Ru, e, "zz"), None);
        assert_eq!(ui(Lang::En, "play"), "Play");
        assert_eq!(
            ui_fmt(Lang::En, "set_by", &[("source", "--stage")]),
            "set by --stage"
        );
        assert_eq!(group_title(Lang::Ru, Group::Display), "Экран");
    }
}
