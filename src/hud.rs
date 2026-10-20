//! HUD g_Hud 0x160F428 (Hud_Calc / Hud_Draw): side panel, boss UI, popups, clear panel, dialogue host (12 §5, 10).

use crate::anmmgr::AnmManager;
use crate::anmvm::{self, AnmVm};
use crate::ascii::Text;
use crate::enemy::EnemyManager;
use crate::game::Ctx;
use crate::gamemgr::{self, Game};
use crate::gfx::{Blend, FULLSCREEN, Gfx, PLAYFIELD, Vertex};
use crate::msg::{self, Dialogue, Event};
use crate::player::{PlayerRef, Popup, Stats};
use crate::stage::Stage;
use crate::task::{self, Chain, Ret, Task};
use anyhow::Result;
use std::cell::RefCell;
use std::rc::Rc;

// Hud_Calc 0x4338CA prio 15, Hud_Draw 0x433927 prio 17; ANM slots front 10, loadingNN 12, stgNtxt 13, times 14 (12 §5.1).
pub const CALC_PRIO: u16 = 15;
pub const DRAW_PRIO: u16 = 17;
pub const FRONT_SLOT: usize = 10;
pub const LOADING_SLOT: usize = 12;
pub const TXT_SLOT: usize = 13;
pub const TIMES_SLOT: usize = 14;

// stgNtxt.anm name per stage index 0–8.
const TXT_NAMES: [&str; 9] = ["1", "2", "3", "4a", "4b", "5", "6", "7", "8"];
// Per-stage clear bonus (tbl 0x4C7158).
const CLEAR_BONUS: [i64; 9] = [
    1_000_000, 1_500_000, 2_000_000, 2_500_000, 2_500_000, 3_000_000, 4_000_000, 6_000_000,
    6_660_000,
];
// Difficulty line of the clear panel, difficulties 0–5 (12 §5.6).
const RANK_TEXT: [&str; 6] = [
    "Easy Rank    *0.5",
    "Normal Rank  *1.0",
    "Hard Rank    *1.2",
    "Lunatic Rank *1.5",
    "Extra Rank   *2.0",
    "Phantasm Rank*2.0",
];

// Hud_ShowBonus / Hud_ShowMessage / Hud_ShowSpellBonus popup with its age in frames (12 §5.5).
struct ActivePopup {
    kind: Popup,
    frame: u32,
}

// Stage-clear panel state (10 §9, 12 §5.6).
struct ClearPanel {
    frame: u32,
    stage: usize,
    // Stored by msg op 9 (10 §9): clear bonus, point items, graze, time.
    clear: i32,
    points: i32,
    graze: i32,
    time: i32,
    // Bonus not computed yet: Hud_Calc does it once, on the first frame after op 9 (12 §5.6).
    pending: bool,
    // Night clock in minutes (660 + 30·clock) before / after the advance (impl+0x22E04 / +0x22E08), and the animated
    // value (impl+0x22E0C).
    from: i32,
    to: i32,
    shown: i32,
    // Stage-clear trigger already fired (11 §4.1); final stages never fire it here.
    done: bool,
}

pub struct Hud {
    pub msg: Rc<RefCell<Dialogue>>,
    player: PlayerRef,
    enemies: Rc<RefCell<EnemyManager>>,
    stage: Rc<RefCell<Stage>>,
    frame: u32,
    // front.anm VMs 0–15 (impl+i·0x2A4, 12 §5.2).
    front: Vec<AnmVm>,
    // stgNtxt.anm scripts 0–3 at impl+0x2A44 (stage title, song-title card = script 3).
    title: Vec<AnmVm>,
    // ascii.anm script 0x19 at impl+0x22E14 (difficulty name), script 3 at impl+0x212C8 (stage clear).
    diff_vm: AnmVm,
    clear_vm: AnmVm,
    // times.anm night clock impl+0x2156C (ECL 179/181) and the stage-start clock impl+0x34D4 (12 §5.1).
    night_vm: AnmVm,
    start_clock_vm: AnmVm,
    raster: crate::text::Raster,
    // +0x1C boss bar alpha 0–255, ±4 per frame (12 §5.1).
    bar_alpha: i32,
    bar_shown: bool,
    // +0x30 / +0x34 target and displayed life fraction (+0.01 up, −0.02 down per frame).
    bar_target: f32,
    bar_disp: f32,
    // +0x28 boss timer value last shown; −1 = none yet.
    timer_last: i32,
    popups: Vec<ActivePopup>,
    clear: Option<ClearPanel>,
    // impl+0x22D88: stored clear-panel total (12 §5.6).
    clear_total: i32,
}

impl Hud {
    pub fn register(
        ctx: &mut Ctx,
        player: PlayerRef,
        enemies: Rc<RefCell<EnemyManager>>,
        stage: Rc<RefCell<Stage>>,
    ) -> Result<Rc<RefCell<Hud>>> {
        let h = Rc::new(RefCell::new(Hud {
            msg: Rc::new(RefCell::new(Dialogue::default())),
            player,
            enemies,
            stage,
            frame: 0,
            front: Vec::new(),
            title: Vec::new(),
            diff_vm: AnmVm::default(),
            clear_vm: AnmVm::default(),
            night_vm: AnmVm::default(),
            start_clock_vm: AnmVm::default(),
            raster: crate::text::Raster::default(),
            bar_alpha: 0,
            bar_shown: false,
            bar_target: 0.0,
            bar_disp: 0.0,
            timer_last: -1,
            popups: Vec::new(),
            clear: None,
            clear_total: 0,
        }));
        let t: task::TaskRef<Ctx> = h.clone();
        task::add(ctx, Chain::Calc, CALC_PRIO, 0, t.clone())?;
        task::add(ctx, Chain::Draw, DRAW_PRIO, 0, t)?;
        Ok(h)
    }

    // Hud_Load + Gui_LoadFaces 0x414590 + Msg_Load 0x439710 (12 §5.1, 10 §1, §3); no msg file in spell practice.
    fn load(&mut self, ctx: &mut Ctx) -> Result<()> {
        let g = &ctx.game;
        let (stage, shot) = (g.stage % 9, g.shot);
        let (gfx, ar) = (&mut ctx.gfx, &ctx.archive);
        ctx.anm.load(gfx, ar, FRONT_SLOT, "front.anm")?;
        if let Some(n) = stage_plate(stage, g) {
            set_boss_plate(&ctx.anm, gfx, n);
        }
        ctx.anm.load(gfx, ar, TIMES_SLOT, "times.anm")?;
        ctx.anm.load(
            gfx,
            ar,
            TXT_SLOT,
            &format!("stg{}txt.anm", TXT_NAMES[stage]),
        )?;
        let team = crate::sht::team(shot);
        let variant = match shot {
            0..=3 => "",
            s if s % 2 == 0 => "h",
            _ => "a",
        };
        if let Err(e) = ctx.anm.load(
            gfx,
            ar,
            LOADING_SLOT,
            &format!("loading{team:02}{variant}.anm"),
        ) {
            log::debug!("loading anm: {e:#}");
        }
        let card = g.spell_practice().then_some(g.card);
        for (k, f) in msg::face_files(stage, shot, card).into_iter().enumerate() {
            ctx.anm.unload(gfx, msg::FACE_SLOTS[k]);
            if let Some(f) = f {
                ctx.anm.load(gfx, ar, msg::FACE_SLOTS[k], &f)?;
            }
        }
        let mut d = self.msg.borrow_mut();
        if !g.spell_practice() {
            d.file = Some(msg::parse(ar.read(&msg::file_name(stage, shot))?)?);
        }
        d.info = msg::StartInfo {
            stage,
            shot,
            continues: g.stats.continues,
            replay_route: None,
            cleared_6a: g.cleared(true, 6),
            cleared_6b: g.cleared(false, 7),
            clock: 0,
        };
        drop(d);
        let anm = &mut ctx.anm;
        self.front = (0..16)
            .map(|i| {
                let mut vm = AnmVm::default();
                anm.set_script(&mut vm, FRONT_SLOT, i);
                vm
            })
            .collect();
        self.title = (0..4)
            .map(|i| {
                let mut vm = AnmVm::default();
                if !g.spell_practice() || i == 3 {
                    anm.set_script(&mut vm, TXT_SLOT, i);
                }
                vm
            })
            .collect();
        anm.set_script(&mut self.diff_vm, crate::ascii::ANM_SLOT, 0x19);
        self.diff_vm
            .set_sprite(anm, crate::ascii::difficulty_sprite(g));
        // Hud_Load 0x4390D6: the night-clock VM starts hidden with alpha 0 (12 §5.7).
        self.night_vm = AnmVm::default();
        self.night_vm.color[anmvm::A] = 0;
        // Clock sprite = clock counter stats+0x28 (12 §5.1); the clamp is a port guard.
        anm.set_script(&mut self.start_clock_vm, TIMES_SLOT, 0);
        self.start_clock_vm
            .set_sprite(anm, g.stats.clock.clamp(0, 15));
        Ok(())
    }

    // The line VM's strip of the text.anm render target: texture, row width and strip top y in texture pixels.
    fn line_texture(
        anm: &AnmManager,
        gfx: &Gfx,
        vm: &AnmVm,
    ) -> Option<(crate::gfx::TextureId, u32, u32)> {
        let sp = anm.sprite(msg::TEXT_SLOT, vm.sprite_index.max(0) as usize)?;
        let t = gfx.texture(sp.tex);
        Some((sp.tex, t.width, (sp.uv[1] * t.height as f32).round() as u32))
    }

    // Text_RenderToVm 0x4663B0 for the queued dialogue lines (10 §6).
    fn flush_texts(&mut self, ctx: &mut Ctx) {
        let mut d = self.msg.borrow_mut();
        let size = d.font_size;
        for (line, s, colour) in std::mem::take(&mut d.texts) {
            let Some((tex, w, y)) = Self::line_texture(&ctx.anm, &ctx.gfx, &d.lines[line]) else {
                continue;
            };
            let rgba = self.raster.render(&s, colour, w as usize, size);
            ctx.gfx.write_texture(tex, 0, y, w, size as u32, &rgba);
        }
    }

    fn handle_events(&mut self, ctx: &mut Ctx, events: Vec<Event>) {
        for ev in events {
            match ev {
                Event::Plate(n) => set_boss_plate(&ctx.anm, &mut ctx.gfx, n),
                Event::SpellBase(n) => self.stage.borrow_mut().spell_first = n,
                Event::Bgm(n) if n < 0 => ctx.audio.stop_bgm(),
                // Msg op 7 (10 §5.1): song-title card = stgNtxt script 3 with sprite n + 3, then BGM slot n.
                Event::Bgm(n) => {
                    let vm = &mut self.title[3];
                    ctx.anm.set_script(vm, TXT_SLOT, 3);
                    vm.set_sprite(&ctx.anm, n + 3);
                    let name = self
                        .stage
                        .borrow()
                        .std
                        .bgm
                        .get(n as usize)
                        .cloned()
                        .unwrap_or_default();
                    ctx.audio.play_bgm(&name);
                }
                Event::BgmFade => ctx.audio.fade_bgm(4.0),
                // Msg op 14 (10 §5.1, 13 §3): ScreenEffect type 4, 442 frames to white, draw prio 0x15.
                // The full-repaint counter 0x17CE8CC := 442 is moot: the port repaints the panel every frame.
                Event::Flash => crate::effect::screen(4, 442, 0xFFFFFF, 0, 0, 0x15),
                Event::StageEnd => self.stage_end(ctx),
                // Msg op 11: GM flags & ~0x60 | 0x40 (game-clear trigger).
                Event::HaltFinal => ctx.game.set_end_state(2),
                Event::Route(c) => {
                    log::info!("final route choice {c} at frame {}", ctx.frame);
                    ctx.game.set_route(c)
                }
                Event::Sfx(n) => crate::sound::play(n as i32),
            }
        }
    }

    // Msg op 9 (10 §9): stores the panel values, advances the clock, restarts the stage-clear VM.
    fn stage_end(&mut self, ctx: &mut Ctx) {
        log::info!(
            "stage clear (msg op 9) at frame {} score {}",
            ctx.frame,
            self.player.borrow().stats.score10
        );
        let g = &mut ctx.game;
        g.flags |= gamemgr::F_CLEAR_PANEL;
        let stage = g.stage % 9;
        let mut p = self.player.borrow_mut();
        let shot = p.shot_type;
        let adv = gamemgr::clock_advance(stage, p.stats.time, p.stats.time_threshold);
        // Stage-clear VM impl+0x212C8 (10 §9): stages 1–5 restart ascii.anm script 3 with sprite 0x80 + clock advance;
        // 6A/6B/Ex only clear its ip; interrupt 1 in every case.
        if stage < 6 {
            ctx.anm
                .set_script(&mut self.clear_vm, crate::ascii::ANM_SLOT, 3);
            self.clear_vm.set_sprite(&ctx.anm, 0x80 + adv);
        } else {
            self.clear_vm.script = None;
        }
        self.clear_vm.pending_interrupt = 1;
        let rng = &mut ctx.anm.rng;
        let st = &mut p.stats;
        let from = st.clock * 30 + 660;
        st.clock += adv;
        let to = st.clock * 30 + 660;
        let final_stage = stage >= 6;
        if final_stage {
            // Stages 6A/6B/Ex set stats+0x34 = −1 (10 §9).
            st.extends = -1;
        } else {
            // Youmu&Yuyuko team and solos: +1 bomb below 3 bombs, stages 1–5 only (10 §9).
            if matches!(shot, 3 | 10 | 11) && st.bombs < 3.0 {
                st.add_bombs(1.0, rng);
                crate::sound::play(crate::sound::CARDGET);
            }
        }
        self.clear = Some(ClearPanel {
            frame: 0,
            stage,
            clear: CLEAR_BONUS[stage] as i32,
            points: st.point_items,
            graze: st.graze,
            time: st.time,
            pending: true,
            from,
            to,
            shown: from,
            done: final_stage,
        });
    }

    // Bonus computation (12 §5.6), integer arithmetic; the lives-option penalty applies in every game.
    fn clear_bonus(&mut self, g: &Game) {
        let Some(c) = &mut self.clear else { return };
        if !std::mem::take(&mut c.pending) {
            return;
        }
        let mut p = self.player.borrow_mut();
        let st = &mut p.stats;
        let mut t = c
            .clear
            .wrapping_add(c.graze.wrapping_mul(50))
            .wrapping_add(c.points.wrapping_mul(5000))
            .wrapping_add(c.time.wrapping_mul(100));
        // Replays included, even practice replays (only GM bit 0 is tested).
        if c.stage >= 6 && !g.practice() {
            t = t
                .wrapping_add((st.lives as i32).wrapping_mul(2_500_000))
                .wrapping_add((st.bombs as i32).wrapping_mul(500_000));
        }
        if c.stage == 7 {
            t = t.wrapping_add((12 - st.clock).wrapping_mul(2_000_000));
        }
        t = match g.difficulty {
            0 => t / 2,
            2 => t.wrapping_mul(12) / 10,
            3 => t.wrapping_mul(15) / 10,
            4 => t.wrapping_mul(2),
            _ => t,
        };
        t = match g.cfg.lives {
            3 => t.wrapping_mul(5) / 10,
            4 => t.wrapping_mul(2) / 10,
            5 => t / 10,
            6 => t / 20,
            _ => t,
        };
        self.clear_total = t;
        for _ in 0..10 {
            st.add_score(t);
        }
        log::info!("clear bonus {t}: score {}", st.score10);
    }

    // Clock animation (12 §5.6): after 60 frames +1 minute per frame, +3 more while Shot or Skip is held.
    fn clear_calc(&mut self, g: &mut Game, inp: &crate::input::Input) {
        let Some(c) = &mut self.clear else { return };
        c.frame += 1;
        if c.frame > 60 && c.shown < c.to {
            c.shown += 1;
            if inp.down(crate::input::SHOT | crate::input::SKIP) {
                c.shown = (c.shown + 3).min(c.to);
            }
        }
        if c.frame > 60 && c.shown >= c.to && !c.done {
            c.done = true;
            if c.stage <= 5 {
                g.set_end_state(2);
            }
        }
    }

    // Boss bar bookkeeping (12 §5.1, §5.4).
    fn boss_calc(&mut self, anm: &mut AnmManager, dialogue: bool, paused: bool) {
        let e = self.enemies.borrow();
        let boss = e.boss(0).map(|i| &e.enemies[i]);
        let show = e.g.hud_boss && boss.is_some();
        if show != self.bar_shown {
            self.bar_shown = show;
            self.front[12].pending_interrupt = if show { 1 } else { 2 };
        }
        // Target fraction life/max from the enemy calc (12 §5.1); hud_life overrides max when set.
        if let Some(b) = boss {
            let max = if b.hud_life > 0 {
                b.hud_life
            } else {
                b.max_life.max(1)
            };
            self.bar_target = (b.life.max(0) as f32 / max as f32).clamp(0.0, 1.0);
        }
        // Popup_LargeStatic from Hud_DrawBoss (12 §5.4, 13 §4): re-issued every frame, "+0" without children too.
        if let Some(i) = e.boss(0)
            && !paused
            && !self.player.borrow().freeze_bullets
        {
            crate::ascii::popup_large_static(
                [2.0, 29.0],
                e.chain_count(i),
                e.enemies[i].children,
                0xFFF0F00F,
            );
        }
        drop(e);
        self.bar_alpha = if self.bar_shown {
            (self.bar_alpha + 4).min(255)
        } else {
            (self.bar_alpha - 4).max(0)
        };
        // The life-bar refresh is skipped while a dialogue runs (10 §8.2).
        if !dialogue {
            if self.bar_disp < self.bar_target {
                self.bar_disp = (self.bar_disp + 0.01).min(self.bar_target);
            } else {
                self.bar_disp = (self.bar_disp - 0.02).max(self.bar_target);
            }
        }
        let _ = anm;
    }

    // Msg_Draw 0x43542B (10 §7): portrait pairs back-to-front by pos.z, text box, lines, boss card (Draw2DSnap).
    fn draw_msg(&mut self, anm: &mut AnmManager, gfx: &mut Gfx) {
        let mut d = self.msg.borrow_mut();
        if !d.active() {
            return;
        }
        gfx.set_viewport(PLAYFIELD);
        for pair in [[0usize, 1], [2, 3]] {
            let (a, b) = (pair[0], pair[1]);
            let order = if d.faces[a].pos[2] >= d.faces[b].pos[2] {
                [a, b]
            } else {
                [b, a]
            };
            for k in order {
                if d.faces[k].script.is_some() {
                    anm.draw_mode(gfx, &mut d.faces[k], 0);
                }
            }
        }
        if d.textbox {
            let h = if d.clock.cur >= 60 {
                48.0
            } else {
                d.clock.cur as f32 * 48.0 / 60.0
            };
            // Text box (10 §7 step 2): top colour 0xD0000000, bottom 0x90000000; grows down over the first 60 clock frames.
            let (x0, x1, y0) = (PLAYFIELD.x + 16.0, PLAYFIELD.x + 368.0, 384.0);
            let (top, bot) = ([0, 0, 0, 0xD0], [0, 0, 0, 0x90]);
            let uv = [0.0, 0.0];
            let v = [
                Vertex::screen(x0, y0, 0.0, uv, top),
                Vertex::screen(x1, y0, 0.0, uv, top),
                Vertex::screen(x0, y0 + h, 0.0, uv, bot),
                Vertex::screen(x1, y0 + h, 0.0, uv, bot),
            ];
            gfx.strip(gfx.white(), Blend::Alpha, false, v);
        }
        let d = &mut *d;
        for vm in d.lines.iter_mut().chain(std::iter::once(&mut d.boss_card)) {
            if vm.script.is_some() {
                anm.draw_mode(gfx, vm, 0);
            }
        }
    }

    fn bar_quad(gfx: &mut Gfx, x0: f32, x1: f32, y0: f32, y1: f32, top: [u8; 4], bot: [u8; 4]) {
        let uv = [0.0, 0.0];
        let v = [
            Vertex::screen(x0, y0, 0.0, uv, top),
            Vertex::screen(x1, y0, 0.0, uv, top),
            Vertex::screen(x0, y1, 0.0, uv, bot),
            Vertex::screen(x1, y1, 0.0, uv, bot),
        ];
        gfx.strip(gfx.white(), Blend::Alpha, false, v);
    }

    // 0xRRGGBB with an explicit alpha → RGBA.
    fn rgb(c: u32, a: u8) -> [u8; 4] {
        [(c >> 16) as u8, (c >> 8) as u8, c as u8, a]
    }

    // Hud_DrawBoss (12 §5.4).
    fn draw_boss(&mut self, ctx: &mut Ctx) {
        if self.bar_alpha == 0 && !self.bar_shown {
            return;
        }
        let a = self.bar_alpha as u8;
        let gfx = &mut ctx.gfx;
        gfx.set_viewport(PLAYFIELD);
        let disp = self.bar_disp;
        Self::bar_quad(
            gfx,
            64.0,
            64.0 + 320.0 * disp,
            19.0,
            23.0,
            Self::rgb(0xFFFFFF, a),
            Self::rgb(0x202060, a),
        );
        let e = self.enemies.borrow();
        let max = e.boss(0).map(|i| &e.enemies[i]).map_or(1, |b| {
            if b.hud_life > 0 {
                b.hud_life
            } else {
                b.max_life.max(1)
            }
        }) as f32;
        // ECL op 158 markers: drawn when end ≠ 0 and start < displayed; bottom colour (c >> 2) & 0x3F3F3F.
        for [end, start, col] in e.g.hud_markers {
            let (end, start) = (end as f32 / max, start as f32 / max);
            if end != 0.0 && start < disp {
                let c = col as u32;
                Self::bar_quad(
                    gfx,
                    64.0 + 320.0 * start,
                    64.0 + 320.0 * end.min(disp),
                    19.0,
                    23.0,
                    Self::rgb(c, a),
                    Self::rgb((c >> 2) & 0x3F3F3F, a),
                );
            }
        }
        // Remaining-bars gauge (+0x20, ECL op 148) between x 35 and 61.
        let n = e.g.hud_bars.max(0);
        let gap = if n < 6 { 2.0 } else { 1.0 };
        for i in 0..n {
            let x0 = 35.0 + 26.0 * i as f32 / n as f32;
            let x1 = 35.0 + 26.0 * (i + 1) as f32 / n as f32 - gap;
            let c = Self::rgb(0xFFFFFF - (i as u32 * 255 / 9), a);
            Self::bar_quad(gfx, x0, x1, 19.0, 23.0, c, c);
        }
        let secs = e
            .boss(0)
            .map(|i| &e.enemies[i])
            .filter(|b| b.timeout > 0)
            .map(|b| ((b.timeout - b.boss_timer.cur) / 60).clamp(0, 99));
        drop(e);
        ctx.anm.draw_mode(&mut ctx.gfx, &mut self.front[12], 0);
        if let Some(s) = secs {
            let c = match s {
                20.. => 0xA0D0FF,
                10..=19 => 0xA080FF,
                5..=9 => 0xE080C0,
                _ => 0xFF4040,
            };
            ctx.ascii
                .push(Text::new(384.0, 16.0, format!("{s:02}")).color(Self::rgb(c, a)));
            // On a change of the shown value: < 3 se_timeout2 (0x26), < 10 se_timeout (0x1D) (12 §5.4).
            if s != self.timer_last && self.timer_last >= 0 {
                if s < 3 {
                    crate::sound::play(crate::sound::TIMEOUT2);
                } else if s < 10 {
                    crate::sound::play(crate::sound::TIMEOUT);
                }
            }
            self.timer_last = s;
        }
    }

    // Hud_DrawPopups 0x438A89 (12 §5.5): every string with the clip flag (playfield viewport); bonus / message
    // popups slide from x 416 by −312 over 30 frames.
    fn draw_popups(&mut self, ctx: &mut Ctx) {
        for p in &self.popups {
            let slide = |f: u32| 416.0 - 312.0 * (f.min(30) as f32 / 30.0);
            match p.kind {
                Popup::Bonus(n) => ctx.ascii.push(
                    Text::new(slide(p.frame), 48.0, format!(" BONUS {n:8}"))
                        .argb(0xFFFFFF80)
                        .clip(),
                ),
                Popup::Message(t, v) => {
                    let (s, small) = match t {
                        1 => ("Full Power Mode!".to_string(), false),
                        4 => (format!("Border Bonus {v:7}"), true),
                        5 => ("Spell Bonus Failed".to_string(), true),
                        _ => ("Last Spell Failed".to_string(), true),
                    };
                    let mut tx = Text::new(slide(p.frame), 168.0, s).clip();
                    if small {
                        tx = tx.scale(0.9).advance(11.0);
                    }
                    ctx.ascii.push(tx);
                }
                // "Spell Card Bonus!" at x (384 − 17·14)/2 + 32 = 105 (14 px per glyph, not the 13 px advance), y 80;
                // "+%d" 2× at y 96, x (384 − len·28)/2 + 32.
                Popup::SpellBonus(n) => {
                    let s = "Spell Card Bonus!";
                    let x = ((384 - s.len() as i32 * 14) / 2 + 32) as f32;
                    ctx.ascii
                        .push(Text::new(x, 80.0, s).argb(0xFFFF0000).clip());
                    let v = format!("+{n}");
                    let x = ((384 - v.len() as i32 * 28) / 2 + 32) as f32;
                    ctx.ascii
                        .push(Text::new(x, 96.0, v).scale(2.0).argb(0xFFFF8080).clip());
                }
            }
        }
    }

    // Hud_DrawClearPanel 0x43826B (12 §5.6): ASCII strings only, full-screen (clip flag 0); pen from (120, 96).
    fn draw_clear(&mut self, ctx: &mut Ctx) {
        let Some(c) = &self.clear else { return };
        let g = &ctx.game;
        let st = &self.player.borrow().stats;
        let q = &mut ctx.ascii;
        let mut y = 96.0;
        let line = |q: &mut crate::ascii::Queue, y: f32, s: String, col: u32| {
            q.push(Text::new(120.0, y, s).argb(col));
        };
        let title = if c.stage < 6 {
            "Stage Clear"
        } else {
            "All Clear!"
        };
        // 6B / Extra put the title at y 80; the pen stays at 96.
        let ty = if c.stage > 6 { 80.0 } else { y };
        line(q, ty, title.into(), 0xFFFFFF40);
        y += 32.0;
        line(q, y, format!("Clear = {:8}0", c.clear), 0xFFFFFFFF);
        y += 16.0;
        line(
            q,
            y,
            format!("Point = {:8}0", c.points.wrapping_mul(5000)),
            0xFFE0E0FF,
        );
        y += 16.0;
        line(
            q,
            y,
            format!("Graze = {:8}0", c.graze.wrapping_mul(50)),
            0xFFD0D0FF,
        );
        y += 16.0;
        line(
            q,
            y,
            format!("Time  = {:8}0", c.time.wrapping_mul(100)),
            0xFFD0D0FF,
        );
        // Integer percentages (h·100)/f and ((h·10000)/f) % 100, truncated.
        // SPEC-GAP: f = 0 (no counted frame) is a division by zero in 12 §5.6; the port divides by 1.
        let f = st.stage_frames.max(1) as i32;
        let pct = |h: u32| {
            let h = h as i32;
            (h.wrapping_mul(100) / f, h.wrapping_mul(10000) / f % 100)
        };
        let (a, b) = pct(st.human_frames);
        y += 32.0;
        line(q, y, format!("over-80% = {a:3}.{b:02}%"), 0xFFD0D0FF);
        let (a, b) = pct(st.youkai_frames);
        y += 16.0;
        line(q, y, format!("over 80% = {a:3}.{b:02}%"), 0xFFD0D0FF);
        if c.stage >= 6 && !g.practice() && !g.practice_replay() {
            let (lv, bm) = (
                (st.lives as i32).wrapping_mul(2_500_000),
                (st.bombs as i32).wrapping_mul(500_000),
            );
            y += 16.0;
            line(q, y, format!("Player ={lv:8}0"), 0xFFFFFF80);
            y += 16.0;
            line(q, y, format!("Bomb   = {bm:7}0"), 0xFFFFFF80);
            if c.stage == 7 {
                let m = c.to;
                y += 16.0;
                line(
                    q,
                    y,
                    format!("Last Time = {:2}:{:02}", (m / 60) % 12, m % 60),
                    0xFFFFFF80,
                );
                y += 16.0;
                line(q, y, "Night Bonus".into(), 0xFFFFFF80);
                y += 16.0;
                let night = (12 - st.clock).wrapping_mul(2_000_000);
                line(q, y, format!("        {night:8}0"), 0xFFFFFF80);
            }
        }
        y += 32.0;
        let rank = RANK_TEXT.get(g.difficulty as usize).copied().unwrap_or("");
        line(q, y, rank.into(), 0xFFFF8080);
        if g.difficulty < 4 && !g.practice() {
            y += 16.0;
            let pen = match g.cfg.lives {
                3 => Some("Player Penalty*0.5"),
                4 => Some("Player Penalty*0.2"),
                5 => Some("Player Penalty*0.1"),
                6 => Some("Player Penalty*0.05"),
                _ => None,
            };
            if let Some(t) = pen {
                line(q, y, t.into(), 0xFFFF8080);
                y += 16.0;
            }
        }
        y += 16.0;
        line(q, y, format!("Total = {:8}0", self.clear_total), 0xFFFFFFFF);
        if c.stage < 6 {
            // Prefix "PM" while minutes/60 < 12, else "AM"; hours (minutes/60) % 12.
            let clock = |m: i32| {
                format!(
                    "{}{:2}:{:02}",
                    if m / 60 < 12 { "PM" } else { "AM" },
                    (m / 60) % 12,
                    m % 60
                )
            };
            y += 40.0;
            q.push(Text::new(120.0, y, clock(c.from)).argb(0xFFDFDFDF));
            q.push(Text::new(219.0, y, ">>").argb(0xFFAFAFAF));
            q.push(Text::new(253.0, y, clock(c.shown)).argb(0xFFFF8F8F));
        }
    }

    // Hud_DrawPanel (12 §5.3), redrawn every frame; flushes and sets the full-screen viewport first.
    fn draw_panel(&mut self, ctx: &mut Ctx, st: &Stats) {
        let gfx = &mut ctx.gfx;
        gfx.set_viewport(FULLSCREEN);
        let anm = &mut ctx.anm;
        // Tiles (scripts 13–15) keep z-write on; depths 0.49 / 0.48 / 0.46 (12 §5.3).
        let spr = |anm: &AnmManager, gfx: &mut Gfx, i: usize, x: f32, y: f32, z: f32| {
            if let Some(s) = anm.sprite(FRONT_SLOT, i) {
                let [u0, v0, u1, v1] = s.uv;
                let v =
                    |x: f32, y: f32, u: f32, vv: f32| Vertex::screen(x, y, z, [u, vv], [255; 4]);
                gfx.strip(
                    s.tex,
                    Blend::Alpha,
                    true,
                    [
                        v(x, y, u0, v0),
                        v(x + s.w, y, u1, v0),
                        v(x, y + s.h, u0, v1),
                        v(x + s.w, y + s.h, u1, v1),
                    ],
                );
            }
        };
        for y in [40.0, 56.0] {
            spr(anm, gfx, 15, 480.0, y, 0.49);
        }
        for y in [88.0, 104.0, 136.0, 152.0, 168.0, 184.0] {
            spr(anm, gfx, 15, 480.0, y, 0.48);
        }
        // Always: the corner under the fps text.
        spr(anm, gfx, 15, 512.0, 464.0, 0.48);
        // Full repaint (cfg bit 12, tile VM 13 still running = first 200 frames, or counter 0x17CE8CC ≠ 0) and the
        // dirty rows are not tracked: the port clears the screen every frame, so it repaints in full every frame.
        for y in (0..480).step_by(32) {
            spr(anm, gfx, 13, 0.0, y as f32, 0.49);
        }
        for x in (416..640).step_by(32) {
            for y in (16..480).step_by(32) {
                spr(anm, gfx, 13, x as f32, y as f32, 0.49);
            }
        }
        for x in (0..640).step_by(128) {
            spr(anm, gfx, 14, x as f32, 0.0, 0.49);
            spr(anm, gfx, 14, x as f32, 464.0, 0.49);
        }
        for vm in self.front.iter_mut().take(10) {
            anm.draw_mode(gfx, vm, if vm.script_index == 1 { 2 } else { 0 });
        }
        anm.draw_mode(gfx, &mut self.diff_vm, 0);
        let q = &mut ctx.ascii;
        q.push(Text::new(
            488.0,
            40.0,
            format!("{:09}", st.hiscore.min(999_999_999)),
        ));
        q.push(Text::new(
            605.0,
            40.0,
            format!("{}", st.hiscore_cont.min(9)),
        ));
        q.push(Text::new(
            488.0,
            56.0,
            format!("{:09}", st.score_disp.min(999_999_999)),
        ));
        q.push(Text::new(605.0, 56.0, format!("{}", st.continues.min(9))));
        for (row, n, vm) in [(88.0, st.lives as i32, 10), (104.0, st.bombs as i32, 11)] {
            for i in 0..n.clamp(0, 8) {
                let v = &mut self.front[vm];
                v.pos = [488.0 + 16.0 * i as f32, row, 0.46];
                anm.draw_mode(gfx, v, 0);
            }
        }
        // Strip 14 along y = 464 for x 32..368 every frame (12 §5.3).
        for x in [32.0, 160.0, 288.0] {
            spr(anm, gfx, 14, x, 464.0, 0.49);
        }
        // Power gauge x 488 … 488 + power, y 136..152, left 0xE0E0E0FF / right 0x80E0E0FF (12 §5.3).
        let pw = st.power.trunc();
        let uv = [0.0, 0.0];
        let (l, r) = ([0xE0, 0xE0, 0xFF, 0xE0], [0xE0, 0xE0, 0xFF, 0x80]);
        gfx.strip(
            gfx.white(),
            Blend::Alpha,
            false,
            [
                // D3D coordinates (+0.5 for wgpu) and z 0.1, as in the original's draw log.
                Vertex::screen(488.5, 136.5, 0.1, uv, l),
                Vertex::screen(488.5 + pw, 136.5, 0.1, uv, r),
                Vertex::screen(488.5, 152.5, 0.1, uv, l),
                Vertex::screen(488.5 + pw, 152.5, 0.1, uv, r),
            ],
        );
        q.push(Text::new(
            488.0,
            136.0,
            if pw >= 128.0 {
                "MAX".to_string()
            } else {
                format!("{}", pw as i32)
            },
        ));
        q.push(Text::new(488.0, 152.0, format!("{}", st.graze_total)));
        // "/" at half a glyph width (+6 px) after the value (12 §5.3).
        let pt = format!("{}", st.point_items2);
        q.push(Text::new(488.0, 168.0, pt.clone()));
        let x = 488.0 + 13.0 * pt.len() as f32;
        q.push(Text::new(x, 168.0, "/"));
        q.push(Text::new(x + 6.0, 168.0, format!("{}", st.next_extend)));
        let col = if st.time >= st.time_threshold {
            0xFFFFF0C0
        } else {
            0xFFFFFFFF
        };
        let tt = format!("{}", st.time);
        q.push(Text::new(488.0, 184.0, tt.clone()).argb(col));
        let x = 488.0 + 13.0 * tt.len() as f32;
        q.push(Text::new(x, 184.0, "/").argb(col));
        q.push(Text::new(x + 6.0, 184.0, format!("{}", st.time_threshold)).argb(col));
    }
}

impl Task<Ctx> for Hud {
    fn on_register(&mut self, ctx: &mut Ctx, chain: Chain, _: u8) -> Result<()> {
        if chain == Chain::Calc {
            self.load(ctx)?;
        }
        Ok(())
    }

    fn on_remove(&mut self, ctx: &mut Ctx, chain: Chain, _: u8) {
        if chain == Chain::Calc {
            for s in [FRONT_SLOT, TIMES_SLOT, TXT_SLOT, LOADING_SLOT]
                .into_iter()
                .chain(msg::FACE_SLOTS)
            {
                ctx.anm.unload(&mut ctx.gfx, s);
            }
        }
    }

    fn calc(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        self.frame += 1;
        let inp = ctx.input.game_view();
        let dying = self.player.borrow().state == 2;
        let (collect, events, side) = {
            let mut d = self.msg.borrow_mut();
            d.info.clock = self.player.borrow().stats.clock;
            let c = d.run(&mut ctx.anm, &inp, dying);
            ctx.game.dialogue_skippable = d.skippable_shown();
            (
                c,
                std::mem::take(&mut d.events),
                std::mem::take(&mut d.side_effects),
            )
        };
        // Msg_Start playfield side effects (10 §4 step 6): cancel bullets, kill killable enemies.
        if side {
            self.enemies.borrow_mut().msg_side_effects(&mut ctx.anm);
        }
        // All items auto-collect while a dialogue runs and the player is not dying (10 §5).
        if collect {
            self.enemies
                .borrow()
                .bullets
                .borrow_mut()
                .items
                .attract_all();
        }
        self.flush_texts(ctx);
        self.handle_events(ctx, events);
        self.flush_texts(ctx);
        let reqs = std::mem::take(&mut self.enemies.borrow_mut().g.hud_requests);
        let clock = self.player.borrow().stats.clock;
        for r in reqs {
            match r {
                // ECL 179 / 181 (12 §5.7, 07 §7): night clock script 2 with sprite = clock; 181 interrupts 2 at 12,
                // else 1; 180 hides it with alpha := 0.
                179 => {
                    ctx.anm.set_script(&mut self.night_vm, TIMES_SLOT, 2);
                    self.night_vm.set_sprite(&ctx.anm, clock.clamp(0, 15));
                }
                180 => self.night_vm.color[anmvm::A] = 0,
                // ECL op 148: progress counter 0x164D30C += 1800 (10 §8.3, end panel 16 §2.5).
                148 => self.player.borrow_mut().stats.rec.progress += 1800,
                181 => {
                    self.night_vm.set_sprite(&ctx.anm, clock.clamp(0, 15));
                    self.night_vm.pending_interrupt = if clock >= 12 { 2 } else { 1 };
                }
                _ => {}
            }
        }
        let dialogue = self.msg.borrow().active();
        self.boss_calc(
            &mut ctx.anm,
            dialogue,
            ctx.game.paused != 0 || ctx.game.game_over != 0,
        );
        self.clear_bonus(&ctx.game);
        self.clear_calc(&mut ctx.game, &inp);
        for p in self.player.borrow_mut().stats.popups.drain(..) {
            self.popups.push(ActivePopup { kind: p, frame: 0 });
        }
        // Popup lifetimes 250 / 180 / 280 frames (12 §5.5).
        self.popups.retain_mut(|p| {
            p.frame += 1;
            p.frame
                < match p.kind {
                    Popup::Bonus(_) => 250,
                    Popup::Message(..) => 180,
                    Popup::SpellBonus(_) => 280,
                }
        });
        // Player-overlap fade of the night clock (12 §5.7): while alpha ≠ 0, −4 down to 0x40 with the player at
        // x ≥ 64, y < 128, else +4 up to 0xFF (≥ 0xFC snaps to 0xFF).
        // SPEC-GAP: 12 §5.7 does not order the fade against the VM run in Hud_Calc; fade first.
        let a = self.night_vm.color[anmvm::A];
        if a != 0 {
            let p = self.player.borrow().pos;
            self.night_vm.color[anmvm::A] = if p[0] >= 64.0 && p[1] < 128.0 {
                if a > 0x40 { (a - 4).max(0x40) } else { a }
            } else if a >= 0xFC {
                0xFF
            } else {
                a + 4
            };
        }
        let anm = &mut ctx.anm;
        for vm in self.front.iter_mut().chain(self.title.iter_mut()).chain([
            &mut self.diff_vm,
            &mut self.clear_vm,
            &mut self.night_vm,
        ]) {
            if vm.script.is_some() {
                vm.run(anm);
            }
        }
        // The clock VM ticks only while stage-title VM 0 has alpha ≠ 0, never in spell practice (12 §5.1).
        let title_on = self.title.first().is_some_and(|v| v.color[anmvm::A] != 0);
        if self.start_clock_vm.script.is_some() && title_on && !ctx.game.spell_practice() {
            self.start_clock_vm.run(anm);
        }
        Ret::Next
    }

    // Hud_Draw (12 §5.3): clear panel strings, Msg_Draw, Hud_DrawBoss, Hud_DrawPanel, Hud_DrawPopups. Everything
    // before Hud_DrawPanel inherits the playfield viewport set by Stage_DrawLow.
    fn draw(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        if ctx.game.loading != 0 {
            return Ret::Next;
        }
        self.draw_clear(ctx);
        ctx.gfx.set_viewport(PLAYFIELD);
        self.draw_msg(&mut ctx.anm, &mut ctx.gfx);
        // Hud_DrawBoss 0x43741D: the bar and timer only while no dialogue runs (12 §5.4).
        if !self.msg.borrow().active() {
            self.draw_boss(ctx);
        }
        ctx.gfx.set_viewport(PLAYFIELD);
        // Stage title VMs, the stage-start clock, then the night clock with Draw2DRot (12 §5.1, §5.7).
        for vm in self
            .title
            .iter_mut()
            .chain(std::iter::once(&mut self.start_clock_vm))
        {
            if vm.script.is_some() {
                ctx.anm.draw_mode(&mut ctx.gfx, vm, 0);
            }
        }
        if self.night_vm.script.is_some() {
            ctx.anm.draw_mode(&mut ctx.gfx, &mut self.night_vm, 2);
        }
        // SPEC-GAP: the draw site of the stage-clear VM impl+0x212C8 is not in 10 §9 / 12 §5; drawn here, playfield viewport.
        if self.clear_vm.script.is_some() {
            ctx.anm.draw_mode(&mut ctx.gfx, &mut self.clear_vm, 0);
        }
        let st = self.player.borrow().stats.clone();
        self.draw_panel(ctx, &st);
        self.draw_popups(ctx);
        Ret::Next
    }
}

// Hud_SetStageNamePlate 0x438046 (12 §5.2 table); None = no call (the plate keeps its old picture, quirk).
fn stage_plate(stage: usize, g: &crate::gamemgr::Game) -> Option<usize> {
    let sp = g.spell_practice();
    Some(match stage {
        1 => 0x11,
        2 => 0x12,
        4 => 0x14,
        6 => 0x17,
        3 if !sp || g.card == 0xD6 => 0x13,
        3 if g.is_practice_card_in(0xD8, 0xDD) => 0x1A + (g.card - 0xD8) as usize,
        3 => return None,
        5 if !sp || g.card == 0xD4 => 0x15,
        5 => 0x16,
        7 if !sp || g.is_practice_card_in(0x93, 0x96) => 0x17,
        7 => 0x18,
        8 if !sp || g.is_practice_card_in(0xBF, 0xC1) || g.card == 0xD5 => 0x20,
        8 => 0x19,
        _ => 0x10,
    })
}

// Hud_SetBossNamePlate 0x437F5C (12 §5.2): copies front.anm sprite n's pixels into sprite 10's rect (copy 0x467040).
pub fn set_boss_plate(anm: &AnmManager, gfx: &mut Gfx, n: usize) {
    if let (Some(&src), Some(&dst)) = (anm.sprite(FRONT_SLOT, n), anm.sprite(FRONT_SLOT, 10)) {
        let px = |s: &crate::anmmgr::SlotSprite, gfx: &Gfx| {
            let t = gfx.texture(s.tex);
            [
                (s.uv[0] * t.width as f32).round() as u32,
                (s.uv[1] * t.height as f32).round() as u32,
            ]
        };
        let (a, b) = (px(&src, gfx), px(&dst, gfx));
        gfx.copy_texture(
            src.tex,
            a,
            dst.tex,
            b,
            [src.w.min(dst.w) as u32, src.h.min(dst.h) as u32],
        );
    }
}
