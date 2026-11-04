//! Launcher and port-menu languages: ja / ru string tables, system locale detection and the egui CJK fallback font.

use crate::settings::{ENTRIES, Entry, Group};
use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

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

// Locale handed over by a platform launcher; wins over the OS lookup.
static GIVEN_LOCALE: RwLock<Option<String>> = RwLock::new(None);

pub fn set_system_locale(tag: &str) {
    *GIVEN_LOCALE.write().unwrap() = Some(tag.to_string());
}

pub fn system_locale() -> Option<String> {
    if let Some(t) = GIVEN_LOCALE.read().unwrap().clone() {
        return Some(t);
    }
    os_locale()
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
    pref.unwrap_or_else(|| system_locale().map_or(Lang::En, |t| Lang::from_locale(&t)))
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
    ("game_folder", "Game folder:"),
    ("browse", "Browse…"),
    ("folder_hint", "folder with th08.dat or th08.pak"),
    ("bundled_apk", "bundled in the APK"),
    ("no_files", "no files"),
    ("dat_found", "game files found"),
    (
        "dat_missing",
        "game files missing: the folder needs th08.dat or th08.pak, and thbgm.dat or thbgm.pak for the music",
    ),
    (
        "android_copy",
        "Copy th08.dat and thbgm.dat (or th08.pak and thbgm.pak) from your Touhou 8 folder into {dir}\n\
         e.g. adb push th08.dat thbgm.dat {dir}/ — then this screen picks them up.",
    ),
    (
        "waiting_touch",
        "Starting the game…  touch the screen to open the settings screen",
    ),
    (
        "waiting_shift",
        "Starting the game…  hold Shift to open the settings screen",
    ),
    ("loading", "Loading th08.dat…"),
    ("start_failed", "The game could not start"),
    ("not_set", "(not set)"),
    ("changed", "changed"),
    ("set_by", "set by {source}"),
    ("saved", "saved"),
    ("port_menu", "Port menu"),
    ("paused", "the game is paused"),
    ("resume", "Resume"),
    ("key_back", "Back"),
    ("controller", "Controller: {name}"),
    ("no_controller", "none"),
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
    ("cancel", "Cancel"),
    ("save", "Save"),
    ("tr_title", "Translations"),
    ("tr_installed", "Installed:"),
    (
        "tr_none",
        "no translation installed: the game shows the original Japanese text",
    ),
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
    (
        "tr_web",
        "Downloading translations is not available in the browser version yet; use the desktop or Android version.",
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
    font.into_iter()
        .chain(th08_render::text::FONT_PATHS.iter().copied())
        .find_map(|p| {
            let data = std::fs::read(p).ok()?;
            log::info!("launcher CJK font: {p}");
            Some(egui::FontData::from_owned(data))
        })
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
    let Some(data) = cjk_font(font) else {
        log::warn!("no CJK font found (set TH08_FONT); Japanese launcher text shows as boxes");
        return;
    };
    let mut fonts = egui::FontDefinitions::default();
    fonts
        .font_data
        .insert("cjk".into(), std::sync::Arc::new(data));
    for fam in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts.families.entry(fam).or_default().push("cjk".into());
    }
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

    // The web subset font holds every non-ASCII glyph of the tables and the language names.
    #[test]
    fn web_font_covers_tables() {
        use ab_glyph::Font;
        let font = ab_glyph::FontRef::try_from_slice(include_bytes!(
            "../i18n/fonts/NotoSansJP-launcher.otf"
        ))
        .unwrap();
        let words = ENTRIES.iter().flat_map(|e| e.words.iter().map(|(_, w)| *w));
        let missing: String = [source(Lang::Ja).unwrap(), source(Lang::Ru).unwrap()]
            .into_iter()
            .chain(words)
            .flat_map(str::chars)
            .filter(|c| !c.is_ascii() && font.glyph_id(*c).0 == 0)
            .collect();
        assert!(
            missing.is_empty(),
            "rerun tools/i18n_font_subset: {missing}"
        );
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
