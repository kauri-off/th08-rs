//! Launcher Stats tab: play statistics from stats.jsonl and the score.dat viewer.

use super::Model;
use crate::i18n::{self, Lang};
use crate::replays::{rank_label, shot_label, stage_label};
use crate::scoreview::{self, SHOTS, STAGE_CLEARED};
use crate::stats::{self, EndReason, Mode, Summary};

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

    pub(super) fn stats_ui(&mut self, ui: &mut egui::Ui, lang: Lang) {
        if self.st.data.is_none() {
            self.st.data = Some(self.load_stats());
        }
        let on = self.settings().stats;
        let save = self.settings().storage_dir();
        ui.horizontal_wrapped(|ui| {
            if ui.button(i18n::ui(lang, "rp_refresh")).clicked() {
                self.st.data = None;
            }
        });
        if !on {
            ui.colored_label(ui.visuals().warn_fg_color, i18n::ui(lang, "st_off"));
        }
        let Some(d) = self.st.data.take() else {
            return;
        };
        if save.is_none() {
            ui.label(i18n::ui(lang, "st_no_save"));
        }
        play_ui(ui, lang, &d);
        ui.add_space(8.0);
        ui.heading(i18n::ui(lang, "st_scoredat"));
        match &d.view {
            Ok(v) => self.scoredat_ui(ui, lang, v, &d.names),
            Err(e) if e.is_empty() => {}
            Err(e) => {
                ui.label(i18n::ui_fmt(lang, "st_sd_error", &[("err", e)]));
            }
        }
        self.st.data = Some(d);
    }

    fn scoredat_ui(
        &mut self,
        ui: &mut egui::Ui,
        lang: Lang,
        v: &scoreview::View,
        names: &[String],
    ) {
        egui::CollapsingHeader::new(i18n::ui(lang, "st_scores"))
            .default_open(true)
            .show(ui, |ui| {
                if v.scores.is_empty() {
                    ui.label(i18n::ui(lang, "st_empty"));
                }
                for ((diff, shot), list) in &v.scores {
                    ui.label(
                        egui::RichText::new(format!(
                            "{} · {}",
                            rank_label(lang, *diff),
                            shot_label(lang, *shot)
                        ))
                        .strong(),
                    );
                    egui::Grid::new(("st_hs", diff, shot))
                        .striped(true)
                        .spacing([18.0, 4.0])
                        .num_columns(5)
                        .show(ui, |ui| {
                            for (i, e) in list.iter().enumerate() {
                                ui.label(format!("{}.", i + 1));
                                ui.label(if e.name.is_empty() { "-" } else { &e.name });
                                ui.monospace(group_digits(e.score));
                                ui.label(if e.stage == STAGE_CLEARED {
                                    i18n::ui(lang, "st_cleared").to_string()
                                } else {
                                    stage_label(lang, Some(usize::from(e.stage)))
                                });
                                ui.label(&e.date);
                                ui.end_row();
                            }
                        });
                    ui.add_space(4.0);
                }
            });
        egui::CollapsingHeader::new(i18n::ui(lang, "st_clears"))
            .default_open(true)
            .show(ui, |ui| {
                ui.label(egui::RichText::new(i18n::ui(lang, "st_clears_hint")).weak());
                egui::Grid::new("st_clears")
                    .striped(true)
                    .spacing([18.0, 4.0])
                    .num_columns(6)
                    .show(ui, |ui| {
                        ui.label("");
                        for d in 0..5 {
                            ui.label(egui::RichText::new(rank_label(lang, d)).strong());
                        }
                        ui.end_row();
                        for (shot, row) in v.clears.iter().enumerate() {
                            ui.label(shot_label(lang, u8::try_from(shot).unwrap_or(0)));
                            for c in row {
                                ui.monospace(c.label());
                            }
                            ui.end_row();
                        }
                    });
            });
        egui::CollapsingHeader::new(i18n::ui(lang, "st_cards"))
            .default_open(true)
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(i18n::ui(lang, "st_shot"));
                    let label = |k: usize| {
                        if k == SHOTS {
                            i18n::ui(lang, "st_all").to_string()
                        } else {
                            shot_label(lang, u8::try_from(k).unwrap_or(0))
                        }
                    };
                    egui::ComboBox::from_id_salt("st_shot")
                        .selected_text(label(self.st.shot))
                        .show_ui(ui, |ui| {
                            for k in (0..=SHOTS).rev() {
                                ui.selectable_value(&mut self.st.shot, k, label(k));
                            }
                        });
                    ui.checkbox(&mut self.st.all_cards, i18n::ui(lang, "st_cards_all"));
                });
                let k = self.st.shot;
                egui::Grid::new("st_cards")
                    .striped(true)
                    .spacing([18.0, 4.0])
                    .num_columns(4)
                    .show(ui, |ui| {
                        for h in ["st_no", "st_card", "st_story", "st_practice"] {
                            ui.label(egui::RichText::new(i18n::ui(lang, h)).strong());
                        }
                        ui.end_row();
                        for c in &v.cards {
                            if !self.st.all_cards && !c.attempted() {
                                continue;
                            }
                            ui.label(format!("{:03}", c.id + 1));
                            ui.label(card_label(lang, c.id, names));
                            ui.monospace(ratio(c.story[k]));
                            ui.monospace(ratio(c.practice[k]));
                            ui.end_row();
                        }
                    });
            });
    }
}

fn play_ui(ui: &mut egui::Ui, lang: Lang, d: &Data) {
    let s = &d.summary;
    ui.heading(i18n::ui_fmt(
        lang,
        "st_playtime",
        &[
            ("time", &stats::format_time(s.frames)),
            ("runs", &s.runs.to_string()),
        ],
    ));
    if s.runs == 0 {
        ui.label(i18n::ui(lang, "st_none"));
        return;
    }
    egui::CollapsingHeader::new(i18n::ui(lang, "st_by_pair"))
        .default_open(true)
        .show(ui, |ui| {
            egui::Grid::new("st_pairs")
                .striped(true)
                .spacing([18.0, 4.0])
                .num_columns(4)
                .show(ui, |ui| {
                    for h in ["st_shot", "st_diff", "st_runs", "st_best"] {
                        ui.label(egui::RichText::new(i18n::ui(lang, h)).strong());
                    }
                    ui.end_row();
                    for ((shot, diff), (n, best)) in &s.by_pair {
                        ui.label(shot_label(lang, *shot));
                        ui.label(rank_label(lang, *diff));
                        ui.monospace(n.to_string());
                        ui.monospace(group_digits(*best));
                        ui.end_row();
                    }
                });
        });
    egui::CollapsingHeader::new(i18n::ui(lang, "st_per_stage"))
        .default_open(true)
        .show(ui, |ui| {
            egui::Grid::new("st_stages")
                .striped(true)
                .spacing([18.0, 4.0])
                .num_columns(3)
                .show(ui, |ui| {
                    for h in ["st_stage", "st_deaths", "st_bombs"] {
                        ui.label(egui::RichText::new(i18n::ui(lang, h)).strong());
                    }
                    ui.end_row();
                    for (i, (deaths, bombs)) in s.per_stage.iter().enumerate() {
                        ui.label(stage_label(lang, Some(i)));
                        ui.monospace(deaths.to_string());
                        ui.monospace(bombs.to_string());
                        ui.end_row();
                    }
                });
        });
    egui::CollapsingHeader::new(i18n::ui(lang, "st_failed"))
        .default_open(true)
        .show(ui, |ui| {
            if s.failed.is_empty() {
                ui.label(i18n::ui(lang, "st_empty"));
                return;
            }
            egui::Grid::new("st_failed")
                .striped(true)
                .spacing([18.0, 4.0])
                .num_columns(4)
                .show(ui, |ui| {
                    for h in ["st_no", "st_card", "st_failures", "st_capture_rate"] {
                        ui.label(egui::RichText::new(i18n::ui(lang, h)).strong());
                    }
                    ui.end_row();
                    for c in &s.failed {
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
                        ui.label(format!("{:03}", c.card + 1));
                        ui.label(name);
                        ui.monospace((c.attempts - c.captures).to_string());
                        ui.monospace(ratio((c.captures, c.attempts)));
                        ui.end_row();
                    }
                });
        });
    egui::CollapsingHeader::new(i18n::ui(lang, "st_last"))
        .default_open(true)
        .show(ui, |ui| {
            egui::Grid::new("st_last")
                .striped(true)
                .spacing([18.0, 4.0])
                .num_columns(7)
                .show(ui, |ui| {
                    for h in [
                        "st_date", "st_mode", "st_shot", "st_stage", "st_score", "st_time",
                        "st_end",
                    ] {
                        ui.label(egui::RichText::new(i18n::ui(lang, h)).strong());
                    }
                    ui.end_row();
                    for r in &s.last {
                        ui.label(date(r.t));
                        ui.label(mode_label(lang, r.mode));
                        ui.label(format!(
                            "{} · {}",
                            shot_label(lang, r.shot),
                            rank_label(lang, r.diff)
                        ));
                        ui.label(stage_label(lang, Some(usize::from(r.stage))));
                        ui.monospace(group_digits(r.score));
                        ui.monospace(stats::format_time(r.frames));
                        ui.label(end_label(lang, r.reason))
                            .on_hover_text(i18n::ui_fmt(
                                lang,
                                "st_run_detail",
                                &[
                                    ("deaths", &r.deaths.to_string()),
                                    ("bombs", &r.bombs.to_string()),
                                    ("continues", &r.continues.to_string()),
                                ],
                            ));
                        ui.end_row();
                    }
                });
        });
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
