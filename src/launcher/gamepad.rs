//! Launcher Gamepad section: the connected pad, per-row button capture with Cancel / timeout, conflicts, defaults.

use super::{Model, Value};
use crate::i18n::{self, Lang};
use crate::platform::padmap;
use crate::settings;

// A capture with no button press ends after this long.
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

// How often the section polls the pad while it is open (gilrs has no wake-up); faster while capturing.
const POLL: std::time::Duration = std::time::Duration::from_millis(500);
const POLL_CAPTURE: std::time::Duration = std::time::Duration::from_millis(30);

#[derive(Default)]
pub(super) struct PadUi {
    // The window's own gamepad reader (desktop launcher); None = no pad source (tools, web, Android).
    joystick: Option<crate::joystick::Joystick>,
    // Physical input of the last poll, for press edges.
    prev: u32,
    // The Gamepad row waiting for a button press and when it started.
    pub(super) capture: Option<(&'static str, web_time::Instant)>,
}

impl Model {
    // Desktop launcher: read the pad for captures and its name.
    pub fn attach_pad(&mut self) {
        self.pad.joystick = Some(crate::joystick::Joystick::new());
    }

    // Tools: start a button capture on a Gamepad row.
    pub fn start_pad_capture(&mut self, key: &'static str) {
        self.capture = None;
        self.pad.prev = self.pad.joystick.as_mut().map_or(0, |j| j.read().0);
        self.pad.capture = Some((key, web_time::Instant::now()));
    }

    pub fn cancel_pad_capture(&mut self) -> bool {
        self.pad.capture.take().is_some()
    }

    // A pad button pressed while a Gamepad row waits for one: added to that row; true when it was taken.
    pub fn capture_pad(&mut self, button: usize) -> bool {
        let Some((key, _)) = self.pad.capture else {
            return false;
        };
        if padmap::name(button).is_none() {
            return false;
        }
        self.pad.capture = None;
        let r = self.row_mut(key);
        let mut bs = padmap::parse(&r.value.text()).unwrap_or_default();
        if !bs.contains(&button) && bs.len() < padmap::MAX_PER_INPUT {
            bs.push(button);
            r.set(Value::Text(padmap::format(&bs)));
        }
        true
    }

    // Polls the pad: a new press goes to the capture, a timed-out capture ends. Returns the pad's name.
    fn pump_pad(&mut self, ctx: &egui::Context) -> Option<String> {
        if let Some((_, t)) = self.pad.capture
            && t.elapsed() >= TIMEOUT
        {
            self.pad.capture = None;
        }
        ctx.request_repaint_after(if self.pad.capture.is_some() {
            POLL_CAPTURE
        } else {
            POLL
        });
        let j = self.pad.joystick.as_mut()?;
        let (phys, _) = j.read();
        let name = j.active_name();
        let pressed = phys & !self.pad.prev;
        self.pad.prev = phys;
        if let Some(b) = padmap::first_bindable(pressed) {
            self.capture_pad(b);
        }
        name
    }

    pub(super) fn pad_section_ui(&mut self, ui: &mut egui::Ui, rows: &[usize], lang: Lang) {
        let name = self.pump_pad(ui.ctx());
        match name {
            Some(n) => ui.label(i18n::ui_fmt(lang, "pad_connected", &[("name", &n)])),
            None => ui.label(egui::RichText::new(i18n::ui(lang, "pad_none")).weak()),
        };
        let binds: Vec<Vec<usize>> = rows
            .iter()
            .map(|&i| padmap::parse(&self.rows[i].value.text()).unwrap_or_default())
            .collect();
        let custom = binds
            .iter()
            .zip(&padmap::INPUTS)
            .any(|(b, input)| *b != padmap::parse(input.default).unwrap_or_default());
        ui.label(i18n::ui(
            lang,
            if custom {
                "pad_mode_port"
            } else {
                "pad_mode_cfg"
            },
        ));
        ui.add_space(4.0);
        egui::Grid::new("gamepad_rows")
            .num_columns(2)
            .striped(true)
            .spacing([12.0, 4.0])
            .show(ui, |ui| {
                self.pad_rows_ui(ui, rows, &binds, lang);
                dir_rows_ui(ui, lang);
            });
        ui.add_space(6.0);
        if ui.button(i18n::ui(lang, "pad_defaults")).clicked() {
            self.pad.capture = None;
            for input in &padmap::INPUTS {
                self.row_mut(input.key)
                    .set(Value::Text(input.default.into()));
            }
        }
        ui.label(
            egui::RichText::new(i18n::ui(lang, "pad_hint"))
                .small()
                .weak(),
        );
    }

    fn pad_rows_ui(&mut self, ui: &mut egui::Ui, rows: &[usize], binds: &[Vec<usize>], lang: Lang) {
        let conflicts: Vec<String> = binds
            .iter()
            .enumerate()
            .map(|(n, b)| {
                let mut others: Vec<&str> = Vec::new();
                for (m, ob) in binds.iter().enumerate() {
                    if m != n && b.iter().any(|k| ob.contains(k)) {
                        others.push(input_label(lang, self.rows[rows[m]].e));
                    }
                }
                others.join(", ")
            })
            .collect();
        let mut capture = self.pad.capture;
        for (n, &i) in rows.iter().enumerate() {
            let r = &mut self.rows[i];
            ui.label(input_label(lang, r.e))
                .on_hover_text(i18n::summary(lang, r.e));
            ui.vertical(|ui| {
                ui.horizontal_wrapped(|ui| {
                    let mut bs = binds[n].clone();
                    let mut remove = None;
                    for (j, &b) in bs.iter().enumerate() {
                        if ui
                            .button(format!("{}  ×", padmap::label(b)))
                            .on_hover_text(padmap::name(b).unwrap_or("?"))
                            .clicked()
                        {
                            remove = Some(j);
                        }
                    }
                    if bs.is_empty() {
                        let none = i18n::word(lang, r.e, "none").unwrap_or("none");
                        ui.label(egui::RichText::new(none).weak());
                    }
                    if let Some(j) = remove {
                        bs.remove(j);
                        r.set(Value::Text(padmap::format(&bs)));
                    }
                    match capture {
                        Some((key, t)) if key == r.e.key => {
                            let left = TIMEOUT.saturating_sub(t.elapsed()).as_secs() + 1;
                            ui.colored_label(
                                egui::Color32::from_rgb(120, 170, 255),
                                i18n::ui_fmt(lang, "pad_press", &[("s", &left.to_string())]),
                            );
                            if ui.button(i18n::ui(lang, "cancel")).clicked() {
                                capture = None;
                            }
                        }
                        _ => {
                            if bs.len() < padmap::MAX_PER_INPUT
                                && ui
                                    .button("+")
                                    .on_hover_text(i18n::ui(lang, "pad_add"))
                                    .clicked()
                            {
                                capture = Some((r.e.key, web_time::Instant::now()));
                            }
                        }
                    }
                });
                if !conflicts[n].is_empty() {
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        i18n::ui_fmt(lang, "keys_conflict", &[("list", &conflicts[n])]),
                    );
                }
                if let Some(err) = r.error() {
                    ui.colored_label(ui.visuals().error_fg_color, err);
                }
            });
            ui.end_row();
        }
        match capture {
            Some((key, _)) if self.pad.capture.map(|c| c.0) != Some(key) => {
                self.start_pad_capture(key);
            }
            _ => self.pad.capture = capture,
        }
    }
}

// The section's row label: the game input's Keyboard label (`pad_shot` → Shot).
fn input_label(lang: Lang, e: &settings::Entry) -> &'static str {
    let key = e.key.replacen("pad_", "key_", 1);
    settings::find(&key).map_or_else(|| i18n::label(lang, e), |k| i18n::label(lang, k))
}

// The fixed directions, read-only: the left stick (th08.cfg thresholds) and the d-pad.
fn dir_rows_ui(ui: &mut egui::Ui, lang: Lang) {
    for (key, arrow) in [
        ("key_up", "↑"),
        ("key_down", "↓"),
        ("key_left", "←"),
        ("key_right", "→"),
    ] {
        let e = settings::find(key).expect("keyboard row");
        ui.label(egui::RichText::new(i18n::label(lang, e)).weak());
        let stick = i18n::ui(lang, "pad_stick");
        let text = format!("{stick} {arrow}   {} {arrow}", i18n::ui(lang, "pad_dpad"));
        ui.label(egui::RichText::new(text).weak());
        ui.end_row();
    }
}
