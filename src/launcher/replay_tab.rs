//! Launcher Replays tab: the replay folder's list, Watch, import (file picker, drag and drop), export, video export and delete.

use super::{Action, Model, Value, screen};
use crate::i18n::{self, Lang};
use crate::replays::{self, Entry, Sort};
use crate::theme::{self, Kind};
use crate::videoexport::{self, Progress};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, PartialEq, Eq)]
enum RowAct {
    Watch,
    Video,
    Export,
    Delete,
}

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

    pub(super) fn replays_ui(
        &mut self,
        ui: &mut egui::Ui,
        x: f32,
        w: f32,
        lang: Lang,
    ) -> Option<Action> {
        let mut act = None;
        self.replay_tools(ui, x, w, lang);
        match &self.rp.status {
            Some(Ok(m)) => screen::note(ui, x, w, m, theme::SMALL, theme::NOTE),
            Some(Err(e)) => screen::note(ui, x, w, e, theme::SMALL, theme::DANGER),
            None => {}
        }
        self.video_progress_ui(ui, x, w, lang);
        if self.rp.list.is_none() {
            self.reload_replays();
        }
        let list = self.rp.list.take().unwrap_or_default();
        let dir = self
            .replay_dir()
            .map_or_else(String::new, |d| screen::tilde(&d.display().to_string()));
        screen::caption(
            ui,
            x,
            w,
            &format!("{} · {}", i18n::ui(lang, "tab_replays"), list.len()),
        );
        if list.is_empty() {
            screen::note(
                ui,
                x,
                w,
                &i18n::ui_fmt(lang, "rp_none", &[("dir", &dir)]),
                theme::SMALL,
                theme::HELP,
            );
        }
        let can_play = self.can_play();
        let mut video = None;
        for (i, e) in list.iter().enumerate() {
            match self.replay_row(ui, x, w, i, e, lang, can_play) {
                Some(RowAct::Watch) => act = Some(e.path.clone()),
                Some(RowAct::Video) => video = Some(e.path.clone()),
                Some(RowAct::Export) => self.export_replay(&e.path, lang),
                Some(RowAct::Delete) => self.rp.confirm = Some(e.path.clone()),
                None => {}
            }
        }
        self.rp.list = Some(list);
        if let Some(p) = video {
            self.ask_video(p);
        }
        self.confirm_ui(ui, lang);
        self.video_ask_ui(ui, lang);
        act.map(|p| self.watch(&p))
    }

    // One row: "Import replays…" and "Refresh" as words, the sort order as words at the right. Z imports, ← → sort.
    fn replay_tools(&mut self, ui: &mut egui::Ui, x: f32, w: f32, lang: Lang) {
        let size = 19.5;
        let (r, _) = ui.allocate_exact_size(
            egui::vec2(ui.max_rect().width(), 47.0),
            egui::Sense::hover(),
        );
        let row = egui::Rect::from_x_y_ranges(x..=x + w, r.y_range());
        let t = screen::Target::Choice("rp_sort");
        let band = egui::Rect::from_x_y_ranges(
            ui.max_rect().min.x + 2.0..=ui.max_rect().max.x,
            row.y_range(),
        );
        self.mark_target(ui, band, t);
        let lit = self.shows(t);
        let p = ui.painter().clone();
        let mut wx = x;
        let mut picked = None;
        for (i, k) in ["rp_import", "rp_refresh"].into_iter().enumerate() {
            let g = p.layout_no_wrap(
                theme::latin_ellipsis(i18n::ui(lang, k)).into_owned(),
                theme::serif(size),
                theme::TEXT,
            );
            let wr = egui::Rect::from_min_size(
                egui::pos2(wx, row.center().y - g.size().y / 2.0),
                g.size(),
            );
            let resp = ui.interact(
                wr.expand(6.0),
                egui::Id::unique(("replay tool", k)),
                egui::Sense::click(),
            );
            let c = if resp.hovered() || (lit && i == 0) {
                theme::TEXT_HI
            } else {
                theme::TEXT
            };
            theme::outlined(&p, wr.min, g, c, false);
            if resp.clicked() {
                picked = Some(i);
            }
            wx = wr.max.x + size * 1.6;
        }
        let words = vec![
            i18n::ui(lang, "rp_sort_date").to_string(),
            i18n::ui(lang, "rp_sort_score").to_string(),
        ];
        let sel = usize::from(self.rp.sort == Sort::Score);
        let st = theme::CtlStyle {
            size,
            num: size,
            cells: false,
            right: true,
            indent: 0.0,
            track: 0.0,
            numw: 0.0,
            gap: size * 1.1,
            cell_w: 0.0,
            cell_h: 48.0,
        };
        let cr = egui::Rect::from_x_y_ranges(wx..=row.max.x, row.y_range());
        let ctl = theme::Ctl::Choices {
            words,
            sel: Some(sel),
        };
        let mut sort =
            match theme::value_ctl(ui, &p, egui::Id::unique("replay sort"), cr, &ctl, lit, &st) {
                Some(theme::CtlOut::Pick(i)) => Some(i),
                _ => None,
            };
        if let Some((_, d)) = self.take_pending(t) {
            if d == 0 {
                picked = Some(0);
            } else {
                sort = Some(usize::from(d > 0));
            }
        }
        match picked {
            Some(0) => self.pick_replays(lang),
            Some(_) => self.rp.list = None,
            None => {}
        }
        if let Some(i) = sort.filter(|&i| i != sel) {
            self.rp.sort = if i == 1 { Sort::Score } else { Sort::Date };
            if let Some(v) = &mut self.rp.list {
                replays::sort(v, self.rp.sort);
            }
        }
    }

    // One replay: name and score, team · difficulty · stage, the file line, and its actions as words.
    #[expect(clippy::too_many_arguments, reason = "one painted row")]
    #[expect(clippy::too_many_lines, reason = "one painted row")]
    fn replay_row(
        &mut self,
        ui: &mut egui::Ui,
        x: f32,
        w: f32,
        i: usize,
        e: &Entry,
        lang: Lang,
        can_play: bool,
    ) -> Option<RowAct> {
        let p = ui.painter().clone();
        let t = screen::Target::Replay(i);
        let lit = self.shows(t);
        let name_c = if lit { theme::TEXT_HI } else { theme::TEXT };
        let (title, meta, meta_c) = match &e.info {
            Ok(inf) => {
                let mut m = format!(
                    "{} · {} · {}",
                    replays::shot_label(lang, inf.shot),
                    replays::rank_label(lang, inf.difficulty),
                    replays::stage_label(lang, inf.stage)
                );
                if inf.practice {
                    m = format!("{m} · {}", i18n::ui(lang, "rp_practice"));
                }
                let name = if inf.name.is_empty() { "-" } else { &inf.name };
                (name.to_string(), m, theme::HELP_HI)
            }
            Err(err) => (
                e.file.clone(),
                i18n::ui_fmt(lang, "rp_damaged", &[("err", err)]),
                theme::DANGER,
            ),
        };
        let score = e
            .info
            .as_ref()
            .map(|inf| p.layout_no_wrap(group_digits(inf.score), theme::sans(theme::BODY), name_c))
            .ok();
        let sw = score.as_ref().map_or(0.0, |g| g.size().x + 16.0);
        let tg = p.layout(
            title,
            theme::serif(theme::TITLE),
            name_c,
            (w - sw).max(40.0),
        );
        let mg = p.layout(meta, theme::sans(theme::SMALL), meta_c, w);
        let detail = match &e.info {
            Ok(inf) => i18n::ui_fmt(
                lang,
                "rp_detail",
                &[
                    ("file", &e.file),
                    ("date", &inf.date),
                    ("slow", &format!("{:.2}", inf.slowdown)),
                ],
            ),
            Err(_) => String::new(),
        };
        let dg = (!detail.is_empty()).then(|| {
            let mut job = egui::text::LayoutJob::simple(
                detail,
                theme::sans(theme::SMALL - 1.0),
                theme::NOTE,
                w,
            );
            job.wrap.max_rows = 1;
            job.wrap.break_anywhere = true;
            p.fonts_mut(|f| f.layout_job(job))
        });
        let mut acts = Vec::new();
        if e.info.is_ok() {
            acts.push((RowAct::Watch, "rp_watch", can_play));
            acts.push((
                RowAct::Video,
                "rp_video",
                can_play && self.rp.video.is_none(),
            ));
            acts.push((RowAct::Export, "rp_export", true));
        }
        acts.push((RowAct::Delete, "rp_delete", true));
        let words: Vec<_> = acts
            .iter()
            .map(|(_, k, _)| {
                let text = if *k == "rp_delete" {
                    format!("{}…", i18n::ui(lang, k))
                } else {
                    i18n::ui(lang, k).to_string()
                };
                let text = theme::latin_ellipsis(&text).into_owned();
                p.layout_no_wrap(text, theme::serif(theme::BODY), theme::TEXT)
            })
            .collect();
        // Action words in lines of `w`, at least a word's height apart.
        let gap = theme::BODY * 1.4;
        let mut pos = Vec::new();
        let (mut ax, mut line) = (0.0, 0usize);
        for g in &words {
            if ax > 0.0 && ax + g.size().x > w {
                ax = 0.0;
                line += 1;
            }
            pos.push((ax, line));
            ax += g.size().x + gap;
        }
        let lh = theme::BODY * 1.9;
        let h = 10.0
            + tg.size().y
            + 2.0
            + mg.size().y
            + dg.as_ref().map_or(0.0, |g| g.size().y + 2.0)
            + 4.0
            + (line + 1) as f32 * lh
            + 6.0;
        let (r, _) =
            ui.allocate_exact_size(egui::vec2(ui.max_rect().width(), h), egui::Sense::hover());
        let row = egui::Rect::from_x_y_ranges(x..=x + w, r.y_range());
        let (_, go) = self.row_target(ui, row, t);
        let mut y = row.min.y + 10.0;
        let th = tg.size().y;
        theme::outlined(&p, egui::pos2(x, y), tg, name_c, lit);
        if let Some(g) = score {
            let gx = x + w - g.size().x;
            theme::shadowed(&p, egui::pos2(gx, y + (th - g.size().y) / 2.0), g, name_c);
        }
        y += th + 2.0;
        let mh = mg.size().y;
        theme::shadowed(&p, egui::pos2(x, y), mg, meta_c);
        y += mh;
        if let Some(g) = dg {
            let dh = g.size().y;
            theme::shadowed(&p, egui::pos2(x, y + 2.0), g, theme::NOTE);
            y += dh + 2.0;
        }
        y += 4.0;
        let mut out = None;
        for (((a, k, on), g), (ax, line)) in acts.iter().zip(words).zip(pos) {
            let gw = g.size();
            let wr = egui::Rect::from_min_size(
                egui::pos2(x + ax, y + line as f32 * lh),
                egui::vec2(gw.x, lh),
            );
            let resp = ui.interact(
                wr.expand2(egui::vec2(gap / 2.0 - 1.0, 0.0)),
                egui::Id::unique(("replay act", i, *k)),
                if *on {
                    egui::Sense::click()
                } else {
                    egui::Sense::hover()
                },
            );
            let c = match (*on, *a) {
                (false, _) => theme::DISABLED,
                _ if resp.hovered() => theme::TEXT_HI,
                (true, RowAct::Delete) => theme::DANGER,
                (true, RowAct::Watch) => theme::TEXT,
                _ => theme::HELP_HI,
            };
            theme::outlined(
                &p,
                egui::pos2(wr.min.x, wr.center().y - gw.y / 2.0),
                g,
                c,
                false,
            );
            if *on && resp.clicked() {
                out = Some(*a);
            }
        }
        if out.is_none() && go && can_play && e.info.is_ok() {
            out = Some(RowAct::Watch);
        }
        out
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
                    .size(theme::TITLE)
                    .strong(),
            );
            ui.label(theme::hint(i18n::ui(lang, "rp_video_hint")));
            ui.add_space(theme::M);
            let row = |ui: &mut egui::Ui, label: &str, words: Vec<String>, sel: usize| {
                let (r, _) = ui.allocate_exact_size(egui::vec2(420.0, 40.0), egui::Sense::hover());
                let p = ui.painter().clone();
                let lg = p.layout_no_wrap(
                    label.to_string(),
                    theme::serif(theme::BODY + 1.0),
                    theme::TEXT,
                );
                theme::outlined(
                    &p,
                    egui::pos2(r.min.x, r.center().y - lg.size().y / 2.0),
                    lg,
                    theme::TEXT,
                    false,
                );
                let st = theme::CtlStyle {
                    size: theme::BODY + 1.0,
                    num: theme::BODY,
                    cells: false,
                    right: false,
                    indent: 0.0,
                    track: 0.0,
                    numw: 0.0,
                    gap: theme::BODY * 1.2,
                    cell_w: 0.0,
                    cell_h: 48.0,
                };
                let cr = egui::Rect::from_x_y_ranges(r.min.x + 170.0..=r.max.x, r.y_range());
                match theme::value_ctl(
                    ui,
                    &p,
                    egui::Id::unique(("video ask", label.to_string())),
                    cr,
                    &theme::Ctl::Choices {
                        words,
                        sel: Some(sel),
                    },
                    false,
                    &st,
                ) {
                    Some(theme::CtlOut::Pick(i)) => Some(i),
                    _ => None,
                }
            };
            let off_on = || {
                vec![
                    i18n::ui(lang, "word_off").to_string(),
                    i18n::ui(lang, "word_on").to_string(),
                ]
            };
            let scales = (1..=3u32)
                .map(|k| format!("{}×{}", 640 * k, 480 * k))
                .collect();
            if let Some(i) = row(
                ui,
                i18n::ui(lang, "rp_video_scale"),
                scales,
                ask.scale as usize - 1,
            ) {
                ask.scale = i as u32 + 1;
            }
            if let Some(i) = row(
                ui,
                i18n::ui(lang, "rp_video_inputs"),
                off_on(),
                usize::from(ask.inputs),
            ) {
                ask.inputs = i == 1;
            }
            if let Some(i) = row(
                ui,
                i18n::ui(lang, "rp_video_hitboxes"),
                off_on(),
                usize::from(ask.hitboxes),
            ) {
                ask.hitboxes = i == 1;
            }
            ui.add_space(theme::M);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = theme::XL;
                go = ui
                    .add(theme::button(
                        i18n::ui(lang, "rp_video_save"),
                        Kind::Primary,
                    ))
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
    fn video_progress_ui(&mut self, ui: &mut egui::Ui, x: f32, w: f32, lang: Lang) {
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
        let total = p.total.max(p.frames).max(1);
        let text = i18n::ui_fmt(
            lang,
            "rp_video_running",
            &[
                ("file", &job.file),
                ("done", &p.frames.to_string()),
                ("total", &total.to_string()),
            ],
        );
        let cancelling = job.cancel.load(Ordering::Relaxed);
        screen::note(ui, x, w, &text, theme::SMALL, theme::HELP_HI);
        let (r, _) = ui.allocate_exact_size(
            egui::vec2(ui.max_rect().width(), 14.0),
            egui::Sense::hover(),
        );
        theme::track(
            ui.painter(),
            egui::Rangef::new(x, x + w),
            r.center().y,
            p.frames as f32 / total as f32,
            true,
        );
        if !cancelling
            && self.action_row(ui, x, w, "cancel", lang)
            && let Some(job) = &self.rp.video
        {
            job.cancel.store(true, Ordering::Relaxed);
        }
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
            ui.add_space(theme::M);
            ui.horizontal(|ui| {
                if ui
                    .add(theme::button(i18n::ui(lang, "rp_delete"), Kind::Danger))
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
