//! Launcher screen and game-folder flow: an egui settings screen generated from `settings::ENTRIES`.

use crate::i18n::{self, Lang};
use crate::settings::{self, Audience, ENTRIES, Entry, Group, Kind, Origin, Settings};
#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};
use th08_render::gfx::{Gfx, SafeArea};
use winit::event::WindowEvent;
use winit::window::Window;

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

// Lookup order: valid configured folder; invalid CLI/env folder is an error; bundled folder; folder dialog.
#[cfg(not(target_arch = "wasm32"))]
fn choose_game_dir(s: &Settings, bundled: impl FnOnce() -> Option<PathBuf>) -> Choice {
    if let Some(d) = &s.game_dir {
        if is_game_dir(d) {
            return Choice::Configured(d.clone());
        }
        let msg = format!("{}: no th08.dat in this folder", d.display());
        if s.origin("game_dir") > Origin::PortCfg {
            return Choice::Invalid(msg);
        }
        log::warn!("port.cfg {msg}");
    }
    match bundled() {
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

// Launcher screen: show the bundled folder in the `game_dir` row when none is configured; as a default-origin
// value it is not written to port.cfg.
#[cfg(not(target_arch = "wasm32"))]
pub fn prefill_bundled_game_dir(s: &mut Settings) {
    if let Choice::Bundled(d) = choose_game_dir(s, bundled_game_dir) {
        log::info!("bundled game folder: {}", d.display());
        s.set_from(Origin::Default, "game_dir", &d.to_string_lossy())
            .expect("game_dir accepts any path");
    }
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
            "{} does not contain th08.dat.\nSelect the Imperishable Night folder.",
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
    log::error!("{} does not contain th08.dat", dir.display());
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
    // Developer view: Cheats and Tools rows and the origin tags are shown.
    pub dev: bool,
    pub view: View,
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
        Self {
            rows,
            folder,
            dirs: s.dirs.clone(),
            dev,
            view: View::Settings,
        }
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

    #[cfg(not(target_arch = "wasm32"))]
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
    // other rows apply to this run only.
    pub fn changes(&self) -> Vec<(&'static str, Option<String>)> {
        self.rows
            .iter()
            .filter(|r| r.dirty && r.e.persist)
            .map(|r| {
                let keep = !r.value.is_unset() && r.value != r.default_value();
                (r.e.key, keep.then(|| r.value.text()))
            })
            .collect()
    }

    pub fn save(&self) {
        let changes = self.changes();
        if !changes.is_empty() {
            let c: Vec<(&str, Option<&str>)> =
                changes.iter().map(|(k, v)| (*k, v.as_deref())).collect();
            settings::write_port_cfg(&self.dirs, &c);
        }
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
                message_ui(
                    ui,
                    i18n::ui(
                        lang,
                        if cfg!(target_os = "android") {
                            "waiting_touch"
                        } else {
                            "waiting_shift"
                        },
                    ),
                );
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
            ui.heading(i18n::ui(lang, "title"));
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
                for g in Group::ALL {
                    self.group_ui(ui, g, lang);
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
            .default_open(matches!(g, Group::GameData | Group::Display) || set_here)
            .show(ui, |ui| {
                egui::Grid::new(g.id())
                    .num_columns(2)
                    .striped(true)
                    .spacing([12.0, 4.0])
                    .show(ui, |ui| {
                        for i in rows {
                            row_ui(ui, &mut self.rows[i], dev, lang);
                            ui.end_row();
                        }
                    });
            });
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
        });
    });
    act
}

// Help is shown once, under the value; the developer view adds the flag / env name and the origin tag.
fn row_ui(ui: &mut egui::Ui, r: &mut Row, dev: bool, lang: Lang) {
    let l = ui.label(i18n::label(lang, r.e));
    if dev {
        l.on_hover_text(format!("--{} / {}", r.e.key.replace('_', "-"), r.e.env));
    }
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            value_ui(ui, r, lang);
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
fn value_ui(ui: &mut egui::Ui, r: &mut Row, lang: Lang) {
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
            egui::ComboBox::from_id_salt(r.e.key)
                .selected_text(shown(c))
                .show_ui(ui, |ui| {
                    if unsettable {
                        ui.selectable_value(c, String::new(), shown(""));
                    }
                    for x in choices {
                        ui.selectable_value(c, x.to_string(), shown(x));
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
        s.set_from(Origin::PortCfg, "game_dir", &user.to_string_lossy())
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
}
