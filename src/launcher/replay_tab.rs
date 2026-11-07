//! Launcher Replays tab: the replay folder's list, Watch, import (file picker, drag and drop), export, video export and delete.

use super::{Action, Model, Value};
use crate::i18n::{self, Lang};
use crate::replays::{self, Entry, Sort};
use crate::videoexport::{self, Progress};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Default)]
pub(super) struct ReplayTab {
    // None = not read since the tab opened or the folder changed.
    list: Option<Vec<Entry>>,
    sort: Sort,
    confirm: Option<PathBuf>,
    status: Option<Result<String, String>>,
    video_ask: Option<VideoAsk>,
    video: Option<VideoJob>,
}

// The video export dialog of one replay.
struct VideoAsk {
    path: PathBuf,
    scale: u32,
    inputs: bool,
    hitboxes: bool,
}

#[derive(Default)]
struct JobState {
    progress: Progress,
    done: Option<Result<videoexport::Report, String>>,
}

// A video export running on its own thread; the window only reads its state.
struct VideoJob {
    file: String,
    out: PathBuf,
    cancel: Arc<AtomicBool>,
    state: Arc<Mutex<JobState>>,
}

impl VideoJob {
    fn start(
        settings: crate::settings::Settings,
        o: videoexport::Options,
        file: String,
        repaint: impl Fn() + Send + 'static,
    ) -> Self {
        let cancel = Arc::new(AtomicBool::new(false));
        let state = Arc::new(Mutex::new(JobState::default()));
        let out = match &o.output {
            videoexport::Output::Video(p) | videoexport::Output::PngWav(p) => p.clone(),
        };
        let (c, st) = (cancel.clone(), state.clone());
        std::thread::spawn(move || {
            let mut last = std::time::Instant::now();
            let r = videoexport::export(&settings, &o, &c, |p| {
                if let Ok(mut s) = st.lock() {
                    s.progress = p;
                }
                if last.elapsed() >= std::time::Duration::from_millis(100) {
                    last = std::time::Instant::now();
                    repaint();
                }
            });
            let r = r.map_err(|e| {
                if e.is::<videoexport::Cancelled>() {
                    String::new()
                } else {
                    format!("{e:#}")
                }
            });
            if let Ok(mut s) = st.lock() {
                s.done = Some(r);
            }
            repaint();
        });
        Self {
            file,
            out,
            cancel,
            state,
        }
    }

    // A job that only shows `progress` (launcher screenshots).
    fn shown(file: String, progress: Progress) -> Self {
        Self {
            file,
            out: PathBuf::new(),
            cancel: Arc::default(),
            state: Arc::new(Mutex::new(JobState {
                progress,
                done: None,
            })),
        }
    }

    fn snapshot(&self) -> (Progress, Option<Result<videoexport::Report, String>>) {
        self.state
            .lock()
            .map(|s| (s.progress, s.done.clone()))
            .unwrap_or_default()
    }
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
        let mut video = None;
        self.video_progress_ui(ui, lang);
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
                            let v = ui
                                .add_enabled(
                                    can_play && self.rp.video.is_none(),
                                    egui::Button::new(i18n::ui(lang, "rp_video")),
                                )
                                .on_hover_text(i18n::ui(lang, "rp_video_hint"));
                            if v.clicked() {
                                video = Some(e.path.clone());
                            }
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
        if let Some(p) = video {
            self.ask_video(p);
        }
        self.confirm_ui(ui, lang);
        self.video_ask_ui(ui, lang);
        act.map(|p| self.watch(&p))
    }

    // Launcher screenshots: the export dialog of `path`, or an export of `file` shown at `done` of `total` frames.
    pub fn shot_video_ask(&mut self, path: PathBuf) {
        self.ask_video(path);
    }

    pub fn shot_video_progress(&mut self, file: String, done: u64, total: u64) {
        self.rp.video = Some(VideoJob::shown(
            file,
            Progress {
                frames: done,
                total,
                stage: 0,
            },
        ));
    }

    // The dialog starts from the game's own render scale, input display and hitbox settings.
    fn ask_video(&mut self, path: PathBuf) {
        let s = self.settings();
        let scale = match s.render_scale {
            th08_render::gfx::RenderScale::Fixed(k) => k.clamp(1, 3),
            th08_render::gfx::RenderScale::Auto => 2,
        };
        self.rp.video_ask = Some(VideoAsk {
            path,
            scale,
            inputs: s.input_display.is_some(),
            hitboxes: s.hitboxes != crate::platform::hitbox::Hitboxes::Off,
        });
    }

    fn video_ask_ui(&mut self, ui: &mut egui::Ui, lang: Lang) {
        let Some(ask) = &mut self.rp.video_ask else {
            return;
        };
        let file = file_name(&ask.path);
        let mut go = false;
        let mut close = false;
        let m = egui::Modal::new(egui::Id::unique("rp_video")).show(ui.ctx(), |ui| {
            ui.label(
                egui::RichText::new(i18n::ui_fmt(lang, "rp_video_title", &[("file", &file)]))
                    .strong(),
            );
            ui.label(egui::RichText::new(i18n::ui(lang, "rp_video_hint")).weak());
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(i18n::ui(lang, "rp_video_scale"));
                for k in 1..=3u32 {
                    ui.selectable_value(&mut ask.scale, k, format!("{}×{}", 640 * k, 480 * k));
                }
            });
            ui.checkbox(&mut ask.inputs, i18n::ui(lang, "rp_video_inputs"));
            ui.checkbox(&mut ask.hitboxes, i18n::ui(lang, "rp_video_hitboxes"));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                go = ui
                    .button(egui::RichText::new(i18n::ui(lang, "rp_video_save")).strong())
                    .clicked();
                close = ui.button(i18n::ui(lang, "cancel")).clicked();
            });
        });
        if go {
            self.start_video(ui.ctx().clone(), lang);
        } else if close || m.should_close() {
            self.rp.video_ask = None;
        }
    }

    fn start_video(&mut self, ctx: egui::Context, lang: Lang) {
        let Some(ask) = self.rp.video_ask.take() else {
            return;
        };
        let ffmpeg = PathBuf::from("ffmpeg");
        if !videoexport::ffmpeg_available(&ffmpeg) {
            self.rp.status = Some(Err(i18n::ui(lang, "rp_video_no_ffmpeg").into()));
            return;
        }
        let stem = ask
            .path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let mut dlg = rfd::FileDialog::new()
            .set_title(i18n::ui(lang, "rp_video"))
            .set_file_name(format!("{stem}.mp4"))
            .add_filter("MP4", &["mp4"])
            .add_filter("MKV", &["mkv"]);
        if let Some(d) =
            directories::UserDirs::new().and_then(|u| u.video_dir().map(Path::to_path_buf))
        {
            dlg = dlg.set_directory(d);
        }
        let Some(mut out) = dlg.save_file() else {
            return;
        };
        if !out
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("mp4") || e.eq_ignore_ascii_case("mkv"))
        {
            out.set_extension("mp4");
        }
        let mut o = videoexport::Options::new(
            videoexport::Source::File(ask.path.clone()),
            videoexport::Output::Video(out),
        );
        o.scale = ask.scale;
        let s = self.settings();
        o.inputs = ask.inputs.then(|| {
            s.input_display
                .unwrap_or(crate::platform::overlay::Corner::BottomLeft)
        });
        o.hitboxes = if ask.hitboxes {
            crate::platform::hitbox::Hitboxes::All
        } else {
            crate::platform::hitbox::Hitboxes::Off
        };
        self.rp.status = None;
        self.rp.video = Some(VideoJob::start(s, o, file_name(&ask.path), move || {
            ctx.request_repaint();
        }));
    }

    // The running export's progress and Cancel; its result becomes the tab's status line.
    fn video_progress_ui(&mut self, ui: &mut egui::Ui, lang: Lang) {
        let Some(job) = &self.rp.video else {
            return;
        };
        let (p, done) = job.snapshot();
        if let Some(r) = done {
            let path = job.out.display().to_string();
            self.rp.status = Some(match r {
                Ok(_) => Ok(i18n::ui_fmt(lang, "rp_video_done", &[("path", &path)])),
                Err(e) if e.is_empty() => Ok(i18n::ui(lang, "rp_video_cancelled").into()),
                Err(e) => Err(i18n::ui_fmt(lang, "rp_video_failed", &[("err", &e)])),
            });
            self.rp.video = None;
            return;
        }
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let total = p.total.max(p.frames).max(1);
            ui.label(i18n::ui_fmt(
                lang,
                "rp_video_running",
                &[
                    ("file", &job.file),
                    ("done", &p.frames.to_string()),
                    ("total", &total.to_string()),
                ],
            ));
            ui.horizontal(|ui| {
                let cancelling = job.cancel.load(Ordering::Relaxed);
                if ui
                    .add_enabled(!cancelling, egui::Button::new(i18n::ui(lang, "cancel")))
                    .clicked()
                {
                    job.cancel.store(true, Ordering::Relaxed);
                }
                ui.add(
                    egui::ProgressBar::new(p.frames as f32 / total as f32)
                        .show_percentage()
                        .animate(true),
                );
            });
        });
        ui.add_space(4.0);
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

fn file_name(p: &Path) -> String {
    p.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

// 1234567890 → "1,234,567,890".
pub(super) fn group_digits(n: u64) -> String {
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
    use crate::videoexport;
    use std::sync::atomic::Ordering;

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

    // The launcher's export job runs on its own thread: 60 frames of demo 0 as PNG + WAV, then a job cancelled at once.
    #[test]
    fn video_job_runs_and_cancels() {
        let Some(dir) = th08_testkit::need(crate::settings::test_game_dir(), th08_testkit::NO_DATA)
        else {
            return;
        };
        let out = std::env::temp_dir().join(format!("th08-videojob-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&out);
        let mut s = crate::settings::get().clone();
        s.game_dir = Some(dir);
        let wait = |j: &super::VideoJob| loop {
            if let (_, Some(r)) = j.snapshot() {
                return r;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        };
        let mut o = videoexport::Options::new(
            videoexport::Source::Demo(0),
            videoexport::Output::PngWav(out.clone()),
        );
        o.to = Some(59);
        let j = super::VideoJob::start(s.clone(), o.clone(), "demo".into(), || {});
        let r = wait(&j).unwrap();
        assert_eq!((r.frames, r.samples), (60, 60 * 735));
        assert_eq!(j.snapshot().0.frames, 60);
        assert!(out.join("000059.png").is_file() && !out.join("000060.png").exists());
        o.to = None;
        let j = super::VideoJob::start(s, o, "demo".into(), || {});
        j.cancel.store(true, Ordering::Relaxed);
        assert_eq!(wait(&j).unwrap_err(), "");
        let _ = std::fs::remove_dir_all(&out);
    }

    #[test]
    fn digits() {
        assert_eq!(super::group_digits(0), "0");
        assert_eq!(super::group_digits(999), "999");
        assert_eq!(super::group_digits(1_234_567_890), "1,234,567,890");
    }
}
