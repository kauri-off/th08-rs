//! Launcher screen and game-folder flow: an egui settings screen generated from `settings::ENTRIES`.

use crate::i18n::{self, Lang};
use crate::settings::{self, Audience, ENTRIES, Entry, Group, Kind, Origin, Settings};
#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};
use th08_render::gfx::{Gfx, SafeArea};
use winit::event::WindowEvent;
use winit::window::Window;

// "version (channel)"; the channel is set only for Android builds.
pub fn version_line() -> String {
    match env!("TH08_CHANNEL") {
        "" => env!("TH08_VERSION").to_owned(),
        c => format!("{} ({c})", env!("TH08_VERSION")),
    }
}

// A game folder holds th08.dat or its repacked th08.pak (any letter case).
#[cfg(not(target_arch = "wasm32"))]
pub fn is_game_dir(dir: &Path) -> bool {
    th08_core::content::CONTENT_FILES
        .iter()
        .any(|n| crate::sources::find_file(dir, n).is_file())
}

// Desktop platform dirs, looked up by the entry point and passed to the settings: config =
// `directories` BaseDirs config dir + th08 ($XDG_CONFIG_HOME/th08 = ~/.config/th08, %APPDATA%\th08,
// ~/Library/Application Support/th08); saves there, else next to the binary.
#[cfg(not(target_arch = "wasm32"))]
pub fn desktop_dirs() -> settings::PlatformDirs {
    let base = directories::BaseDirs::new();
    let config = base.as_ref().map(|d| d.config_dir().join("th08"));
    let save = config.clone().or_else(exe_dir);
    let mut soundfonts: Vec<PathBuf> = [
        "/usr/share/soundfonts",
        "/usr/share/sounds/sf2",
        "/usr/share/sf2",
        "/usr/local/share/soundfonts",
        "/usr/local/share/sounds/sf2",
        "/opt/homebrew/share/soundfonts",
        "C:\\soundfonts",
    ]
    .iter()
    .map(Into::into)
    .collect();
    if let Some(d) = &base {
        soundfonts.push(d.data_dir().join("soundfonts"));
        soundfonts.push(d.home_dir().join("Library/Audio/Sounds/Banks"));
    }
    settings::PlatformDirs {
        config,
        save,
        soundfonts,
    }
}

// Directory of the running binary (save fallback when there is no config dir; bundled game files).
#[cfg(not(target_arch = "wasm32"))]
fn exe_dir() -> Option<PathBuf> {
    Some(std::env::current_exe().ok()?.parent()?.to_path_buf())
}

// Bundled game files of a local package (packaging/local): `<exe dir>/game` (Windows install), then
// `$APPDIR/usr/share/th08/game` (AppImage).
#[cfg(not(target_arch = "wasm32"))]
pub fn bundled_candidates(exe_dir: Option<&Path>, appdir: Option<&Path>) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = exe_dir.map(|d| d.join("game")).into_iter().collect();
    v.extend(appdir.map(|d| d.join("usr/share/th08/game")));
    v
}

#[cfg(not(target_arch = "wasm32"))]
fn bundled_game_dir() -> Option<PathBuf> {
    let appdir = std::env::var_os("APPDIR").map(PathBuf::from);
    bundled_candidates(exe_dir().as_deref(), appdir.as_deref())
        .into_iter()
        .find(|d| is_game_dir(d))
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, PartialEq)]
enum Choice {
    Configured(PathBuf),
    Bundled(PathBuf),
    // Folder given on the command line / TH08_DIR without th08.dat.
    Invalid(String),
    Ask,
}

// Lookup order: valid CLI / env / launcher folder; invalid CLI/env folder is an error; bundled folder (a package's own
// data beats the port.cfg folder shared by every build); valid port.cfg folder; folder dialog.
#[cfg(not(target_arch = "wasm32"))]
fn choose_game_dir(s: &Settings, bundled: impl FnOnce() -> Option<PathBuf>) -> Choice {
    let mut bundled = Some(bundled);
    if s.origin("game_dir") <= Origin::PortCfg
        && let Some(d) = bundled.take().and_then(|f| f())
    {
        return Choice::Bundled(d);
    }
    if let Some(d) = &s.game_dir {
        if is_game_dir(d) {
            return Choice::Configured(d.clone());
        }
        let msg = format!("{}: no th08.dat or th08.pak in this folder", d.display());
        if s.origin("game_dir") > Origin::PortCfg {
            return Choice::Invalid(msg);
        }
        log::warn!("port.cfg {msg}");
    }
    match bundled.and_then(|f| f()) {
        Some(d) => Choice::Bundled(d),
        None => Choice::Ask,
    }
}

// The game folder to run; Ok(None) = dialog cancelled, Err = the folder given on the command line / TH08_DIR is invalid.
#[cfg(not(target_arch = "wasm32"))]
pub fn resolve_game_dir(s: &Settings) -> Result<Option<PathBuf>, String> {
    match choose_game_dir(s, bundled_game_dir) {
        Choice::Configured(d) => Ok(Some(d)),
        Choice::Bundled(d) => {
            log::info!("bundled game folder: {}", d.display());
            Ok(Some(d))
        }
        Choice::Invalid(msg) => Err(msg),
        Choice::Ask => Ok(pick_game_dir(&s.dirs)),
    }
}

// Launcher screen: show the bundled folder in the `game_dir` row unless the command line / env names one. A build
// with bundled data never writes `game_dir`: a folder edited on screen applies to that run only.
#[cfg(not(target_arch = "wasm32"))]
pub fn prefill_bundled_game_dir(s: &mut Settings) {
    prefill_bundled(s, bundled_game_dir);
}

#[cfg(not(target_arch = "wasm32"))]
fn prefill_bundled(s: &mut Settings, bundled: impl FnOnce() -> Option<PathBuf>) {
    if let Choice::Bundled(d) = choose_game_dir(s, bundled) {
        log::info!("bundled game folder: {}", d.display());
        s.set_from(Origin::Bundled, "game_dir", &d.to_string_lossy())
            .expect("game_dir accepts any path");
    }
}

// "Cheats on: …; no saves" for the active Cheats rows (keys), None when there are none.
pub fn cheats_note(lang: Lang, keys: &[&str]) -> Option<String> {
    if keys.is_empty() {
        return None;
    }
    let names: Vec<&str> = keys
        .iter()
        .filter_map(|k| settings::find(k))
        .map(|e| i18n::label(lang, e))
        .collect();
    Some(i18n::ui_fmt(
        lang,
        "cheats_nosave",
        &[("list", &names.join(", "))],
    ))
}

// Folder dialog until a valid folder is picked (saved as port.cfg `game_dir`) or the dialog is cancelled.
#[cfg(not(target_arch = "wasm32"))]
fn pick_game_dir(dirs: &settings::PlatformDirs) -> Option<PathBuf> {
    loop {
        let Some(dir) = folder_dialog() else {
            log::info!("folder dialog cancelled or unavailable");
            eprintln!("no game folder selected (usage: th08 <game dir>, see --help)");
            return None;
        };
        if is_game_dir(&dir) {
            settings::save_port_cfg(dirs, "game_dir", &dir.to_string_lossy());
            return Some(dir);
        }
        not_game_dir_dialog(&dir);
    }
}

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
fn not_game_dir_dialog(dir: &std::path::Path) {
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title("th08")
        .set_description(format!(
            "{} has no th08.dat or th08.pak (and thbgm.dat or thbgm.pak for the music).\nSelect the Imperishable Night folder.",
            dir.display()
        ))
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
fn folder_dialog() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Select the Touhou 8 (Imperishable Night) game folder")
        .pick_folder()
}

// Android: no rfd backend; the folder comes from `game_dir` (launcher text field, port.cfg) or the bundled lookup.
#[cfg(target_os = "android")]
fn not_game_dir_dialog(dir: &std::path::Path) {
    log::error!("{} has no th08.dat or th08.pak", dir.display());
}

#[cfg(target_os = "android")]
fn folder_dialog() -> Option<PathBuf> {
    None
}

// Where the game files come from: the `game_dir` row (native), the files picked in the web page, or the APK's
// bundled assets (Android).
pub enum Folder {
    Native,
    Web(Vec<String>),
    #[cfg_attr(not(target_os = "android"), allow(dead_code))]
    Bundled(Vec<String>),
}

// What a screen pass asks the app's Shell to do (app.rs).
pub enum Action {
    Play,
    Quit,
    // Error screen: back to the settings (Shell::Error → Setup).
    Back,
    // Port menu: back to the paused game.
    Resume,
    // The Fullscreen box was changed on screen; the app applies it to the window.
    Fullscreen(bool),
}

// What the screen shows; one per Shell state that draws with egui (app.rs).
#[derive(Clone, Debug, PartialEq)]
pub enum View {
    Settings,
    // Saved `launcher = off`: the game starts unless Shift is held.
    Waiting,
    // th08.dat / audio load (one frame, drawn before the load starts).
    Loading,
    // Start-up error, with Back / Quit.
    Error(String),
    // In-game port menu over the paused game (stub: Resume only; never logic-affecting settings).
    PortMenu,
}

// A row's value, typed by the entry's `Kind`; "" / None = unset. `text` is the port.cfg / `Settings` value text.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Bool(bool),
    Int(Option<i64>),
    Choice(String),
    // Kind::Path and Kind::File.
    Path(String),
    Text(String),
}

impl Value {
    // From value text (a `Settings` value or the table default); a bad number reads as unset.
    pub fn parse(kind: Kind, v: &str) -> Self {
        let v = v.trim();
        match kind {
            Kind::Bool => Value::Bool(settings::parse_bool(v).unwrap_or(false)),
            Kind::Int { .. } => Value::Int(
                v.parse::<i64>()
                    .ok()
                    .or_else(|| v.parse::<f64>().ok().map(|n| n.round() as i64)),
            ),
            Kind::Enum(_) => Value::Choice(v.into()),
            Kind::Path | Kind::File => Value::Path(v.into()),
            Kind::Text(_) => Value::Text(v.into()),
        }
    }

    // Booleans "on"/"off", numbers in decimal (the port.cfg format).
    pub fn text(&self) -> String {
        match self {
            Value::Bool(b) => if *b { "on" } else { "off" }.into(),
            Value::Int(n) => n.map_or(String::new(), |n| n.to_string()),
            Value::Choice(s) | Value::Path(s) | Value::Text(s) => s.clone(),
        }
    }

    pub fn is_unset(&self) -> bool {
        match self {
            Value::Bool(_) => false,
            Value::Int(n) => n.is_none(),
            Value::Choice(s) | Value::Path(s) | Value::Text(s) => s.is_empty(),
        }
    }
}

// A row's port.cfg text: paths absolute, as port.cfg resolves relative ones against the config dir.
fn saved_text(r: &Row) -> String {
    let t = r.value.text();
    #[cfg(not(target_arch = "wasm32"))]
    if matches!(r.e.kind, Kind::Path | Kind::File)
        && let Ok(p) = std::path::absolute(&t)
    {
        return p.to_string_lossy().into_owned();
    }
    t
}

// Translation packs on the settings screen: the job manager (None without a config folder) and the installed packs,
// re-listed when (job generation, game folder) changes.
struct Tr {
    manager: Option<crate::translations::Manager>,
    installed: Vec<crate::packs::Installed>,
    seen: Option<(u64, String)>,
    filter: String,
}

// One table entry as edited on screen; `dirty` = differs from the value the screen opened with.
struct Row {
    e: &'static Entry,
    value: Value,
    initial: Value,
    dirty: bool,
    origin: Origin,
}

impl Row {
    fn new(e: &'static Entry, v: &str, origin: Origin) -> Self {
        let value = Value::parse(e.kind, v);
        Self {
            e,
            initial: value.clone(),
            value,
            dirty: false,
            origin,
        }
    }

    fn default_value(&self) -> Value {
        Value::parse(self.e.kind, self.e.default)
    }

    fn set(&mut self, v: Value) {
        self.dirty = v != self.initial;
        self.value = v;
    }

    fn error(&self) -> Option<String> {
        (!self.value.is_unset()).then(|| self.e.check(&self.value.text()).err())?
    }
}

// The edited settings: one row per `ENTRIES` entry.
pub struct Model {
    rows: Vec<Row>,
    folder: Folder,
    dirs: settings::PlatformDirs,
    tr: Tr,
    // Developer view: Cheats and Tools rows and the origin tags are shown.
    pub dev: bool,
    // The game folder row holds this build's bundled data: never saved to port.cfg.
    bundled: bool,
    #[cfg(not(target_os = "android"))]
    saves: SavesUi,
    pub view: View,
    // The previous run's `last_error.txt`, shown until dismissed or Play.
    last_error: Option<String>,
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    update: Update,
}

// The update check: started on the first settings screen frame, so never when the launcher is skipped.
#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
enum Update {
    Off,
    Pending,
    Running(crate::update::Background),
}

// Save export / import on the settings screen: the port.cfg box and the last outcome (Ok / Err message).
#[cfg(not(target_os = "android"))]
#[derive(Default)]
struct SavesUi {
    port_cfg: bool,
    status: Option<Result<String, String>>,
    #[cfg(target_arch = "wasm32")]
    picker: crate::saves::Picker,
}

impl Model {
    pub fn new(s: &Settings, folder: Folder) -> Self {
        let rows = ENTRIES
            .iter()
            .map(|e| Row::new(e, s.value(e.key).unwrap_or(e.default), s.origin(e.key)))
            .collect::<Vec<Row>>();
        // A developer switch given on the command line / env opens the developer view.
        let dev = rows
            .iter()
            .any(|r| r.e.audience == Audience::Dev && r.origin > Origin::PortCfg);
        #[cfg(not(target_arch = "wasm32"))]
        let manager = s
            .dirs
            .config
            .as_deref()
            .map(|c| crate::translations::Manager::new(c, || {}));
        #[cfg(target_arch = "wasm32")]
        let manager = Some(crate::translations::Manager::browser());
        #[cfg(not(target_arch = "wasm32"))]
        let last_error = s
            .dirs
            .config
            .as_deref()
            .and_then(crate::crashlog::last_error);
        #[cfg(target_arch = "wasm32")]
        let last_error = None;
        let mut m = Self {
            rows,
            folder,
            dirs: s.dirs.clone(),
            tr: Tr {
                manager,
                installed: Vec::new(),
                seen: None,
                filter: String::new(),
            },
            dev,
            bundled: s.origin("game_dir") == Origin::Bundled,
            #[cfg(not(target_os = "android"))]
            saves: SavesUi::default(),
            view: View::Settings,
            last_error,
            #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
            update: if crate::update::wanted(s) {
                Update::Pending
            } else {
                Update::Off
            },
        };
        m.refresh_installed();
        m
    }

    // The translation job manager (tools drive it directly for screenshots).
    pub fn translations(&self) -> Option<&crate::translations::Manager> {
        self.tr.manager.as_ref()
    }

    // Re-lists the installed packs after an install / remove or a game folder change.
    fn refresh_installed(&mut self) {
        let generation = self.tr.manager.as_ref().map_or(0, |t| t.state().generation);
        let key = (generation, self.game_dir());
        if self.tr.seen.as_ref() != Some(&key) {
            let gd = (!key.1.is_empty()).then(|| std::path::PathBuf::from(&key.1));
            self.tr.installed = crate::packs::installed(gd.as_deref(), self.dirs.config.as_deref());
            self.tr.seen = Some(key);
        }
    }

    // `game_text` choices after follow / ja: the installed pack languages, (code, shown name), plus the current value
    // when no pack of it is installed (kept, so a reinstall brings it back).
    fn pack_choices(&self, lang: Lang) -> Vec<(String, String)> {
        let mut v: Vec<(String, String)> = Vec::new();
        for p in &self.tr.installed {
            if p.language != "ja" && !v.iter().any(|(c, _)| *c == p.language) {
                let name = if p.title.is_empty() {
                    p.language.clone()
                } else {
                    format!("{} — {}", p.language, p.title)
                };
                v.push((p.language.clone(), name));
            }
        }
        let cur = self.row("game_text").value.text();
        if !matches!(cur.as_str(), "" | "follow" | "ja") && !v.iter().any(|(c, _)| *c == cur) {
            let name = i18n::ui_fmt(lang, "tr_missing", &[("code", &cur)]);
            v.push((cur, name));
        }
        v
    }

    // The window's fullscreen state (F11 / Alt+Enter) mirrored into the box; saved with the other edits on Play.
    pub fn set_fullscreen(&mut self, on: bool) {
        self.row_mut("fullscreen").set(Value::Bool(on));
    }

    // The screen language: the `language` row as edited (a change applies at once), auto = the system locale.
    pub fn lang(&self) -> Lang {
        i18n::resolve(Lang::from_code(&self.row("language").value.text()))
    }

    fn row(&self, key: &str) -> &Row {
        self.rows.iter().find(|r| r.e.key == key).unwrap()
    }

    fn row_mut(&mut self, key: &str) -> &mut Row {
        self.rows.iter_mut().find(|r| r.e.key == key).unwrap()
    }

    fn game_dir(&self) -> String {
        self.row("game_dir").value.text()
    }

    fn folder_ok(&self) -> bool {
        match &self.folder {
            #[cfg(not(target_arch = "wasm32"))]
            Folder::Native => {
                let v = self.game_dir();
                !v.is_empty() && is_game_dir(Path::new(&v))
            }
            #[cfg(target_arch = "wasm32")]
            Folder::Native => false,
            Folder::Web(names) | Folder::Bundled(names) => th08_core::content::has_content(names),
        }
    }

    pub fn can_play(&self) -> bool {
        self.folder_ok() && self.rows.iter().all(|r| r.error().is_none())
    }

    // The final settings: unchanged rows keep their origin, edited ones come from the launcher.
    pub fn settings(&self) -> Settings {
        let values: Vec<(&'static Entry, String, Origin)> = self
            .rows
            .iter()
            .filter(|r| !r.value.is_unset())
            .map(|r| {
                let o = if r.dirty { Origin::Launcher } else { r.origin };
                (r.e, r.value.text(), o)
            })
            .collect();
        Settings::from_values(
            self.dirs.clone(),
            values.iter().map(|(e, v, o)| (*e, v.as_str(), *o)),
        )
    }

    // port.cfg / localStorage changes: every `persist` row edited on screen, removed when back at its default; the
    // other rows (and the game folder of a build with bundled data) apply to this run only.
    pub fn changes(&self) -> Vec<(&'static str, Option<String>)> {
        self.rows
            .iter()
            .filter(|r| r.dirty && r.e.persist && !(self.bundled && r.e.key == "game_dir"))
            .map(|r| {
                let keep = !r.value.is_unset() && r.value != r.default_value();
                (r.e.key, keep.then(|| saved_text(r)))
            })
            .collect()
    }

    // Also drops the developer keys port.cfg holds (`write_port_cfg`), even with no row edited.
    pub fn save(&self) {
        self.dismiss_last_error_file();
        let changes = self.changes();
        if !changes.is_empty() || self.dirs.config.is_some() {
            let c: Vec<(&str, Option<&str>)> =
                changes.iter().map(|(k, v)| (*k, v.as_deref())).collect();
            settings::write_port_cfg(&self.dirs, &c);
        }
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    fn dismiss_last_error_file(&self) {
        #[cfg(not(target_arch = "wasm32"))]
        if self.last_error.is_some()
            && let Some(d) = &self.dirs.config
        {
            crate::crashlog::clear_last_error(d);
        }
    }

    // "The last run failed": the failure line, Copy report / Open folder / Dismiss.
    fn last_error_ui(&mut self, ui: &mut egui::Ui, lang: Lang) {
        let Some(report) = &self.last_error else {
            return;
        };
        let mut dismiss = false;
        egui::Frame::group(ui.style())
            .stroke(egui::Stroke::new(1.0, ui.visuals().error_fg_color))
            .show(ui, |ui| {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    egui::RichText::new(i18n::ui(lang, "last_failed")).strong(),
                );
                ui.label(crate::crashlog::headline(report));
                #[cfg(not(target_arch = "wasm32"))]
                if let Some(d) = &self.dirs.config {
                    ui.label(
                        egui::RichText::new(
                            d.join(crate::crashlog::LAST_ERROR).display().to_string(),
                        )
                        .small()
                        .weak(),
                    );
                }
                ui.horizontal_wrapped(|ui| {
                    if ui.button(i18n::ui(lang, "copy_report")).clicked() {
                        ui.ctx().copy_text(report.clone());
                    }
                    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
                    if let Some(d) = &self.dirs.config
                        && ui.button(i18n::ui(lang, "open_folder")).clicked()
                        && let Err(e) = open::that_detached(d)
                    {
                        log::warn!("{}: {e}", d.display());
                    }
                    dismiss = ui.button(i18n::ui(lang, "dismiss")).clicked();
                });
            });
        ui.add_space(6.0);
        if dismiss {
            self.dismiss_last_error_file();
            self.last_error = None;
        }
    }

    // "Version X is available" with What is new / Download (update.rs).
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    fn update_ui(&mut self, ui: &mut egui::Ui, lang: Lang) {
        if matches!(self.update, Update::Pending) {
            let on = self.row("update_check").value == Value::Bool(true);
            self.update = if on {
                let ctx = ui.ctx().clone();
                Update::Running(crate::update::Background::start(
                    self.dirs.config.clone(),
                    move || ctx.request_repaint(),
                ))
            } else {
                Update::Off
            };
        }
        let Update::Running(bg) = &self.update else {
            return;
        };
        let Some(r) = bg.get() else {
            return;
        };
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new(i18n::ui_fmt(
                        lang,
                        "update_available",
                        &[("version", &r.version)],
                    ))
                    .strong(),
                );
                for (key, url) in [("update_notes", &r.page), ("update_download", &r.download)] {
                    if ui.button(i18n::ui(lang, key)).clicked()
                        && let Err(e) = open::that_detached(url)
                    {
                        log::warn!("{url}: {e}");
                    }
                }
            });
        });
        ui.add_space(6.0);
    }

    // Active Cheats rows as edited on screen (keys).
    fn active_cheats(&self) -> Vec<&'static str> {
        self.rows
            .iter()
            .filter(|r| r.e.group == Group::Cheats && r.value != r.default_value())
            .map(|r| r.e.key)
            .collect()
    }

    fn reset(&mut self) {
        for r in self.rows.iter_mut().filter(|r| r.e.key != "game_dir") {
            r.set(r.default_value());
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let lang = self.lang();
        match &self.view {
            View::Settings => {
                let before = self.row("fullscreen").value.clone();
                let act = self.settings_ui(ui);
                let after = &self.row("fullscreen").value;
                if act.is_none() && *after != before {
                    return Some(Action::Fullscreen(*after == Value::Bool(true)));
                }
                act
            }
            View::Waiting => {
                let wait = i18n::ui(
                    lang,
                    if cfg!(target_os = "android") {
                        "waiting_touch"
                    } else {
                        "waiting_shift"
                    },
                );
                match cheats_note(lang, &self.active_cheats()) {
                    Some(note) => message_ui(ui, &format!("{wait}\n\n{note}")),
                    None => message_ui(ui, wait),
                }
                None
            }
            View::Loading => {
                message_ui(ui, i18n::ui(lang, "loading"));
                None
            }
            View::Error(msg) => error_ui(ui, lang, msg),
            View::PortMenu => port_menu_ui(ui, lang),
        }
    }

    fn settings_ui(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let mut act = None;
        let lang = self.lang();
        egui::Panel::top("folder").show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.heading(i18n::ui(lang, "title"));
                ui.label(egui::RichText::new(version_line()).small().weak());
            });
            self.folder_ui(ui);
            ui.add_space(4.0);
        });
        egui::Panel::bottom("buttons").show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let play = ui.add_enabled(
                    self.can_play(),
                    egui::Button::new(
                        egui::RichText::new(format!("  {}  ", i18n::ui(lang, "play"))).strong(),
                    ),
                );
                if play.clicked() {
                    act = Some(Action::Play);
                }
                if ui.button(i18n::ui(lang, "reset")).clicked() {
                    self.reset();
                }
                if cfg!(not(target_arch = "wasm32")) && ui.button(i18n::ui(lang, "quit")).clicked()
                {
                    act = Some(Action::Quit);
                }
                ui.checkbox(&mut self.dev, i18n::ui(lang, "developer"))
                    .on_hover_text(i18n::ui(lang, "developer_hint"));
                if !self.folder_ok() {
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        i18n::ui(lang, "select_folder_first"),
                    );
                } else if self.rows.iter().any(|r| r.error().is_some()) {
                    ui.colored_label(ui.visuals().error_fg_color, i18n::ui(lang, "fix_values"));
                }
            });
            ui.add_space(4.0);
        });
        egui::CentralPanel::default_margins().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
                self.update_ui(ui, lang);
                self.last_error_ui(ui, lang);
                if !self.dev {
                    let active: Vec<&str> = self
                        .rows
                        .iter()
                        .filter(|r| r.e.audience == Audience::Dev && r.value != r.default_value())
                        .map(|r| i18n::label(lang, r.e))
                        .collect();
                    if !active.is_empty() {
                        ui.colored_label(
                            ui.visuals().error_fg_color,
                            i18n::ui_fmt(lang, "dev_active", &[("list", &active.join(", "))]),
                        );
                    }
                }
                if let Some(note) = cheats_note(lang, &self.active_cheats()) {
                    ui.colored_label(ui.visuals().error_fg_color, note);
                }
                for g in Group::ALL {
                    self.group_ui(ui, g, lang);
                    #[cfg(not(target_os = "android"))]
                    if g == Group::GameData {
                        self.saves_ui(ui, lang);
                    }
                }
            });
        });
        act
    }

    fn folder_ui(&mut self, ui: &mut egui::Ui) {
        let ok = self.folder_ok();
        let lang = self.lang();
        ui.horizontal(|ui| {
            ui.label(i18n::ui(lang, "game_folder"));
            match &self.folder {
                Folder::Native => {
                    let r = self.row_mut("game_dir");
                    #[cfg_attr(target_os = "android", allow(unused_mut))]
                    let mut v = r.value.text();
                    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
                    if ui.button(i18n::ui(lang, "browse")).clicked()
                        && let Some(d) = folder_dialog()
                    {
                        v = d.to_string_lossy().into_owned();
                    }
                    #[cfg(not(target_os = "android"))]
                    ui.add(
                        egui::TextEdit::singleline(&mut v)
                            .hint_text(i18n::ui(lang, "folder_hint"))
                            .desired_width(f32::INFINITY),
                    );
                    // Android: no soft keyboard under NativeActivity, so the fixed folder set by `android_main` is shown, not edited.
                    #[cfg(target_os = "android")]
                    ui.label(v.as_str());
                    if v != r.value.text() {
                        r.set(Value::Path(v));
                    }
                }
                Folder::Bundled(_) => {
                    ui.label(i18n::ui(lang, "bundled_apk"));
                }
                Folder::Web(names) => {
                    ui.label(if names.is_empty() {
                        i18n::ui(lang, "no_files").to_string()
                    } else {
                        names.join(", ")
                    });
                }
            }
        });
        ui.horizontal(|ui| {
            if ok {
                ui.colored_label(
                    egui::Color32::from_rgb(110, 200, 110),
                    i18n::ui(lang, "dat_found"),
                );
            } else {
                ui.colored_label(ui.visuals().error_fg_color, i18n::ui(lang, "dat_missing"));
            }
            if self.dev && matches!(self.folder, Folder::Native) {
                origin_label(ui, self.row("game_dir"), lang);
            }
        });
        if self.bundled {
            ui.label(
                egui::RichText::new(i18n::ui(lang, "bundled_note"))
                    .small()
                    .weak(),
            );
        }
        #[cfg(target_os = "android")]
        if !ok {
            let dir = self.game_dir();
            ui.label(i18n::ui_fmt(lang, "android_copy", &[("dir", &dir)]));
            // Re-check the folder every second so an `adb push` shows up without a touch (the folder check runs
            // on every redraw; a `Resumed` redraws too).
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs(1));
        }
    }

    fn group_ui(&mut self, ui: &mut egui::Ui, g: Group, lang: Lang) {
        self.refresh_installed();
        let extra = self.pack_choices(lang);
        // The game folder row is at the top.
        let dev = self.dev;
        let rows: Vec<usize> = (0..self.rows.len())
            .filter(|&i| {
                let e = self.rows[i].e;
                e.group == g
                    && e.key != "game_dir"
                    && e.shown_on_this_platform()
                    && (dev || e.audience == Audience::Player)
            })
            .collect();
        if rows.is_empty() {
            return;
        }
        let set_here = rows.iter().any(|&i| self.rows[i].origin > Origin::PortCfg);
        // Ids from the group id, so a language switch keeps the open/closed state.
        egui::CollapsingHeader::new(egui::RichText::new(i18n::group_title(lang, g)).strong())
            .id_salt(g.id())
            .default_open(
                matches!(g, Group::GameData | Group::Language | Group::Display) || set_here,
            )
            .show(ui, |ui| {
                egui::Grid::new(g.id())
                    .num_columns(2)
                    .striped(true)
                    .spacing([12.0, 4.0])
                    .show(ui, |ui| {
                        for i in rows {
                            let x: &[(String, String)] = if self.rows[i].e.key == "game_text" {
                                &extra
                            } else {
                                &[]
                            };
                            row_ui(ui, &mut self.rows[i], dev, lang, x);
                            ui.end_row();
                        }
                    });
                if g == Group::Language {
                    ui.add_space(6.0);
                    self.translations_ui(ui, lang);
                }
            });
    }

    // Export / Import of score.dat, th08.cfg, replays and the player rows of port.cfg (saves.rs); shown on the settings
    // screen only, so never while the game runs.
    #[cfg(not(target_os = "android"))]
    fn saves_ui(&mut self, ui: &mut egui::Ui, lang: Lang) {
        #[cfg(target_arch = "wasm32")]
        if let Some(r) = self.saves.picker.take() {
            self.saves.status = Some(r.and_then(|d| self.import_saves(&d, lang)));
        }
        #[cfg(target_arch = "wasm32")]
        if self.saves.picker.waiting() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(250));
        }
        egui::CollapsingHeader::new(egui::RichText::new(i18n::ui(lang, "saves_title")).strong())
            .id_salt("saves")
            .show(ui, |ui| {
                ui.label(egui::RichText::new(i18n::ui(lang, "saves_hint")).weak());
                ui.horizontal_wrapped(|ui| {
                    if ui.button(i18n::ui(lang, "saves_export")).clicked() {
                        self.saves.status = self.export_saves(lang);
                    }
                    if ui.button(i18n::ui(lang, "saves_import")).clicked() {
                        self.saves.status = self.pick_import(lang);
                    }
                    ui.checkbox(&mut self.saves.port_cfg, i18n::ui(lang, "saves_port_cfg"))
                        .on_hover_text(i18n::ui(lang, "saves_port_cfg_hint"));
                });
                match &self.saves.status {
                    Some(Ok(m)) => {
                        ui.label(m);
                    }
                    Some(Err(e)) => {
                        ui.colored_label(ui.visuals().error_fg_color, e);
                    }
                    None => {}
                }
            });
    }

    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    fn save_store(&self) -> Result<crate::saves::DirStore, String> {
        let save = self.settings().storage_dir().ok_or("no save folder")?;
        Ok(crate::saves::DirStore {
            save,
            dirs: self.dirs.clone(),
        })
    }

    #[cfg(target_arch = "wasm32")]
    fn save_store(&self) -> Result<crate::saves::WebStore, String> {
        Ok(crate::saves::WebStore {
            storage: self.settings().storage(),
            dirs: self.dirs.clone(),
        })
    }

    // None = the file dialog was cancelled.
    #[cfg(not(target_os = "android"))]
    fn export_saves(&self, lang: Lang) -> Option<Result<String, String>> {
        let fail = |e: String| i18n::ui_fmt(lang, "saves_export_failed", &[("err", &e)]);
        let t = crate::platform::Clock::default().local_time();
        let out = match self
            .save_store()
            .and_then(|st| crate::saves::export(&st, &t))
        {
            Ok(o) => o,
            Err(e) => return Some(Err(fail(e))),
        };
        let name = crate::saves::export_name(&t);
        #[cfg(not(target_arch = "wasm32"))]
        let r = {
            let path = rfd::FileDialog::new()
                .set_title(i18n::ui(lang, "saves_export"))
                .set_file_name(&name)
                .add_filter("zip", &["zip"])
                .save_file()?;
            crate::platform::write_atomic(&path, &out.zip)
                .map(|()| path.display().to_string())
                .map_err(|e| format!("{}: {e}", path.display()))
        };
        #[cfg(target_arch = "wasm32")]
        let r = crate::saves::download(&name, &out.zip).map(|()| name);
        let n = out.files.to_string();
        Some(
            r.map(|p| i18n::ui_fmt(lang, "saves_exported", &[("n", &n), ("path", &p)]))
                .map_err(fail),
        )
    }

    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    fn pick_import(&mut self, lang: Lang) -> Option<Result<String, String>> {
        let path = rfd::FileDialog::new()
            .set_title(i18n::ui(lang, "saves_import"))
            .add_filter("zip", &["zip"])
            .pick_file()?;
        let fail = |e: String| i18n::ui_fmt(lang, "saves_import_failed", &[("err", &e)]);
        let data = std::fs::metadata(&path)
            .map_err(|e| e.to_string())
            .and_then(|m| {
                if m.len() > crate::saves::MAX_ZIP {
                    Err(format!("larger than {} MiB", crate::saves::MAX_ZIP >> 20))
                } else {
                    std::fs::read(&path).map_err(|e| e.to_string())
                }
            });
        Some(data.map_err(fail).and_then(|d| self.import_saves(&d, lang)))
    }

    // The zip arrives later through the picker (polled in `saves_ui`).
    #[cfg(target_arch = "wasm32")]
    fn pick_import(&mut self, lang: Lang) -> Option<Result<String, String>> {
        self.saves
            .picker
            .open()
            .err()
            .map(|e| Err(i18n::ui_fmt(lang, "saves_import_failed", &[("err", &e)])))
    }

    #[cfg(not(target_os = "android"))]
    fn import_saves(&mut self, data: &[u8], lang: Lang) -> Result<String, String> {
        let t = crate::platform::Clock::default().local_time();
        let r = self
            .save_store()
            .and_then(|mut st| crate::saves::import(&mut st, data, self.saves.port_cfg, &t))
            .map_err(|e| i18n::ui_fmt(lang, "saves_import_failed", &[("err", &e)]))?;
        if r.port_cfg {
            self.reload_port_cfg();
        }
        let n = r.files.to_string();
        Ok(match &r.backup {
            Some(b) => i18n::ui_fmt(lang, "saves_imported_backup", &[("n", &n), ("backup", b)]),
            None => i18n::ui_fmt(lang, "saves_imported", &[("n", &n)]),
        })
    }

    // Rows not edited on screen and not set above port.cfg take the values of a port.cfg just imported.
    #[cfg(not(target_os = "android"))]
    fn reload_port_cfg(&mut self) {
        let s = Settings::game_environment(self.dirs.clone());
        let bundled = self.bundled;
        for r in self.rows.iter_mut().filter(|r| {
            !r.dirty
                && r.e.persist
                && r.origin <= Origin::PortCfg
                && !(bundled && r.e.key == "game_dir")
        }) {
            let o = s.origin(r.e.key);
            if o <= Origin::PortCfg {
                *r = Row::new(r.e, s.value(r.e.key).unwrap_or(r.e.default), o);
            }
        }
    }

    // Installed packs, the server's languages and the running download (port-only UI).
    fn translations_ui(&mut self, ui: &mut egui::Ui, lang: Lang) {
        #[cfg(not(target_arch = "wasm32"))]
        let data = match &self.folder {
            Folder::Native if self.folder_ok() => Some(PathBuf::from(self.game_dir())),
            _ => None,
        };
        // The browser reads th08.dat / th08.pak from the files picked in the page.
        #[cfg(target_arch = "wasm32")]
        let data = matches!(&self.folder, Folder::Web(_) if self.folder_ok()).then_some(());
        let Some(t) = &self.tr.manager else {
            return;
        };
        let st = t.state();
        if st.busy.is_some() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(150));
        }
        ui.label(egui::RichText::new(i18n::ui(lang, "tr_title")).strong());
        let can_install = st.busy.is_none() && data.is_some();
        let mut act = installed_ui(ui, lang, &self.tr.installed, &st, can_install);
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(i18n::ui(lang, "tr_server"));
            if st.busy.as_deref() == Some("") {
                ui.spinner();
                ui.label(i18n::ui(lang, "tr_loading"));
            } else if st.server.is_none()
                && ui
                    .add_enabled(
                        st.busy.is_none(),
                        egui::Button::new(i18n::ui(lang, "tr_load")),
                    )
                    .clicked()
            {
                t.load_list();
            }
            if st.server.is_some() {
                ui.add(
                    egui::TextEdit::singleline(&mut self.tr.filter)
                        .hint_text(i18n::ui(lang, "tr_filter"))
                        .desired_width(140.0),
                );
            }
        });
        if let Some(list) = &st.server {
            act = act.or(server_ui(ui, lang, list, &self.tr, can_install));
        }
        if data.is_none() {
            ui.colored_label(ui.visuals().warn_fg_color, i18n::ui(lang, "tr_needs_data"));
        }
        job_ui(ui, lang, &st);
        #[cfg(not(target_arch = "wasm32"))]
        let dir = t.packs_dir().display().to_string();
        #[cfg(target_arch = "wasm32")]
        let dir = i18n::ui(lang, "tr_site_storage").to_string();
        ui.label(
            egui::RichText::new(i18n::ui_fmt(lang, "tr_attribution", &[("dir", &dir)]))
                .small()
                .weak(),
        );
        match act {
            Some((true, code)) => {
                #[cfg(not(target_arch = "wasm32"))]
                if let Some(d) = data {
                    t.install(&code, crate::translations::Data::Dir(d));
                }
                #[cfg(target_arch = "wasm32")]
                if data.is_some() {
                    t.install(&code);
                }
            }
            Some((false, code)) => {
                t.remove(&code);
            }
            None => {}
        }
    }
}

// Installed packs with Update / Remove for the downloaded ones; returns (install?, code) of a click.
fn installed_ui(
    ui: &mut egui::Ui,
    lang: Lang,
    installed: &[crate::packs::Installed],
    st: &crate::translations::State,
    can_install: bool,
) -> Option<(bool, String)> {
    let mut act = None;
    let busy = st.busy.is_some();
    ui.label(i18n::ui(lang, "tr_installed"));
    if installed.is_empty() {
        ui.label(egui::RichText::new(i18n::ui(lang, "tr_none")).weak());
    }
    egui::Grid::new("tr_installed")
        .num_columns(3)
        .spacing([12.0, 4.0])
        .show(ui, |ui| {
            for p in installed {
                ui.label(egui::RichText::new(&p.language).monospace());
                let mut s = p.title.clone();
                if let Some(c) = p.coverage {
                    let pct = format!("{c:.1}");
                    s = format!(
                        "{s}  ({})",
                        i18n::ui_fmt(lang, "tr_translated", &[("pct", &pct)])
                    );
                }
                ui.label(s);
                ui.horizontal(|ui| {
                    if !p.managed {
                        ui.label(
                            egui::RichText::new(p.path.display().to_string())
                                .small()
                                .weak(),
                        );
                        return;
                    }
                    let server = st
                        .server
                        .as_ref()
                        .and_then(|l| l.iter().find(|x| x.code() == p.language));
                    if server.is_some_and(|x| x.base.is_empty() && x.revision == p.revision) {
                        ui.label(egui::RichText::new(i18n::ui(lang, "tr_up_to_date")).weak());
                    } else if ui
                        .add_enabled(can_install, egui::Button::new(i18n::ui(lang, "tr_update")))
                        .clicked()
                    {
                        act = Some((true, p.language.clone()));
                    }
                    if ui
                        .add_enabled(!busy, egui::Button::new(i18n::ui(lang, "tr_remove")))
                        .clicked()
                    {
                        act = Some((false, p.language.clone()));
                    }
                });
                ui.end_row();
            }
        });
    act
}

// The server's languages (filtered) with Download / Update.
fn server_ui(
    ui: &mut egui::Ui,
    lang: Lang,
    list: &[th08_thcrap::manage::ServerLang],
    tr: &Tr,
    can_install: bool,
) -> Option<(bool, String)> {
    let mut act = None;
    let f = tr.filter.to_lowercase();
    egui::ScrollArea::vertical()
        .id_salt("tr_server")
        .max_height(220.0)
        .show(ui, |ui| {
            egui::Grid::new("tr_server_grid")
                .num_columns(4)
                .striped(true)
                .spacing([12.0, 3.0])
                .show(ui, |ui| {
                    let mut shown: Vec<&th08_thcrap::manage::ServerLang> = list
                        .iter()
                        .filter(|l| {
                            f.is_empty() || l.id.contains(&f) || l.title.to_lowercase().contains(&f)
                        })
                        .collect();
                    // Exact and prefix code matches first.
                    shown.sort_by_key(|l| {
                        u8::from(l.code() != f) + u8::from(!l.code().starts_with(f.as_str()))
                    });
                    for l in shown {
                        let have = tr
                            .installed
                            .iter()
                            .any(|p| p.managed && p.language == l.code());
                        let label = if have { "tr_update" } else { "tr_install" };
                        if ui
                            .add_enabled(can_install, egui::Button::new(i18n::ui(lang, label)))
                            .clicked()
                        {
                            act = Some((true, l.code().to_string()));
                        }
                        ui.label(egui::RichText::new(l.code()).monospace());
                        ui.label(&l.title);
                        let pct = format!("{:.0}", l.estimate());
                        let mut info = i18n::ui_fmt(lang, "tr_estimate", &[("pct", &pct)]);
                        if !l.base.is_empty() {
                            let base: Vec<&str> = l
                                .base
                                .iter()
                                .map(|b| b.strip_prefix("lang_").unwrap_or(b))
                                .collect();
                            info = format!(
                                "{info}; {}",
                                i18n::ui_fmt(lang, "tr_on_top", &[("base", &base.join(", "))])
                            );
                        }
                        ui.label(egui::RichText::new(info).small());
                        ui.end_row();
                    }
                });
        });
    act
}

// The running job's progress, else the last outcome.
fn job_ui(ui: &mut egui::Ui, lang: Lang, st: &crate::translations::State) {
    use crate::translations::Outcome;
    if let Some(code) = st.busy.as_deref().filter(|c| !c.is_empty()) {
        let (done, total) = st.files;
        let text = if st.building {
            i18n::ui_fmt(lang, "tr_building", &[("lang", code)])
        } else {
            i18n::ui_fmt(
                lang,
                "tr_working",
                &[
                    ("lang", code),
                    ("done", &done.to_string()),
                    ("total", &total.to_string()),
                ],
            )
        };
        let frac = if total == 0 {
            0.0
        } else {
            done as f32 / total as f32
        };
        ui.add(egui::ProgressBar::new(frac).text(text).animate(st.building));
        ui.label(egui::RichText::new(&st.last_line).small().weak());
        return;
    }
    match &st.outcome {
        Some(Outcome::Installed { lang: l, percent }) => {
            let pct = format!("{percent:.1}");
            ui.colored_label(
                egui::Color32::from_rgb(110, 200, 110),
                i18n::ui_fmt(lang, "tr_done", &[("lang", l), ("pct", &pct)]),
            );
        }
        Some(Outcome::Removed { lang: l }) => {
            ui.label(i18n::ui_fmt(lang, "tr_removed", &[("lang", l)]));
        }
        Some(Outcome::Failed { lang: l, error }) => {
            ui.colored_label(
                ui.visuals().error_fg_color,
                i18n::ui_fmt(lang, "tr_failed", &[("lang", l), ("err", error)]),
            );
        }
        None => {}
    }
}

fn message_ui(ui: &mut egui::Ui, text: &str) {
    egui::CentralPanel::default_margins().show(ui, |ui| {
        ui.centered_and_justified(|ui| {
            ui.label(text);
        });
    });
}

fn error_ui(ui: &mut egui::Ui, lang: Lang, msg: &str) -> Option<Action> {
    let mut act = None;
    egui::Panel::bottom("buttons").show(ui, |ui| {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui
                .button(format!("  {}  ", i18n::ui(lang, "back")))
                .clicked()
            {
                act = Some(Action::Back);
            }
            if cfg!(not(target_arch = "wasm32")) && ui.button(i18n::ui(lang, "quit")).clicked() {
                act = Some(Action::Quit);
            }
        });
        ui.add_space(4.0);
    });
    egui::CentralPanel::default_margins().show(ui, |ui| {
        ui.add_space(4.0);
        ui.heading(i18n::ui(lang, "start_failed"));
        ui.add_space(8.0);
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.colored_label(ui.visuals().error_fg_color, msg);
        });
    });
    act
}

// Port menu stub: the hook for the touch-layout editor; host settings only will go here, never core ones (seed,
// stage, cheats), so a mid-game visit cannot change the game's logic.
fn port_menu_ui(ui: &mut egui::Ui, lang: Lang) -> Option<Action> {
    let mut act = None;
    egui::CentralPanel::default_margins().show(ui, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() / 3.0);
            ui.heading(i18n::ui(lang, "port_menu"));
            ui.add_space(4.0);
            ui.label(egui::RichText::new(i18n::ui(lang, "paused")).weak());
            ui.add_space(12.0);
            if ui
                .button(egui::RichText::new(format!("  {}  ", i18n::ui(lang, "resume"))).strong())
                .clicked()
            {
                act = Some(Action::Resume);
            }
            ui.add_space(4.0);
            ui.label(egui::RichText::new("F1 / Esc").small().weak());
            ui.label(egui::RichText::new(version_line()).small().weak());
        });
    });
    act
}

// Help is shown once, under the value; the developer view adds the flag / env name and the origin tag.
fn row_ui(ui: &mut egui::Ui, r: &mut Row, dev: bool, lang: Lang, extra: &[(String, String)]) {
    let l = ui.label(i18n::label(lang, r.e));
    if dev {
        l.on_hover_text(format!("--{} / {}", r.e.key.replace('_', "-"), r.e.env));
    }
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            value_ui(ui, r, lang, extra);
            if dev {
                origin_label(ui, r, lang);
            }
        });
        if let Some(err) = r.error() {
            ui.colored_label(ui.visuals().error_fg_color, err);
        }
        ui.label(egui::RichText::new(i18n::help(lang, r.e)).small().weak());
    });
}

// Edits a copy of the value; the row is set (and its dirty flag updated) only when a widget changed it.
// `extra` = choices after the table's (value, shown name): the installed pack languages for `game_text`.
fn value_ui(ui: &mut egui::Ui, r: &mut Row, lang: Lang, extra: &[(String, String)]) {
    let unsettable = r.e.default.is_empty();
    let mut v = r.value.clone();
    match (&mut v, r.e.kind) {
        (Value::Bool(on), _) => {
            ui.checkbox(on, "");
        }
        (Value::Int(n), Kind::Int { min, max }) => {
            let mut set = n.is_some();
            if unsettable && ui.checkbox(&mut set, "").changed() {
                *n = set.then(|| min.max(0).min(max));
            }
            if let Some(n) = n {
                let mut f = *n as f64;
                let range = min as f64..=max as f64;
                let changed = if max - min <= 16 {
                    ui.add(
                        egui::Slider::new(&mut f, range)
                            .step_by(1.0)
                            .fixed_decimals(0),
                    )
                    .changed()
                } else {
                    ui.add(
                        egui::DragValue::new(&mut f)
                            .range(range)
                            .speed(1.0)
                            .fixed_decimals(0),
                    )
                    .changed()
                };
                if changed {
                    *n = f.round() as i64;
                }
            }
        }
        (Value::Choice(c), Kind::Enum(choices)) => {
            // Choices show their display words; a value without one shows as is.
            let e = r.e;
            let shown = |v: &str| -> String {
                match i18n::word(lang, e, v) {
                    Some(w) => w.to_string(),
                    None if v.is_empty() => i18n::ui(lang, "not_set").to_string(),
                    None => v.to_string(),
                }
            };
            let selected = extra
                .iter()
                .find(|(x, _)| x == c)
                .map_or_else(|| shown(c), |(_, n)| n.clone());
            egui::ComboBox::from_id_salt(r.e.key)
                .selected_text(selected)
                .show_ui(ui, |ui| {
                    if unsettable {
                        ui.selectable_value(c, String::new(), shown(""));
                    }
                    for x in choices {
                        ui.selectable_value(c, x.to_string(), shown(x));
                    }
                    for (x, name) in extra {
                        ui.selectable_value(c, x.clone(), name);
                    }
                });
        }
        // Android: text rows are read-only (no soft keyboard); they are set through port.cfg.
        #[cfg(target_os = "android")]
        (Value::Text(s) | Value::Path(s), _) => {
            ui.label(if s.is_empty() {
                i18n::ui(lang, "not_set")
            } else {
                s.as_str()
            });
        }
        #[cfg(not(target_os = "android"))]
        (Value::Text(s), kind) => {
            let hint = if let Kind::Text(h) = kind { h } else { "" };
            ui.add(
                egui::TextEdit::singleline(s)
                    .hint_text(hint)
                    .desired_width(220.0),
            );
        }
        #[cfg(not(target_os = "android"))]
        (Value::Path(s), _) => {
            ui.add(
                egui::TextEdit::singleline(s)
                    .hint_text(r.e.hint())
                    .desired_width(220.0),
            );
            #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
            if ui.button(i18n::ui(lang, "browse")).clicked() {
                let d = rfd::FileDialog::new().set_title(i18n::help(lang, r.e));
                let picked = if r.e.key == "soundfont" {
                    d.add_filter("SoundFont", &["sf2"]).pick_file()
                } else if r.e.kind == Kind::File {
                    d.add_filter("fonts", &["ttf", "otf", "ttc"]).pick_file()
                } else {
                    d.pick_folder()
                };
                if let Some(p) = picked {
                    *s = p.to_string_lossy().into_owned();
                }
            }
        }
        _ => unreachable!("Value::parse follows the entry kind"),
    }
    if v != r.value {
        r.set(v);
    }
}

// "set by TH08_STAGE" / "set by --stage" for values from the environment or command line, "changed" after an edit.
fn origin_label(ui: &mut egui::Ui, r: &Row, lang: Lang) {
    let set_by = |source: &str| i18n::ui_fmt(lang, "set_by", &[("source", source)]);
    let text = if r.dirty {
        i18n::ui(lang, "changed").to_string()
    } else {
        match r.origin {
            Origin::Cli => set_by(&format!("--{}", r.e.key.replace('_', "-"))),
            Origin::Launcher => i18n::ui(lang, "changed").into(),
            Origin::Env if cfg!(target_arch = "wasm32") => set_by(&format!("URL ?{}=", r.e.url())),
            Origin::Env => set_by(r.e.env),
            Origin::PortCfg if cfg!(target_arch = "wasm32") => i18n::ui(lang, "saved").into(),
            Origin::PortCfg => "port.cfg".into(),
            Origin::Bundled => i18n::ui(lang, "bundled").into(),
            Origin::Default => return,
        }
    };
    let c = match (r.dirty, r.origin) {
        (true, _) | (_, Origin::Launcher) => egui::Color32::from_rgb(120, 170, 255),
        (_, Origin::Cli | Origin::Env) => egui::Color32::from_rgb(240, 190, 80),
        _ => ui.visuals().weak_text_color(),
    };
    ui.colored_label(c, text);
}

// Size (px) of the safe rect of a w×h window.
#[expect(
    clippy::cast_sign_loss,
    reason = "the safe rect is inside the window, so its size is non-negative"
)]
fn safe_size(safe: SafeArea, (w, h): (u32, u32)) -> (u32, u32) {
    let [_, _, sw, sh] = safe.rect(w, h);
    (sw as u32, sh as u32)
}

// The safe area in egui points at `ppp` px per point.
fn insets(safe: SafeArea, ppp: f32) -> egui::SafeAreaInsets {
    egui::SafeAreaInsets(egui::epaint::MarginF32 {
        left: safe.left as f32 / ppp,
        right: safe.right as f32 / ppp,
        top: safe.top as f32 / ppp,
        bottom: safe.bottom as f32 / ppp,
    })
}

// The 640×480 layout scaled to the window's safe rect.
fn zoom(size_px: (u32, u32)) -> f32 {
    (size_px.0 as f32 / 640.0)
        .min(size_px.1 as f32 / 480.0)
        .max(0.5)
}

// egui context and renderer on the game's device, drawing the model.
struct Painter {
    model: Model,
    ctx: egui::Context,
    renderer: egui_wgpu::Renderer,
    // `Gfx::generation` the renderer was made on.
    generation: u32,
    // Texture updates not yet uploaded (a pass whose frame was skipped keeps them for the next paint).
    pending: egui::TexturesDelta,
}

// The launcher in the game window. Dropped before the game loads, freeing its GPU objects.
pub struct Screen {
    p: Painter,
    state: egui_winit::State,
    // When egui wants the next repaint (tooltips, animations).
    pub repaint_at: Option<web_time::Instant>,
}

impl Screen {
    pub fn new(window: &Window, gfx: &Gfx, model: Model) -> Self {
        let p = Painter::new(gfx, model);
        let state = egui_winit::State::new(
            p.ctx.clone(),
            egui::ViewportId::ROOT,
            window,
            Some(window.scale_factor() as f32),
            None,
            None,
        );
        Self {
            p,
            state,
            repaint_at: None,
        }
    }

    pub fn model(&self) -> &Model {
        &self.p.model
    }

    pub fn model_mut(&mut self) -> &mut Model {
        &mut self.p.model
    }

    // Returns whether the window needs a redraw.
    pub fn on_event(&mut self, window: &Window, ev: &WindowEvent) -> bool {
        self.state.on_window_event(window, ev).repaint
    }

    // One egui pass drawn to the window surface.
    pub fn redraw(&mut self, window: &Window, gfx: &mut Gfx) -> Option<Action> {
        let size = window.inner_size();
        gfx.resize(size.width, size.height);
        let (w, h) = gfx.surface_size();
        let ppp = zoom(safe_size(gfx.safe, (w, h)));
        let z = ppp / window.scale_factor() as f32;
        // egui's zoom-change pass lays out one frame with the old insets: draw once more after it.
        if self.p.ctx.zoom_factor() != z {
            window.request_redraw();
        }
        self.p.ctx.set_zoom_factor(z);
        let mut raw = self.state.take_egui_input(window);
        raw.safe_area_insets = Some(insets(gfx.safe, ppp));
        let (out, act) = self.p.pass(raw);
        self.state
            .handle_platform_output(window, out.platform_output.clone());
        let delay = out
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .map_or(std::time::Duration::MAX, |v| v.repaint_delay);
        self.repaint_at =
            (delay < std::time::Duration::from_secs(5)).then(|| web_time::Instant::now() + delay);
        gfx.recover_if_lost();
        if self.p.generation != gfx.generation() {
            self.p.renew(gfx);
        }
        let Some(surface) = &gfx.surface else {
            return act;
        };
        let (wgpu::CurrentSurfaceTexture::Success(frame)
        | wgpu::CurrentSurfaceTexture::Suboptimal(frame)) = surface.get_current_texture()
        else {
            gfx.reconfigure();
            window.request_redraw();
            return act;
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let enc = self.p.paint(gfx, out, &view, (w, h));
        gfx.queue.submit([enc.finish()]);
        gfx.queue.present(frame);
        gfx.presented();
        act
    }
}

fn atlas_image(f: &egui::epaint::text::FontsView<'_>) -> egui::ColorImage {
    f.image()
}

// After a device rebuild the egui renderer is dead and the new one has no font atlas: queue the whole current
// atlas in place of its owed partial updates (other texture ids are dropped with the old renderer).
pub fn resend_atlas(ctx: &egui::Context, pending: &mut egui::TexturesDelta) {
    let font = egui::TextureId::default();
    let options = ctx
        .tex_manager()
        .read()
        .meta(font)
        .map(|m| m.options)
        .unwrap_or_default();
    let image = ctx.fonts(atlas_image);
    pending.free.clear();
    pending.set.insert(
        font,
        [egui::epaint::ImageDelta::full(image, options)].into(),
    );
}

impl Painter {
    fn new(gfx: &Gfx, model: Model) -> Self {
        let ctx = egui::Context::default();
        ctx.set_visuals(egui::Visuals::dark());
        let font = model.row("font").value.text();
        i18n::install_fonts(&ctx, Some(font.as_str()).filter(|f| !f.is_empty()));
        let renderer = egui_wgpu::Renderer::new(
            &gfx.device,
            gfx.surface_format(),
            egui_wgpu::RendererOptions::default(),
        );
        Self {
            model,
            ctx,
            renderer,
            generation: gfx.generation(),
            pending: egui::TexturesDelta::default(),
        }
    }

    // A new renderer on the rebuilt device (`Gfx::rebuild`), with the font atlas sent again.
    fn renew(&mut self, gfx: &Gfx) {
        self.renderer = egui_wgpu::Renderer::new(
            &gfx.device,
            gfx.surface_format(),
            egui_wgpu::RendererOptions::default(),
        );
        self.generation = gfx.generation();
        resend_atlas(&self.ctx, &mut self.pending);
    }

    fn pass(&mut self, raw: egui::RawInput) -> (egui::FullOutput, Option<Action>) {
        let mut act = None;
        let model = &mut self.model;
        // Every view is laid out inside the safe area (egui's content rect); the clear colour fills the rest.
        let mut out = self.ctx.run_ui(raw, |ui| {
            let rect = ui.ctx().content_rect();
            ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                if let Some(a) = model.ui(ui) {
                    act = Some(a);
                }
            });
        });
        self.pending.append(std::mem::take(&mut out.textures_delta));
        (out, act)
    }

    fn paint(
        &mut self,
        gfx: &Gfx,
        out: egui::FullOutput,
        view: &wgpu::TextureView,
        (w, h): (u32, u32),
    ) -> wgpu::CommandEncoder {
        let jobs = self.ctx.tessellate(out.shapes, out.pixels_per_point);
        let desc = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [w, h],
            pixels_per_point: out.pixels_per_point,
        };
        for (id, deltas) in self.pending.set.drain() {
            for d in &deltas {
                self.renderer.update_texture(&gfx.device, &gfx.queue, id, d);
            }
        }
        let mut enc = gfx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        let extra = self
            .renderer
            .update_buffers(&gfx.device, &gfx.queue, &mut enc, &jobs, &desc);
        if !extra.is_empty() {
            gfx.queue.submit(extra);
        }
        {
            let mut pass = enc
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("launcher"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                })
                .forget_lifetime();
            self.renderer.render(&mut pass, &jobs, &desc);
        }
        for id in self.pending.free.drain() {
            self.renderer.free_texture(&id);
        }
        enc
    }
}

// Offscreen render of the launcher (tests, `launchershot`): a few passes so the layout settles, then read back.
#[cfg(not(target_arch = "wasm32"))]
pub fn render_image(
    gfx: &Gfx,
    model: Model,
    (w, h): (u32, u32),
    safe: SafeArea,
) -> image::RgbaImage {
    let mut s = Painter::new(gfx, model);
    let tex = gfx.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("launcher shot"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: gfx.surface_format(),
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    let ppp = zoom(safe_size(safe, (w, h)));
    let mut enc = None;
    for i in 0..4 {
        let mut raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(w as f32 / ppp, h as f32 / ppp),
            )),
            time: Some(f64::from(i) / 60.0),
            safe_area_insets: Some(insets(safe, ppp)),
            ..Default::default()
        };
        raw.viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .native_pixels_per_point = Some(ppp);
        let (out, _) = s.pass(raw);
        let e = s.paint(gfx, out, &view, (w, h));
        if i < 3 {
            gfx.queue.submit([e.finish()]);
        } else {
            enc = Some(e);
        }
    }
    gfx.read_texture(enc.unwrap(), &tex)
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    fn game_dir(root: &Path, name: &str) -> PathBuf {
        let d = root.join(name);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("TH08.DAT"), b"").unwrap();
        d
    }

    // A folder with only the packs is a game folder, as is one with th08.dat.
    #[test]
    fn pack_folder_is_game_dir() {
        let root = std::env::temp_dir().join(format!("th08_pack_dir_{}", std::process::id()));
        let d = root.join("packs");
        std::fs::create_dir_all(&d).unwrap();
        assert!(!is_game_dir(&d));
        std::fs::write(d.join("thbgm.pak"), b"").unwrap();
        assert!(!is_game_dir(&d));
        std::fs::write(d.join("TH08.PAK"), b"").unwrap();
        assert!(is_game_dir(&d));
        assert!(th08_core::content::has_content(&["thbgm.pak", "th08.pak"]));
        assert!(!th08_core::content::has_content(&[
            "thbgm.pak",
            "thbgm.dat"
        ]));
        std::fs::remove_dir_all(&root).unwrap();
    }

    fn edit(m: &mut Model, key: &str, v: &str) {
        let r = m.row_mut(key);
        r.set(Value::parse(r.e.kind, v));
    }

    fn some(c: &[(&'static str, &str)]) -> Vec<(&'static str, Option<String>)> {
        c.iter().map(|(k, v)| (*k, Some(v.to_string()))).collect()
    }

    #[test]
    fn developer_rows_not_saved() {
        let mut s = Settings::default();
        s.set_from(Origin::PortCfg, "turbo", "2").unwrap();
        let mut m = Model::new(&s, Folder::Native);
        assert!(!m.dev);
        edit(&mut m, "invuln", "on");
        edit(&mut m, "replay_save", "3");
        edit(&mut m, "fullscreen", "on");
        edit(&mut m, "launcher", "off");
        edit(&mut m, "scale", "1");
        assert_eq!(
            m.changes(),
            some(&[("fullscreen", "on"), ("launcher", "off")])
        );
        let r = m.settings();
        assert!(r.invuln && r.replay_save == Some(3) && r.fullscreen && !r.launcher);
        assert_eq!((r.turbo, r.origin("turbo")), (2, Origin::PortCfg));
        assert_eq!(r.origin("invuln"), Origin::Launcher);
        assert_eq!(r.origin("scale"), Origin::Default);
        edit(&mut m, "fullscreen", "off");
        assert_eq!(m.changes(), some(&[("launcher", "off")]));
        edit(&mut m, "seed", "");
        edit(&mut m, "power", "64");
        edit(&mut m, "power", "");
        assert_eq!(m.changes(), some(&[("launcher", "off")]));
        s.set_from(Origin::Cli, "invuln", "on").unwrap();
        assert!(Model::new(&s, Folder::Native).dev);
    }

    #[test]
    fn typed_values() {
        let k = |key| settings::find(key).unwrap().kind;
        assert_eq!(Value::parse(k("vsync"), "1"), Value::Bool(true));
        assert_eq!(Value::parse(k("scale"), " 3 "), Value::Int(Some(3)));
        assert_eq!(Value::parse(k("seed"), ""), Value::Int(None));
        assert_eq!(
            Value::parse(k("rank"), "hard"),
            Value::Choice("hard".into())
        );
        assert_eq!(
            Value::parse(k("font"), "/f.ttf"),
            Value::Path("/f.ttf".into())
        );
        assert_eq!(Value::parse(k("ending"), "7c"), Value::Text("7c".into()));
        for (key, v) in [
            ("vsync", "off"),
            ("scale", "2"),
            ("seed", ""),
            ("stage", "4a"),
        ] {
            assert_eq!(Value::parse(k(key), v).text(), v);
        }
        let mut s = Settings::default();
        s.set_from(Origin::Cli, "ending", "zz").ok();
        let mut m = Model::new(&s, Folder::Native);
        edit(&mut m, "ending", "zz");
        assert!(m.row("ending").dirty && m.row("ending").error().is_some());
        edit(&mut m, "ending", "");
        assert!(!m.row("ending").dirty && m.row("ending").error().is_none());
    }

    #[test]
    fn bundled_lookup_order() {
        let root = std::env::temp_dir().join(format!("th08-bundled-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let exe = root.join("exe");
        let appdir = root.join("appdir");
        assert_eq!(
            bundled_candidates(Some(&exe), Some(&appdir)),
            [exe.join("game"), appdir.join("usr/share/th08/game")]
        );
        assert_eq!(bundled_candidates(None, None), Vec::<PathBuf>::new());

        let user = game_dir(&root, "user");
        let bundled = game_dir(&root, "bundled");
        let some = || Some(bundled.clone());
        let mut s = Settings::default();
        assert_eq!(choose_game_dir(&s, some), Choice::Bundled(bundled.clone()));
        assert_eq!(choose_game_dir(&s, || None), Choice::Ask);
        s.set_from(Origin::Bundled, "game_dir", &bundled.to_string_lossy())
            .unwrap();
        assert_eq!(choose_game_dir(&s, some), Choice::Bundled(bundled.clone()));
        s.set_from(Origin::PortCfg, "game_dir", &user.to_string_lossy())
            .unwrap();
        assert_eq!(choose_game_dir(&s, some), Choice::Bundled(bundled.clone()));
        assert_eq!(
            choose_game_dir(&s, || None),
            Choice::Configured(user.clone())
        );
        s.set_from(Origin::Launcher, "game_dir", &user.to_string_lossy())
            .unwrap();
        assert_eq!(choose_game_dir(&s, some), Choice::Configured(user.clone()));
        let missing = root.join("missing").to_string_lossy().into_owned();
        s.set_from(Origin::PortCfg, "game_dir", &missing).unwrap();
        assert_eq!(choose_game_dir(&s, some), Choice::Bundled(bundled.clone()));
        s.set_from(Origin::Cli, "game_dir", &missing).unwrap();
        assert!(matches!(choose_game_dir(&s, some), Choice::Invalid(_)));

        let mut s = Settings::default();
        let found = bundled_candidates(Some(&root.join("none")), Some(&root.join("appimage")))
            .into_iter()
            .find(|d| is_game_dir(d));
        assert_eq!(found, None);
        game_dir(&root, "appimage/usr/share/th08/game");
        let ex = game_dir(&root, "exe/game");
        let found = bundled_candidates(Some(&exe), Some(&root.join("appimage")))
            .into_iter()
            .find(|d| is_game_dir(d));
        assert_eq!(found, Some(ex));
        s.set_from(Origin::Env, "game_dir", &user.to_string_lossy())
            .unwrap();
        assert_eq!(choose_game_dir(&s, some), Choice::Configured(user));
        std::fs::remove_dir_all(&root).unwrap();
    }

    // A build with bundled data shows its folder as bundled and runs an edited folder without saving it; a build
    // without bundled data saves the folder as before.
    #[test]
    fn bundled_game_dir_not_saved() {
        let root = std::env::temp_dir().join(format!("th08-bundled-save-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let user = game_dir(&root, "user");
        let bundled = game_dir(&root, "bundled");
        let user_text = user.to_string_lossy().into_owned();

        let mut s = Settings::default();
        s.set_from(
            Origin::PortCfg,
            "game_dir",
            &root.join("old").to_string_lossy(),
        )
        .unwrap();
        prefill_bundled(&mut s, || Some(bundled.clone()));
        assert_eq!(s.origin("game_dir"), Origin::Bundled);
        assert_eq!(s.game_dir.as_deref(), Some(bundled.as_path()));
        let mut m = Model::new(&s, Folder::Native);
        assert!(m.bundled && m.changes().is_empty());
        edit(&mut m, "game_dir", &user_text);
        edit(&mut m, "scale", "2");
        assert_eq!(m.changes(), some(&[("scale", "2")]));
        let run = m.settings();
        assert_eq!(run.origin("game_dir"), Origin::Launcher);
        assert_eq!(
            choose_game_dir(&run, || Some(bundled.clone())),
            Choice::Configured(user.clone())
        );

        let s = Settings::default();
        let mut s2 = s.clone();
        prefill_bundled(&mut s2, || None);
        assert_eq!(s2.origin("game_dir"), Origin::Default);
        let mut m = Model::new(&s, Folder::Native);
        assert!(!m.bundled);
        edit(&mut m, "game_dir", &user_text);
        assert_eq!(m.changes(), some(&[("game_dir", &user_text)]));
        std::fs::remove_dir_all(&root).unwrap();
    }

    // A save writes absolute paths and drops port.cfg's developer keys; a game_text without its pack stays listed.
    #[test]
    fn save_hygiene_and_missing_pack() {
        let root = std::env::temp_dir().join(format!("th08-launcher-save-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("port.cfg"),
            "invuln = on\nfuture = 1\ngame_text = de\n",
        )
        .unwrap();
        let dirs = settings::PlatformDirs {
            config: Some(root.clone()),
            ..Default::default()
        };
        let s = Settings::game_environment(dirs);
        assert_eq!(s.game_text.as_deref(), Some("de"));
        assert!(!s.invuln);
        let mut m = Model::new(&s, Folder::Native);
        let missing = m.pack_choices(Lang::En);
        assert_eq!(
            missing,
            [(
                "de".to_string(),
                "de (not installed: original text)".to_string()
            )]
        );
        m.save();
        assert_eq!(
            std::fs::read_to_string(root.join("port.cfg")).unwrap(),
            "future = 1\ngame_text = de\n"
        );
        edit(&mut m, "save_dir", "saves");
        let abs = std::env::current_dir().unwrap().join("saves");
        assert_eq!(m.changes(), some(&[("save_dir", &abs.to_string_lossy())]));
        std::fs::remove_dir_all(&root).unwrap();
    }

    // The cheat note follows the Cheats rows as edited, in every view that can start the game.
    #[test]
    fn cheats_note_rows() {
        let mut m = Model::new(&Settings::default(), Folder::Native);
        assert!(m.active_cheats().is_empty());
        assert_eq!(cheats_note(Lang::En, &[]), None);
        edit(&mut m, "invuln", "on");
        edit(&mut m, "enemy_dmg", "0");
        edit(&mut m, "turbo", "2");
        assert_eq!(m.active_cheats(), ["invuln"]);
        assert_eq!(
            cheats_note(Lang::En, &m.active_cheats()).as_deref(),
            Some("Cheats on: Invulnerable. Scores and replays are not saved.")
        );
        assert_eq!(m.settings().active_cheats(), ["invuln"]);
    }
}
