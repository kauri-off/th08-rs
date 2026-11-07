//! Launcher Stats tab: play statistics from stats.jsonl and the score.dat viewer.

use super::Model;
use super::screen::{caption, note};
use crate::i18n::{self, Lang};
use crate::replays::{rank_label, shot_label, stage_label};
use crate::scoreview::{self, SHOTS, STAGE_CLEARED};
use crate::stats::{self, EndReason, Mode, Summary};
use crate::theme;

pub(super) struct StatsTab {
    // None = not read since the tab opened.
    data: Option<Data>,
    // Spell-card history: shot shown (12 = all), attempted cards only.
    shot: usize,
    all_cards: bool,
}

impl Default for StatsTab {
    fn default() -> Self {
        StatsTab {
            data: None,
            shot: SHOTS,
            all_cards: false,
        }
    }
}

struct Data {
    summary: Summary,
    view: Result<scoreview::View, String>,
    // Card names for the stats list: translated / game data / score.dat, by card id.
    names: Vec<String>,
}

impl Model {
    pub(super) fn forget_stats(&mut self) {
        self.st.data = None;
    }

    fn load_stats(&self) -> Data {
        let s = self.settings();
        let save = s.storage_dir();
        let summary = save
            .as_deref()
            .map(|d| stats::summarize(&stats::read(&stats::path(d))))
            .unwrap_or_default();
        let ecl = s
            .game_dir
            .as_deref()
            .map(scoreview::ecl_names)
            .unwrap_or_default();
        let pack = crate::packs::select(&s);
        let db = pack.as_ref().map(|m| &m.text.db);
        let view = match save.as_deref().map(scoreview::read) {
            Some(Ok(sd)) => Ok(scoreview::view(&sd, db, &ecl)),
            Some(Err(e)) => Err(e),
            None => Err(String::new()),
        };
        let names = match &view {
            Ok(v) => v.cards.iter().map(|c| c.name.clone()).collect(),
            Err(_) => (0..th08_core::scoredat::CARDS)
                .map(|i| {
                    let id = u16::try_from(i).unwrap_or(u16::MAX);
                    scoreview::card_name(id, db, &ecl, b"").unwrap_or_default()
                })
                .collect(),
        };
        Data {
            summary,
            view,
            names,
        }
    }

    pub(super) fn stats_ui(&mut self, ui: &mut egui::Ui, x: f32, w: f32, lang: Lang) {
        if self.st.data.is_none() {
            self.st.data = Some(self.load_stats());
        }
        let on = self.settings().stats;
        let save = self.settings().storage_dir();
        if self.action_row(ui, x, w, "rp_refresh", lang) {
            self.st.data = None;
            self.st.data = Some(self.load_stats());
        }
        if !on {
            note(
                ui,
                x,
                w,
                i18n::ui(lang, "st_off"),
                theme::SMALL,
                theme::WARN,
            );
        }
        let Some(d) = self.st.data.take() else {
            return;
        };
        if save.is_none() {
            note(
                ui,
                x,
                w,
                i18n::ui(lang, "st_no_save"),
                theme::SMALL,
                theme::HELP,
            );
        }
        play_ui(ui, x, w, lang, &d);
        caption(ui, x, w, i18n::ui(lang, "st_scoredat"));
        match &d.view {
            Ok(v) => self.scoredat_ui(ui, x, w, lang, v, &d.names),
            Err(e) if e.is_empty() => {}
            Err(e) => note(
                ui,
                x,
                w,
                &i18n::ui_fmt(lang, "st_sd_error", &[("err", e)]),
                theme::SMALL,
                theme::DANGER,
            ),
        }
        self.st.data = Some(d);
    }

    fn scoredat_ui(
        &mut self,
        ui: &mut egui::Ui,
        x: f32,
        w: f32,
        lang: Lang,
        v: &scoreview::View,
        names: &[String],
    ) {
        sub(ui, x, w, i18n::ui(lang, "st_scores"));
        if v.scores.is_empty() {
            note(
                ui,
                x,
                w,
                i18n::ui(lang, "st_empty"),
                theme::SMALL,
                theme::HELP,
            );
        }
        for ((diff, shot), list) in &v.scores {
            note(
                ui,
                x,
                w,
                &format!("{} · {}", rank_label(lang, *diff), shot_label(lang, *shot)),
                theme::BODY,
                theme::ACCENT,
            );
            let rows: Vec<Vec<String>> = list
                .iter()
                .enumerate()
                .map(|(i, e)| {
                    vec![
                        format!("{}.", i + 1),
                        if e.name.is_empty() {
                            "-".into()
                        } else {
                            e.name.clone()
                        },
                        group_digits(e.score),
                        if e.stage == STAGE_CLEARED {
                            i18n::ui(lang, "st_cleared").to_string()
                        } else {
                            stage_label(lang, Some(usize::from(e.stage)))
                        },
                        e.date.clone(),
                    ]
                })
                .collect();
            table(ui, x, w, &[], &rows);
        }
        sub(ui, x, w, i18n::ui(lang, "st_clears"));
        note(
            ui,
            x,
            w,
            i18n::ui(lang, "st_clears_hint"),
            theme::SMALL,
            theme::HELP,
        );
        let mut head = vec![String::new()];
        head.extend((0..5).map(|d| rank_label(lang, d)));
        let rows: Vec<Vec<String>> = v
            .clears
            .iter()
            .enumerate()
            .map(|(shot, row)| {
                let mut r = vec![shot_label(lang, u8::try_from(shot).unwrap_or(0))];
                r.extend(row.iter().map(|c| c.label().to_string()));
                r
            })
            .collect();
        table(ui, x, w, &head, &rows);
        sub(ui, x, w, i18n::ui(lang, "st_cards"));
        let label = |k: usize| {
            if k == SHOTS {
                i18n::ui(lang, "st_all").to_string()
            } else {
                shot_label(lang, u8::try_from(k).unwrap_or(0))
            }
        };
        // The list runs All, then the teams.
        let order: Vec<usize> = std::iter::once(SHOTS).chain(0..SHOTS).collect();
        let words = order.iter().map(|&k| label(k)).collect();
        let at = order.iter().position(|&k| k == self.st.shot).unwrap_or(0);
        if let Some(i) = self.choice_row(ui, x, w, "st_shot", lang, words, at) {
            self.st.shot = order[i];
        }
        let words = vec![
            i18n::ui(lang, "word_off").to_string(),
            i18n::ui(lang, "word_on").to_string(),
        ];
        if let Some(i) = self.choice_row(
            ui,
            x,
            w,
            "st_cards_all",
            lang,
            words,
            usize::from(self.st.all_cards),
        ) {
            self.st.all_cards = i == 1;
        }
        let k = self.st.shot;
        let head: Vec<String> = ["st_no", "st_card", "st_story", "st_practice"]
            .iter()
            .map(|h| i18n::ui(lang, h).to_string())
            .collect();
        let rows: Vec<Vec<String>> = v
            .cards
            .iter()
            .filter(|c| self.st.all_cards || c.attempted())
            .map(|c| {
                vec![
                    format!("{:03}", c.id + 1),
                    card_label(lang, c.id, names),
                    ratio(c.story[k]),
                    ratio(c.practice[k]),
                ]
            })
            .collect();
        table(ui, x, w, &head, &rows);
    }
}

// A sub-caption inside a section: the serif word in the accent colour.
fn sub(ui: &mut egui::Ui, x: f32, w: f32, text: &str) {
    let g = ui.painter().layout(
        text.to_string(),
        theme::serif(theme::BODY + 2.0),
        theme::TEXT_HI,
        w,
    );
    let (r, _) = ui.allocate_exact_size(
        egui::vec2(ui.max_rect().width(), g.size().y + 18.0),
        egui::Sense::hover(),
    );
    theme::outlined(
        ui.painter(),
        egui::pos2(x, r.min.y + 12.0),
        g,
        theme::TEXT_HI,
        false,
    );
}

// A table in the text column: columns as wide as their widest cell; when they do not fit `w`, each row as one
// wrapped line ("a · b · c") instead.
fn table(ui: &mut egui::Ui, x: f32, w: f32, head: &[String], rows: &[Vec<String>]) {
    let p = ui.painter().clone();
    let font = theme::sans(theme::BODY - 1.0);
    let hfont = theme::sans(theme::SMALL);
    let gap = 18.0;
    let cols = rows.first().map_or(head.len(), Vec::len).max(head.len());
    let mut cw = vec![0.0f32; cols];
    let lay = |t: &str, f: &egui::FontId, c| p.layout_no_wrap(t.to_string(), f.clone(), c);
    for (i, h) in head.iter().enumerate() {
        cw[i] = cw[i].max(lay(h, &hfont, theme::HELP).size().x);
    }
    for r in rows {
        for (i, c) in r.iter().enumerate() {
            cw[i] = cw[i].max(lay(c, &font, theme::TEXT).size().x);
        }
    }
    let total: f32 = cw.iter().sum::<f32>() + gap * (cols.max(1) - 1) as f32;
    let line = theme::BODY * 1.6;
    if total <= w {
        let mut draw = |cells: &[String], f: &egui::FontId, c: egui::Color32| {
            let (r, _) = ui.allocate_exact_size(
                egui::vec2(ui.max_rect().width(), line),
                egui::Sense::hover(),
            );
            let mut cx = x;
            for (i, t) in cells.iter().enumerate() {
                let g = lay(t, f, c);
                theme::shadowed(&p, egui::pos2(cx, r.center().y - g.size().y / 2.0), g, c);
                cx += cw[i] + gap;
            }
        };
        if head.iter().any(|h| !h.is_empty()) {
            draw(head, &hfont, theme::HELP);
        }
        for r in rows {
            draw(r, &font, theme::TEXT);
        }
    } else {
        for r in rows {
            let text = r
                .iter()
                .filter(|c| !c.is_empty())
                .cloned()
                .collect::<Vec<_>>()
                .join(" · ");
            note(ui, x, w, &text, theme::BODY - 1.0, theme::TEXT);
        }
    }
    ui.add_space(6.0);
}

fn play_ui(ui: &mut egui::Ui, x: f32, w: f32, lang: Lang, d: &Data) {
    let s = &d.summary;
    let head = |keys: &[&str]| -> Vec<String> {
        keys.iter().map(|k| i18n::ui(lang, k).to_string()).collect()
    };
    sub(
        ui,
        x,
        w,
        &i18n::ui_fmt(
            lang,
            "st_playtime",
            &[
                ("time", &stats::format_time(s.frames)),
                ("runs", &s.runs.to_string()),
            ],
        ),
    );
    if s.runs == 0 {
        note(
            ui,
            x,
            w,
            i18n::ui(lang, "st_none"),
            theme::SMALL,
            theme::HELP,
        );
        return;
    }
    caption(ui, x, w, i18n::ui(lang, "st_by_pair"));
    let rows: Vec<Vec<String>> = s
        .by_pair
        .iter()
        .map(|((shot, diff), (n, best))| {
            vec![
                shot_label(lang, *shot),
                rank_label(lang, *diff),
                n.to_string(),
                group_digits(*best),
            ]
        })
        .collect();
    table(
        ui,
        x,
        w,
        &head(&["st_shot", "st_diff", "st_runs", "st_best"]),
        &rows,
    );
    caption(ui, x, w, i18n::ui(lang, "st_per_stage"));
    let rows: Vec<Vec<String>> = s
        .per_stage
        .iter()
        .enumerate()
        .map(|(i, (deaths, bombs))| {
            vec![
                stage_label(lang, Some(i)),
                deaths.to_string(),
                bombs.to_string(),
            ]
        })
        .collect();
    table(
        ui,
        x,
        w,
        &head(&["st_stage", "st_deaths", "st_bombs"]),
        &rows,
    );
    caption(ui, x, w, i18n::ui(lang, "st_failed"));
    if s.failed.is_empty() {
        note(
            ui,
            x,
            w,
            i18n::ui(lang, "st_empty"),
            theme::SMALL,
            theme::HELP,
        );
    } else {
        let rows: Vec<Vec<String>> = s
            .failed
            .iter()
            .map(|c| {
                let name = d
                    .names
                    .get(usize::from(c.card))
                    .filter(|n| !n.is_empty())
                    .cloned()
                    .unwrap_or_else(|| c.name.clone());
                let name = if name.is_empty() {
                    card_no(lang, c.card)
                } else {
                    name
                };
                vec![
                    format!("{:03}", c.card + 1),
                    name,
                    (c.attempts - c.captures).to_string(),
                    ratio((c.captures, c.attempts)),
                ]
            })
            .collect();
        table(
            ui,
            x,
            w,
            &head(&["st_no", "st_card", "st_failures", "st_capture_rate"]),
            &rows,
        );
    }
    caption(ui, x, w, i18n::ui(lang, "st_last"));
    let rows: Vec<Vec<String>> = s
        .last
        .iter()
        .map(|r| {
            vec![
                date(r.t),
                mode_label(lang, r.mode).to_string(),
                format!(
                    "{} · {}",
                    shot_label(lang, r.shot),
                    rank_label(lang, r.diff)
                ),
                stage_label(lang, Some(usize::from(r.stage))),
                group_digits(r.score),
                stats::format_time(r.frames),
                end_label(lang, r.reason).to_string(),
            ]
        })
        .collect();
    table(
        ui,
        x,
        w,
        &head(&[
            "st_date", "st_mode", "st_shot", "st_stage", "st_score", "st_time", "st_end",
        ]),
        &rows,
    );
}

fn card_no(lang: Lang, card: u16) -> String {
    i18n::ui_fmt(
        lang,
        "st_card_no",
        &[("n", &(u32::from(card) + 1).to_string())],
    )
}

fn card_label(lang: Lang, card: u16, names: &[String]) -> String {
    names
        .get(usize::from(card))
        .filter(|n| !n.is_empty())
        .cloned()
        .unwrap_or_else(|| card_no(lang, card))
}

fn ratio((captures, attempts): (u32, u32)) -> String {
    format!("{captures} / {attempts}")
}

pub(super) fn mode_label(lang: Lang, m: Option<Mode>) -> &'static str {
    i18n::ui(
        lang,
        match m {
            Some(Mode::Story) | None => "st_mode_story",
            Some(Mode::Practice) => "st_mode_practice",
            Some(Mode::Spell) => "st_mode_spell",
        },
    )
}

fn end_label(lang: Lang, r: Option<EndReason>) -> &'static str {
    i18n::ui(
        lang,
        match r {
            Some(EndReason::Menu) => "st_end_menu",
            Some(EndReason::Ending) => "st_end_ending",
            Some(EndReason::Retry) => "st_end_retry",
            Some(EndReason::Quit) => "st_end_quit",
            None => "st_end_none",
        },
    )
}

fn date(t: u64) -> String {
    use chrono::TimeZone;
    i64::try_from(t)
        .ok()
        .and_then(|t| chrono::Local.timestamp_opt(t, 0).single())
        .map_or_else(|| "-".into(), |d| d.format("%Y-%m-%d %H:%M").to_string())
}

fn group_digits(n: u64) -> String {
    super::replay_tab::group_digits(n)
}
