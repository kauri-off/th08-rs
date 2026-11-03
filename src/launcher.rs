//! Launcher screen and game-folder flow, port infrastructure (no spec section): an egui settings screen generated
//! from `settings::ENTRIES`, drawn on the game's wgpu device and window before the game starts.

use crate::settings::{self, ENTRIES, Entry, Group, Kind, Origin, Settings};
#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};
use th08_render::gfx::Gfx;
use winit::event::WindowEvent;
use winit::window::Window;

// A game folder holds th08.dat (any letter case).
#[cfg(not(target_arch = "wasm32"))]
pub fn is_game_dir(dir: &Path) -> bool {
    crate::sources::find_file(dir, "th08.dat").is_file()
}

// Desktop platform dirs (ARCH_D §6.1 item 1), looked up by the entry point and passed to the settings: config =
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

// Where the game files come from: the `game_dir` row (native) or the files picked in the web page.
pub enum Folder {
    Native,
    Web(Vec<String>),
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

// One table entry as edited on screen. Value text "" = unset; booleans are "on"/"off".
struct Row {
    e: &'static Entry,
    value: String,
    initial: String,
    origin: Origin,
}

impl Row {
    fn default_value(&self) -> String {
        norm(self.e, self.e.default)
    }

    fn changed(&self) -> bool {
        self.value != self.initial
    }

    fn error(&self) -> Option<String> {
        (!self.value.is_empty()).then(|| self.e.check(&self.value).err())?
    }
}

fn norm(e: &Entry, v: &str) -> String {
    match e.kind {
        Kind::Bool => match settings::parse_bool(v) {
            Ok(true) => "on".into(),
            Ok(false) => "off".into(),
            Err(_) => v.into(),
        },
        _ => v.trim().into(),
    }
}

// Player-facing name of an entry; developer entries keep their key.
pub fn label(e: &Entry) -> String {
    let l = match e.key {
        "game_dir" => "Game folder",
        "scale" => "Window size",
        "fullscreen" => "Fullscreen",
        "vsync" => "VSync",
        "launcher" => "Show this screen at start",
        "save_dir" => "Save folder",
        "font" => "Dialogue font",
        "soundfont" => "MIDI SoundFont",
        "seed" => "RNG seed",
        "stage" => "Start at stage",
        "stage_frame" => "Stage start frame",
        "player" => "Shot type",
        "rank" => "Difficulty",
        "power" => "Starting power",
        "continues" => "Continues used",
        "ending" => "Play ending",
        "replay" => "Play replay",
        "replay_speed" => "Replay speed",
        _ => return e.key.replace('_', " "),
    };
    l.into()
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
            .map(|e| {
                let v = norm(e, s.value(e.key).unwrap_or(""));
                Row {
                    e,
                    value: v.clone(),
                    initial: v,
                    origin: s.origin(e.key),
                }
            })
            .collect::<Vec<Row>>();
        // A developer switch given on the command line / env opens the developer view.
        let dev = rows
            .iter()
            .any(|r| r.e.group.developer() && r.origin > Origin::PortCfg);
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
        let r = self
            .rows
            .iter_mut()
            .find(|r| r.e.key == "fullscreen")
            .unwrap();
        r.value = if on { "on" } else { "off" }.into();
    }

    fn row(&self, key: &str) -> &Row {
        self.rows.iter().find(|r| r.e.key == key).unwrap()
    }

    fn folder_ok(&self) -> bool {
        match &self.folder {
            #[cfg(not(target_arch = "wasm32"))]
            Folder::Native => {
                let v = &self.row("game_dir").value;
                !v.is_empty() && is_game_dir(Path::new(v))
            }
            #[cfg(target_arch = "wasm32")]
            Folder::Native => false,
            Folder::Web(names) => names.iter().any(|n| n.eq_ignore_ascii_case("th08.dat")),
        }
    }

    pub fn can_play(&self) -> bool {
        self.folder_ok() && self.rows.iter().all(|r| r.error().is_none())
    }

    // The final settings: unchanged rows keep their origin, edited ones count as set on the command line.
    pub fn settings(&self) -> Settings {
        Settings::from_values(
            self.dirs.clone(),
            self.rows.iter().filter(|r| !r.value.is_empty()).map(|r| {
                let o = if r.changed() { Origin::Cli } else { r.origin };
                (r.e, r.value.as_str(), o)
            }),
        )
    }

    // port.cfg / localStorage changes: every player row edited on screen, removed when back at its default. Cheats
    // and Tools edits apply to this run only.
    pub fn changes(&self) -> Vec<(&'static str, Option<&str>)> {
        self.rows
            .iter()
            .filter(|r| r.changed() && !r.e.group.developer())
            .map(|r| {
                let keep = !r.value.is_empty() && r.value != r.default_value();
                (r.e.key, keep.then_some(r.value.as_str()))
            })
            .collect()
    }

    pub fn save(&self) {
        let changes = self.changes();
        if !changes.is_empty() {
            settings::write_port_cfg(&self.dirs, &changes);
        }
    }

    fn reset(&mut self) {
        for r in self.rows.iter_mut().filter(|r| r.e.key != "game_dir") {
            r.value = r.default_value();
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        match &self.view {
            View::Settings => {
                let before = self.row("fullscreen").value.clone();
                let act = self.settings_ui(ui);
                let after = &self.row("fullscreen").value;
                if act.is_none() && *after != before {
                    return Some(Action::Fullscreen(after == "on"));
                }
                act
            }
            View::Waiting => {
                message_ui(
                    ui,
                    "Starting the game…  hold Shift to open the settings screen",
                );
                None
            }
            View::Loading => {
                message_ui(ui, "Loading th08.dat…");
                None
            }
            View::Error(msg) => error_ui(ui, msg),
            View::PortMenu => port_menu_ui(ui),
        }
    }

    fn settings_ui(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let mut act = None;
        egui::Panel::top("folder").show(ui, |ui| {
            ui.add_space(4.0);
            ui.heading("th08 — Touhou 8 port");
            self.folder_ui(ui);
            ui.add_space(4.0);
        });
        egui::Panel::bottom("buttons").show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let play = ui.add_enabled(
                    self.can_play(),
                    egui::Button::new(egui::RichText::new("  Play  ").strong()),
                );
                if play.clicked() {
                    act = Some(Action::Play);
                }
                if ui.button("Reset to defaults").clicked() {
                    self.reset();
                }
                if cfg!(not(target_arch = "wasm32")) && ui.button("Quit").clicked() {
                    act = Some(Action::Quit);
                }
                ui.checkbox(&mut self.dev, "Developer")
                    .on_hover_text("show cheats and tools (used for this run only, never saved)");
                if !self.folder_ok() {
                    ui.colored_label(ui.visuals().warn_fg_color, "select the game folder first");
                } else if self.rows.iter().any(|r| r.error().is_some()) {
                    ui.colored_label(ui.visuals().error_fg_color, "fix the marked values");
                }
            });
            ui.add_space(4.0);
        });
        egui::CentralPanel::default_margins().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                if !self.dev {
                    let active: Vec<String> = self
                        .rows
                        .iter()
                        .filter(|r| r.e.group.developer() && r.value != r.default_value())
                        .map(|r| r.e.key.replace('_', " "))
                        .collect();
                    if !active.is_empty() {
                        ui.colored_label(
                            ui.visuals().error_fg_color,
                            format!("Developer settings active: {}", active.join(", ")),
                        );
                    }
                }
                let dev = self.dev;
                for g in Group::ALL.into_iter().filter(|g| dev || !g.developer()) {
                    self.group_ui(ui, g);
                }
            });
        });
        act
    }

    fn folder_ui(&mut self, ui: &mut egui::Ui) {
        let ok = self.folder_ok();
        ui.horizontal(|ui| {
            ui.label("Game folder:");
            match &self.folder {
                Folder::Native => {
                    let r = self
                        .rows
                        .iter_mut()
                        .find(|r| r.e.key == "game_dir")
                        .unwrap();
                    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
                    if ui.button("Browse…").clicked()
                        && let Some(d) = folder_dialog()
                    {
                        r.value = d.to_string_lossy().into_owned();
                    }
                    #[cfg(not(target_os = "android"))]
                    ui.add(
                        egui::TextEdit::singleline(&mut r.value)
                            .hint_text("folder with th08.dat")
                            .desired_width(f32::INFINITY),
                    );
                    // Android: no soft keyboard under NativeActivity (launcher review item 9), so the fixed folder
                    // set by `android_main` is shown, not edited.
                    #[cfg(target_os = "android")]
                    ui.label(r.value.as_str());
                }
                Folder::Web(names) => {
                    ui.label(if names.is_empty() {
                        "no files".to_string()
                    } else {
                        names.join(", ")
                    });
                }
            }
        });
        ui.horizontal(|ui| {
            if ok {
                ui.colored_label(egui::Color32::from_rgb(110, 200, 110), "th08.dat found");
            } else {
                ui.colored_label(ui.visuals().error_fg_color, "no th08.dat in this folder");
            }
            if self.dev && matches!(self.folder, Folder::Native) {
                origin_label(ui, self.row("game_dir"));
            }
        });
        #[cfg(target_os = "android")]
        if !ok {
            let dir = &self.row("game_dir").value;
            ui.label(format!(
                "Copy th08.dat and thbgm.dat from your Touhou 8 folder into {dir}\n\
                 e.g. adb push th08.dat thbgm.dat {dir}/ — then this screen picks them up."
            ));
        }
    }

    fn group_ui(&mut self, ui: &mut egui::Ui, g: Group) {
        let web = matches!(self.folder, Folder::Web(_));
        // Folder rows are at the top; on the web the files come from the page.
        let rows: Vec<usize> = (0..self.rows.len())
            .filter(|&i| self.rows[i].e.group == g && self.rows[i].e.key != "game_dir")
            .filter(|&i| !(web && matches!(self.rows[i].e.kind, Kind::Path | Kind::File)))
            .collect();
        if rows.is_empty() {
            return;
        }
        let set_here = rows.iter().any(|&i| self.rows[i].origin > Origin::PortCfg);
        let dev = self.dev;
        egui::CollapsingHeader::new(egui::RichText::new(g.title()).strong())
            .default_open(matches!(g, Group::GameData | Group::Display) || set_here)
            .show(ui, |ui| {
                egui::Grid::new(g.title())
                    .num_columns(2)
                    .striped(true)
                    .spacing([12.0, 4.0])
                    .show(ui, |ui| {
                        for i in rows {
                            row_ui(ui, &mut self.rows[i], dev);
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

fn error_ui(ui: &mut egui::Ui, msg: &str) -> Option<Action> {
    let mut act = None;
    egui::Panel::bottom("buttons").show(ui, |ui| {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui.button("  Back  ").clicked() {
                act = Some(Action::Back);
            }
            if cfg!(not(target_arch = "wasm32")) && ui.button("Quit").clicked() {
                act = Some(Action::Quit);
            }
        });
        ui.add_space(4.0);
    });
    egui::CentralPanel::default_margins().show(ui, |ui| {
        ui.add_space(4.0);
        ui.heading("The game could not start");
        ui.add_space(8.0);
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.colored_label(ui.visuals().error_fg_color, msg);
        });
    });
    act
}

// Port menu stub: the hook for the touch-layout editor; host settings only will go here, never core ones (seed,
// stage, cheats), so a mid-game visit cannot change the game's logic.
fn port_menu_ui(ui: &mut egui::Ui) -> Option<Action> {
    let mut act = None;
    egui::CentralPanel::default_margins().show(ui, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() / 3.0);
            ui.heading("Port menu");
            ui.add_space(4.0);
            ui.label(egui::RichText::new("the game is paused").weak());
            ui.add_space(12.0);
            if ui
                .button(egui::RichText::new("  Resume  ").strong())
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
fn row_ui(ui: &mut egui::Ui, r: &mut Row, dev: bool) {
    let l = ui.label(label(r.e));
    if dev {
        l.on_hover_text(format!("--{} / {}", r.e.key.replace('_', "-"), r.e.env));
    }
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            value_ui(ui, r);
            if dev {
                origin_label(ui, r);
            }
        });
        if let Some(err) = r.error() {
            ui.colored_label(ui.visuals().error_fg_color, err);
        }
        ui.label(egui::RichText::new(r.e.help).small().weak());
    });
}

fn value_ui(ui: &mut egui::Ui, r: &mut Row) {
    let unsettable = r.e.default.is_empty();
    match r.e.kind {
        Kind::Bool => {
            let mut on = r.value == "on";
            if ui.checkbox(&mut on, "").changed() {
                r.value = if on { "on" } else { "off" }.into();
            }
        }
        Kind::Int { min, max } => {
            let mut set = !r.value.is_empty();
            if unsettable && ui.checkbox(&mut set, "").changed() {
                r.value = if set {
                    min.max(0).min(max).to_string()
                } else {
                    String::new()
                };
            }
            if set {
                let mut n: f64 = r.value.parse().unwrap_or(min as f64);
                let range = min as f64..=max as f64;
                let changed = if max - min <= 16 {
                    ui.add(
                        egui::Slider::new(&mut n, range)
                            .step_by(1.0)
                            .fixed_decimals(0),
                    )
                    .changed()
                } else {
                    ui.add(
                        egui::DragValue::new(&mut n)
                            .range(range)
                            .speed(1.0)
                            .fixed_decimals(0),
                    )
                    .changed()
                };
                if changed {
                    r.value = (n.round() as i64).to_string();
                }
            }
        }
        Kind::Enum(choices) => {
            let shown = if r.value.is_empty() {
                "(not set)"
            } else {
                r.value.as_str()
            };
            egui::ComboBox::from_id_salt(r.e.key)
                .selected_text(shown.to_string())
                .show_ui(ui, |ui| {
                    if unsettable {
                        ui.selectable_value(&mut r.value, String::new(), "(not set)");
                    }
                    for c in choices {
                        ui.selectable_value(&mut r.value, c.to_string(), *c);
                    }
                });
        }
        // Android: text rows are read-only (no soft keyboard); they are set through port.cfg.
        #[cfg(target_os = "android")]
        Kind::Text(_) | Kind::Path | Kind::File => {
            ui.label(if r.value.is_empty() {
                "(not set)"
            } else {
                r.value.as_str()
            });
        }
        #[cfg(not(target_os = "android"))]
        Kind::Text(hint) => {
            ui.add(
                egui::TextEdit::singleline(&mut r.value)
                    .hint_text(hint)
                    .desired_width(220.0),
            );
        }
        #[cfg(not(target_os = "android"))]
        Kind::Path | Kind::File => {
            ui.add(
                egui::TextEdit::singleline(&mut r.value)
                    .hint_text(r.e.hint())
                    .desired_width(220.0),
            );
            #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
            if ui.button("Browse…").clicked() {
                let d = rfd::FileDialog::new().set_title(r.e.help);
                let picked = if r.e.key == "soundfont" {
                    d.add_filter("SoundFont", &["sf2"]).pick_file()
                } else if r.e.kind == Kind::File {
                    d.add_filter("fonts", &["ttf", "otf", "ttc"]).pick_file()
                } else {
                    d.pick_folder()
                };
                if let Some(p) = picked {
                    r.value = p.to_string_lossy().into_owned();
                }
            }
        }
    }
}

// "set by TH08_STAGE" / "set by --stage" for values from the environment or command line, "changed" after an edit.
fn origin_label(ui: &mut egui::Ui, r: &Row) {
    let text = if r.changed() {
        "changed".to_string()
    } else {
        match r.origin {
            Origin::Cli => format!("set by --{}", r.e.key.replace('_', "-")),
            Origin::Env if cfg!(target_arch = "wasm32") => format!("set by URL ?{}=", r.e.url()),
            Origin::Env => format!("set by {}", r.e.env),
            Origin::PortCfg if cfg!(target_arch = "wasm32") => "saved".into(),
            Origin::PortCfg => "port.cfg".into(),
            Origin::Default => return,
        }
    };
    let c = match (r.changed(), r.origin) {
        (true, _) => egui::Color32::from_rgb(120, 170, 255),
        (_, Origin::Cli | Origin::Env) => egui::Color32::from_rgb(240, 190, 80),
        _ => ui.visuals().weak_text_color(),
    };
    ui.colored_label(c, text);
}

// The 640×480 layout scaled to the window.
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
        self.p
            .ctx
            .set_zoom_factor(zoom((w, h)) / window.scale_factor() as f32);
        let raw = self.state.take_egui_input(window);
        let (out, act) = self.p.pass(raw);
        self.state
            .handle_platform_output(window, out.platform_output.clone());
        let delay = out
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .map_or(std::time::Duration::MAX, |v| v.repaint_delay);
        self.repaint_at =
            (delay < std::time::Duration::from_secs(5)).then(|| web_time::Instant::now() + delay);
        let Some(surface) = &gfx.surface else {
            return act;
        };
        let frame = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f)
            | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            _ => {
                gfx.reconfigure();
                window.request_redraw();
                return act;
            }
        };
        let view = frame.texture.create_view(&Default::default());
        let enc = self.p.paint(gfx, out, &view, (w, h));
        gfx.queue.submit([enc.finish()]);
        gfx.queue.present(frame);
        act
    }
}

impl Painter {
    fn new(gfx: &Gfx, model: Model) -> Self {
        let ctx = egui::Context::default();
        ctx.set_visuals(egui::Visuals::dark());
        let renderer = egui_wgpu::Renderer::new(
            &gfx.device,
            gfx.surface_format(),
            egui_wgpu::RendererOptions::default(),
        );
        Self {
            model,
            ctx,
            renderer,
            pending: egui::TexturesDelta::default(),
        }
    }

    fn pass(&mut self, raw: egui::RawInput) -> (egui::FullOutput, Option<Action>) {
        let mut act = None;
        let model = &mut self.model;
        let mut out = self.ctx.run_ui(raw, |ui| {
            if let Some(a) = model.ui(ui) {
                act = Some(a);
            }
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
        let mut enc = gfx.device.create_command_encoder(&Default::default());
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
pub fn render_image(gfx: &Gfx, model: Model, (w, h): (u32, u32)) -> image::RgbaImage {
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
    let view = tex.create_view(&Default::default());
    let ppp = zoom((w, h));
    let mut enc = None;
    for i in 0..4 {
        let mut raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(w as f32 / ppp, h as f32 / ppp),
            )),
            time: Some(i as f64 / 60.0),
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

    fn edit(m: &mut Model, key: &str, v: &str) {
        m.rows.iter_mut().find(|r| r.e.key == key).unwrap().value = v.into();
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
            [("fullscreen", Some("on")), ("launcher", Some("off"))]
        );
        let r = m.settings();
        assert!(r.invuln && r.replay_save == Some(3) && r.fullscreen && !r.launcher);
        assert_eq!((r.turbo, r.origin("turbo")), (2, Origin::PortCfg));
        edit(&mut m, "fullscreen", "off");
        assert_eq!(m.changes(), [("launcher", Some("off"))]);
        s.set_from(Origin::Cli, "invuln", "on").unwrap();
        assert!(Model::new(&s, Folder::Native).dev);
    }

    #[test]
    fn labels() {
        assert_eq!(label(settings::find("scale").unwrap()), "Window size");
        assert_eq!(label(settings::find("session_rng").unwrap()), "session rng");
        for e in ENTRIES.iter().filter(|e| !e.group.developer()) {
            let l = label(e);
            assert!(l.starts_with(char::is_uppercase), "{}: no label", e.key);
        }
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
