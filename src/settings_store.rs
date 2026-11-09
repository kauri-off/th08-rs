//! The settings rows behind every settings screen: values as edited, step, reset, control data and saving.

use crate::i18n::{self, Lang};
use crate::settings::{self, Audience, ENTRIES, Entry, Group, Kind, Origin, Settings};

// A settings row's value control as the menus paint it.
#[derive(Clone, Debug, PartialEq)]
pub enum Ctl {
    // Inline words, the selected one white (a value without a word in the list: none selected).
    Choices {
        words: Vec<String>,
        sel: Option<usize>,
    },
    // ◀ track ▶ and the number; `frac` 0..1 along the track.
    Slider {
        frac: f32,
        text: String,
        unit: String,
    },
    // Read-only value text with a › (activating the row opens or cycles it).
    Value(String),
    None,
}

// What a click / tap / drag on a control asks for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CtlOut {
    Pick(usize),
    Step(i32),
    Frac(f32),
}

#[derive(Clone, Copy)]
pub(crate) struct LiveRows {
    touch: bool,
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
pub(crate) fn saved_text(r: &Row) -> String {
    let t = r.value.text();
    #[cfg(not(target_arch = "wasm32"))]
    if matches!(r.e.kind, Kind::Path | Kind::File)
        && let Ok(p) = std::path::absolute(&t)
    {
        return p.to_string_lossy().into_owned();
    }
    t
}

// One table entry as edited on screen; `dirty` = differs from the value the screen opened with.
pub(crate) struct Row {
    pub(crate) e: &'static Entry,
    pub(crate) value: Value,
    pub(crate) initial: Value,
    pub(crate) dirty: bool,
    pub(crate) origin: Origin,
}

impl Row {
    pub(crate) fn new(e: &'static Entry, v: &str, origin: Origin) -> Self {
        let value = Value::parse(e.kind, v);
        Self {
            e,
            initial: value.clone(),
            value,
            dirty: false,
            origin,
        }
    }

    pub(crate) fn default_value(&self) -> Value {
        Value::parse(self.e.kind, self.e.default)
    }

    pub(crate) fn set(&mut self, v: Value) {
        self.dirty = v != self.initial;
        self.value = v;
    }

    // The value after ← / → (`wrap`: Z / A, which cycles and leaves numbers alone); true when it changed.
    pub(crate) fn apply(&mut self, dir: i32, wrap: bool) -> bool {
        let v = match (&self.value, self.e.kind) {
            (Value::Bool(b), _) => Value::Bool(if wrap { !b } else { dir > 0 }),
            (Value::Int(Some(n)), Kind::Int { min, max }) if !wrap => {
                let step = if max - min <= 16 { 1 } else { 5 };
                Value::Int(Some((n + i64::from(dir) * step).clamp(min, max)))
            }
            (Value::Choice(c), Kind::Enum(choices)) => {
                let all: Vec<&str> = (self.e.default.is_empty().then_some(""))
                    .into_iter()
                    .chain(choices.iter().copied())
                    .collect();
                let at = all.iter().position(|x| x == c).unwrap_or(0);
                let n = all.len();
                let to = match (dir > 0, wrap) {
                    (true, true) => (at + 1) % n,
                    (false, true) => (at + n - 1) % n,
                    (true, false) => (at + 1).min(n - 1),
                    (false, false) => at.saturating_sub(1),
                };
                Value::Choice(all[to].to_string())
            }
            _ => return false,
        };
        let changed = v != self.value;
        if changed {
            self.set(v);
        }
        changed
    }

    pub(crate) fn error(&self) -> Option<String> {
        (!self.value.is_unset()).then(|| self.e.check(&self.value.text()).err())?
    }
}

// The values an Enum row offers: the unset value first when the entry has no default, then the table's
// choices, then `extra` (value, shown name).
pub(crate) fn enum_values(r: &Row, choices: &[&str], extra: &[(String, String)]) -> Vec<String> {
    (r.e.default.is_empty().then(String::new))
        .into_iter()
        .chain(choices.iter().map(|c| (*c).to_string()))
        .chain(extra.iter().map(|(v, _)| v.clone()))
        .collect()
}

pub(crate) fn row_ctl(r: &Row, lang: Lang, extra: &[(String, String)]) -> Ctl {
    let shown = |v: &str| -> String {
        if let Some((_, n)) = extra.iter().find(|(x, _)| x == v) {
            return n.clone();
        }
        match i18n::word(lang, r.e, v) {
            Some(w) => w.to_string(),
            None if v.is_empty() => i18n::ui(lang, "not_set").to_string(),
            None => v.to_string(),
        }
    };
    match (&r.value, r.e.kind) {
        (Value::Bool(b), _) => Ctl::Choices {
            words: vec![
                i18n::ui(lang, "word_off").into(),
                i18n::ui(lang, "word_on").into(),
            ],
            sel: Some(usize::from(*b)),
        },
        (Value::Int(n), Kind::Int { min, max }) => {
            let v = n.unwrap_or(min);
            let word = n.and_then(|n| i18n::word(lang, r.e, &n.to_string()));
            Ctl::Slider {
                frac: (v - min) as f32 / (max - min).max(1) as f32,
                text: match (n, word) {
                    (None, _) => i18n::ui(lang, "not_set").into(),
                    (_, Some(w)) => w.into(),
                    (Some(n), None) => n.to_string(),
                },
                unit: if word.is_some() || n.is_none() {
                    String::new()
                } else {
                    i18n::unit(lang, r.e).trim().to_string()
                },
            }
        }
        (Value::Choice(c), Kind::Enum(choices)) => {
            let all = enum_values(r, choices, extra);
            Ctl::Choices {
                sel: all.iter().position(|x| x == c),
                words: all.iter().map(|v| shown(v)).collect(),
            }
        }
        _ => Ctl::None,
    }
}

pub(crate) fn row_input(r: &mut Row, out: CtlOut, extra: &[(String, String)]) -> bool {
    let v = match (out, &r.value, r.e.kind) {
        (CtlOut::Step(d), ..) => return r.apply(d, false),
        (CtlOut::Pick(i), Value::Bool(_), _) => Value::Bool(i == 1),
        (CtlOut::Pick(i), Value::Choice(_), Kind::Enum(choices)) => {
            match enum_values(r, choices, extra).get(i) {
                Some(v) => Value::Choice(v.clone()),
                None => return false,
            }
        }
        (CtlOut::Frac(f), Value::Int(_), Kind::Int { min, max }) => {
            Value::Int(Some(min + ((max - min) as f32 * f).round() as i64))
        }
        _ => return false,
    };
    let changed = v != r.value;
    if changed {
        r.set(v);
    }
    changed
}

// The edited settings: one row per `ENTRIES` entry.
pub struct SettingsStore {
    pub(crate) rows: Vec<Row>,
    pub(crate) dirs: settings::PlatformDirs,
    // Developer view: Cheats and Tools rows and the origin tags are shown.
    pub dev: bool,
    // In-game port menu: only `live` rows (Touch rows only while the touch overlay is on).
    pub(crate) live: Option<LiveRows>,
    // The game folder row holds this build's bundled data: never saved to port.cfg.
    pub(crate) bundled: bool,
}

impl SettingsStore {
    pub fn new(s: &Settings) -> Self {
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
            dirs: s.dirs.clone(),
            dev,
            live: None,
            bundled: s.origin("game_dir") == Origin::Bundled,
        }
    }

    // Writes port.cfg even with no row edited: it also drops the developer keys the file holds.
    pub(crate) fn write_all(&self) {
        let changes = self.changes();
        if !changes.is_empty() || self.dirs.config.is_some() {
            let c: Vec<(&str, Option<&str>)> =
                changes.iter().map(|(k, v)| (*k, v.as_deref())).collect();
            settings::write_port_cfg(&self.dirs, &c);
        }
    }

    // The port menu's rows: the `live` entries of the held game's settings.
    pub fn port_menu(s: &Settings, touch: bool) -> Self {
        let mut m = Self::new(s);
        m.dev = false;
        m.live = Some(LiveRows { touch });
        m
    }

    // The port menu's settings tabs: the groups with shown rows, Sound first.
    pub fn live_tabs(&self) -> Vec<Group> {
        const FIRST: [Group; 5] = [
            Group::Sound,
            Group::Display,
            Group::Accessibility,
            Group::Touch,
            Group::Language,
        ];
        FIRST
            .iter()
            .chain(Group::ALL.iter().filter(|g| !FIRST.contains(g)))
            .copied()
            .filter(|&g| !self.live_rows(g).is_empty())
            .collect()
    }

    // Keys of the shown rows of group `g`, in table order.
    pub fn live_rows(&self, g: Group) -> Vec<&'static str> {
        (0..self.rows.len())
            .filter(|&i| self.rows[i].e.group == g && self.shown(i))
            .map(|i| self.rows[i].e.key)
            .collect()
    }

    // ← / → on a row (`dir` −1 / 1): true when the value changed.
    pub fn step(&mut self, key: &str, dir: i32) -> bool {
        self.row_mut(key).apply(dir, false)
    }

    // Z / A on a row: a box toggles, a choice moves to the next one (wrapping), a number stays.
    pub fn activate(&mut self, key: &str) -> bool {
        self.row_mut(key).apply(1, true)
    }

    // The painted control of a Bool / Int / Enum row (`Ctl::None` for text and path rows).
    pub fn ctl(&self, key: &str, lang: Lang) -> Ctl {
        row_ctl(self.row(key), lang, &[])
    }

    // A click / tap / drag on a row's control; true when the value changed.
    pub fn ctl_input(&mut self, key: &str, out: CtlOut) -> bool {
        row_input(self.row_mut(key), out, &[])
    }

    // The row's label and help text in `lang`.
    pub fn row_text(&self, key: &str, lang: Lang) -> (&'static str, &'static str) {
        let e = self.row(key).e;
        (i18n::label(lang, e), i18n::help(lang, e))
    }

    // These rows back to their defaults (the port menu's "Reset this page").
    pub fn reset_rows(&mut self, keys: &[&str]) {
        for k in keys {
            let r = self.row_mut(k);
            r.set(r.default_value());
        }
    }

    // Keys of the rows the port menu shows.
    pub fn live_keys(&self) -> Vec<&'static str> {
        (0..self.rows.len())
            .filter(|&i| self.shown(i))
            .map(|i| self.rows[i].e.key)
            .collect()
    }

    // Writes the rows changed on screen to port.cfg (nothing when none changed).
    pub fn save_changes(&self) {
        let changes = self.changes();
        if !changes.is_empty() {
            let c: Vec<(&str, Option<&str>)> =
                changes.iter().map(|(k, v)| (*k, v.as_deref())).collect();
            settings::write_port_cfg(&self.dirs, &c);
        }
    }

    // The row's value text as edited (None: unset).
    pub fn value_text(&self, key: &str) -> Option<String> {
        let r = self.row(key);
        (!r.value.is_unset()).then(|| r.value.text())
    }

    // Whether row `i` is on this screen.
    pub(crate) fn shown(&self, i: usize) -> bool {
        let e = self.rows[i].e;
        e.key != "game_dir"
            && e.shown_on_this_platform()
            && (self.dev || e.audience == Audience::Player)
            && self
                .live
                .is_none_or(|l| e.live && (l.touch || e.group != Group::Touch))
    }

    // The window's fullscreen state (F11 / Alt+Enter) mirrored into the box; saved with the other edits on Play.
    pub fn set_fullscreen(&mut self, on: bool) {
        self.row_mut("fullscreen").set(Value::Bool(on));
    }

    // The screen language: the `language` row as edited (a change applies at once), auto = the system locale.
    pub fn lang(&self) -> Lang {
        i18n::resolve(Lang::from_code(&self.row("language").value.text()))
    }

    pub(crate) fn row(&self, key: &str) -> &Row {
        self.rows.iter().find(|r| r.e.key == key).unwrap()
    }

    pub(crate) fn row_mut(&mut self, key: &str) -> &mut Row {
        self.rows.iter_mut().find(|r| r.e.key == key).unwrap()
    }

    pub(crate) fn game_dir(&self) -> String {
        self.row("game_dir").value.text()
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

    // Active Cheats rows as edited on screen (keys).
    pub(crate) fn active_cheats(&self) -> Vec<&'static str> {
        self.rows
            .iter()
            .filter(|r| r.e.group == Group::Cheats && r.value != r.default_value())
            .map(|r| r.e.key)
            .collect()
    }

    pub(crate) fn reset(&mut self) {
        for r in self.rows.iter_mut().filter(|r| r.e.key != "game_dir") {
            r.set(r.default_value());
        }
    }
}
