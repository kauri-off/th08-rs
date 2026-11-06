//! Launcher Replays tab: the replay folder's list, Watch, import (file picker, drag and drop), export and delete.

use super::{Action, Model, Value};
use crate::i18n::{self, Lang};
use crate::replays::{self, Entry, Sort};
use std::path::{Path, PathBuf};

#[derive(Default)]
pub(super) struct ReplayTab {
    // None = not read since the tab opened or the folder changed.
    list: Option<Vec<Entry>>,
    sort: Sort,
    confirm: Option<PathBuf>,
    status: Option<Result<String, String>>,
}

impl Model {
    fn replay_dir(&self) -> Option<PathBuf> {
        self.settings().storage_dir().map(|d| replays::dir(&d))
    }

    // Leaving the tab drops the list, so a game run or a file manager change shows on the next visit.
    pub(super) fn forget_replays(&mut self) {
        self.rp.list = None;
    }

    fn reload_replays(&mut self) {
        let mut v = self
            .replay_dir()
            .map(|d| replays::list(&d))
            .unwrap_or_default();
        replays::sort(&mut v, self.rp.sort);
        self.rp.list = Some(v);
    }

    // Each file is validated and copied; the message names every failure.
    pub(super) fn import_replays(&mut self, files: &[PathBuf], lang: Lang) {
        let Some(dir) = self.replay_dir() else {
            self.rp.status = Some(Err(i18n::ui(lang, "rp_no_save").into()));
            return;
        };
        let mut ok = 0;
        let mut errs = Vec::new();
        for f in files {
            match replays::import_file(&dir, f) {
                Ok(_) => ok += 1,
                Err(e) => errs.push(i18n::ui_fmt(
                    lang,
                    "rp_import_failed",
                    &[
                        ("file", &f.file_name().unwrap_or_default().to_string_lossy()),
                        ("err", &e),
                    ],
                )),
            }
        }
        let n = ok.to_string();
        let done = i18n::ui_fmt(lang, "rp_imported", &[("n", &n)]);
        self.rp.status = Some(if errs.is_empty() {
            Ok(done)
        } else {
            Err(format!("{done}\n{}", errs.join("\n")))
        });
        self.rp.list = None;
    }

    fn pick_replays(&mut self, lang: Lang) {
        if let Some(files) = rfd::FileDialog::new()
            .set_title(i18n::ui(lang, "rp_import"))
            .add_filter("rpy", &["rpy", "RPY"])
            .pick_files()
        {
            self.import_replays(&files, lang);
        }
    }

    fn export_replay(&mut self, path: &Path, lang: Lang) {
        let Some(dest) = rfd::FileDialog::new()
            .set_title(i18n::ui(lang, "rp_export"))
            .pick_folder()
        else {
            return;
        };
        self.rp.status = Some(
            replays::export(path, &dest)
                .map(|p| i18n::ui_fmt(lang, "rp_exported", &[("path", &p.display().to_string())]))
                .map_err(|e| i18n::ui_fmt(lang, "rp_export_failed", &[("err", &e)])),
        );
    }

    // The game starts straight into the replay through the `replay` row (this run only).
    fn watch(&mut self, path: &Path) -> Action {
        self.row_mut("replay")
            .set(Value::Text(path.display().to_string()));
        Action::Play
    }

    pub(super) fn replays_ui(&mut self, ui: &mut egui::Ui, lang: Lang) -> Option<Action> {
        let mut act = None;
        ui.horizontal_wrapped(|ui| {
            if ui.button(i18n::ui(lang, "rp_import")).clicked() {
                self.pick_replays(lang);
            }
            if ui.button(i18n::ui(lang, "rp_refresh")).clicked() {
                self.rp.list = None;
            }
            ui.separator();
            ui.label(i18n::ui(lang, "rp_sort"));
            let before = self.rp.sort;
            ui.selectable_value(
                &mut self.rp.sort,
                Sort::Date,
                i18n::ui(lang, "rp_sort_date"),
            );
            ui.selectable_value(
                &mut self.rp.sort,
                Sort::Score,
                i18n::ui(lang, "rp_sort_score"),
            );
            if self.rp.sort != before
                && let Some(v) = &mut self.rp.list
            {
                replays::sort(v, self.rp.sort);
            }
        });
        ui.label(egui::RichText::new(i18n::ui(lang, "rp_hint")).weak());
        match &self.rp.status {
            Some(Ok(m)) => {
                ui.label(m);
            }
            Some(Err(e)) => {
                ui.colored_label(ui.visuals().error_fg_color, e);
            }
            None => {}
        }
        ui.add_space(4.0);
        if self.rp.list.is_none() {
            self.reload_replays();
        }
        let list = self.rp.list.take().unwrap_or_default();
        if list.is_empty() {
            let dir = self
                .replay_dir()
                .map_or_else(String::new, |d| d.display().to_string());
            ui.label(i18n::ui_fmt(lang, "rp_none", &[("dir", &dir)]));
        }
        let can_play = self.can_play();
        for e in &list {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_width(ui.available_width());
                match &e.info {
                    Ok(i) => {
                        ui.horizontal_wrapped(|ui| {
                            let name = if i.name.is_empty() { "-" } else { &i.name };
                            ui.label(egui::RichText::new(name).strong());
                            ui.label(format!(
                                "{} · {} · {}",
                                replays::shot_label(lang, i.shot),
                                replays::rank_label(lang, i.difficulty),
                                replays::stage_label(lang, i.stage)
                            ));
                            if i.practice {
                                ui.label(egui::RichText::new(i18n::ui(lang, "rp_practice")).weak());
                            }
                            ui.label(
                                egui::RichText::new(group_digits(i.score))
                                    .strong()
                                    .monospace(),
                            );
                        });
                    }
                    Err(err) => {
                        ui.colored_label(
                            ui.visuals().error_fg_color,
                            i18n::ui_fmt(lang, "rp_damaged", &[("err", err)]),
                        );
                    }
                }
                ui.horizontal_wrapped(|ui| {
                    let detail = match &e.info {
                        Ok(i) => i18n::ui_fmt(
                            lang,
                            "rp_detail",
                            &[
                                ("file", &e.file),
                                ("date", &i.date),
                                ("slow", &format!("{:.2}", i.slowdown)),
                            ],
                        ),
                        Err(_) => e.file.clone(),
                    };
                    ui.label(egui::RichText::new(detail).small().weak());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(i18n::ui(lang, "rp_delete")).clicked() {
                            self.rp.confirm = Some(e.path.clone());
                        }
                        if e.info.is_ok() {
                            if ui.button(i18n::ui(lang, "rp_export")).clicked() {
                                self.export_replay(&e.path, lang);
                            }
                            let w = ui
                                .add_enabled(
                                    can_play,
                                    egui::Button::new(
                                        egui::RichText::new(format!(
                                            "▶ {}",
                                            i18n::ui(lang, "rp_watch")
                                        ))
                                        .strong(),
                                    ),
                                )
                                .on_disabled_hover_text(i18n::ui(lang, "select_folder_first"));
                            if w.clicked() {
                                act = Some(e.path.clone());
                            }
                        }
                    });
                });
            });
        }
        self.rp.list = Some(list);
        self.confirm_ui(ui, lang);
        act.map(|p| self.watch(&p))
    }

    fn confirm_ui(&mut self, ui: &mut egui::Ui, lang: Lang) {
        let Some(path) = self.rp.confirm.clone() else {
            return;
        };
        let file = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let m = egui::Modal::new(egui::Id::unique("rp_confirm")).show(ui.ctx(), |ui| {
            ui.label(i18n::ui_fmt(lang, "rp_delete_confirm", &[("file", &file)]));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui
                    .button(
                        egui::RichText::new(i18n::ui(lang, "rp_delete"))
                            .color(ui.visuals().error_fg_color),
                    )
                    .clicked()
                {
                    self.rp.status = Some(
                        replays::delete(&path)
                            .map(|()| i18n::ui_fmt(lang, "rp_deleted", &[("file", &file)])),
                    );
                    self.rp.confirm = None;
                    self.rp.list = None;
                }
                if ui.button(i18n::ui(lang, "cancel")).clicked() {
                    self.rp.confirm = None;
                }
            });
        });
        if m.should_close() {
            self.rp.confirm = None;
        }
    }
}

// 1234567890 → "1,234,567,890".
fn group_digits(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::{Folder, Model};
    use crate::i18n::Lang;
    use crate::settings::{PlatformDirs, Settings};

    // Dropped files land in the save dir's replay folder; Watch plays the file through the `replay` setting only.
    #[test]
    fn drop_then_watch() {
        let root = std::env::temp_dir().join(format!("th08-replaytab-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let mut r = th08_core::replay::Replay::default();
        r.stages[0] = Some(th08_core::replay::StageRecord {
            inputs: vec![0, 1],
            ..Default::default()
        });
        let src = root.join("mine.rpy");
        std::fs::write(&src, r.encode(&mut th08_core::rng::Rng::seeded(1))).unwrap();
        std::fs::write(root.join("junk.rpy"), b"junk").unwrap();
        let mut s = Settings::default();
        s.dirs = PlatformDirs {
            config: Some(root.join("cfg")),
            save: Some(root.join("save")),
            soundfonts: Vec::new(),
        };
        let mut m = Model::new(&s, Folder::Native);
        m.import_replays(&[src.clone(), root.join("junk.rpy")], Lang::En);
        let msg = m.rp.status.clone().unwrap().unwrap_err();
        assert!(
            msg.starts_with("1 replays imported\njunk.rpy not imported"),
            "{msg}"
        );
        let dst = root.join("save/replay/mine.rpy");
        assert!(dst.is_file() && src.is_file());
        m.reload_replays();
        assert_eq!(m.rp.list.as_ref().unwrap().len(), 1);
        assert!(matches!(m.watch(&dst), super::Action::Play));
        assert_eq!(m.settings().replay.as_deref(), Some(dst.to_str().unwrap()));
        assert!(m.changes().iter().all(|(k, _)| *k != "replay"));
    }

    #[test]
    fn digits() {
        assert_eq!(super::group_digits(0), "0");
        assert_eq!(super::group_digits(999), "999");
        assert_eq!(super::group_digits(1_234_567_890), "1,234,567,890");
    }
}
