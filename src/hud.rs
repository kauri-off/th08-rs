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

// Hud_ShowBonus / Hud_ShowMessage / Hud_ShowSpellBonus popup: timer t (frames) and slide x (12 §5.5).
struct ActivePopup {
    kind: Popup,
    t: u32,
    x: f32,
}

impl ActivePopup {
    // The show call: x := 416, t := 0.
    fn show(kind: Popup) -> Self {
        Self {
            kind,
            t: 0,
            x: 416.0,
        }
    }

    // Popup slot = one of the three timers impl+0x22D9C / +0x22DBC / +0x22DDC.
    fn slot(kind: Popup) -> usize {
        match kind {
            Popup::Bonus(_) => 0,
            Popup::Message(..) => 1,
            Popup::SpellBonus(_) => 2,
        }
    }

    // Hud_Update step 5: x from the timer before its tick, then the lifetime test (250 / 180 / 280), then the tick.
    // Returns false when the popup is switched off.
    fn update(&mut self) -> bool {
        self.x = if self.t < 30 {
            self.t as f32 * -312.0 / 30.0 + 416.0
        } else {
            104.0
        };
        let life = match self.kind {
            Popup::Bonus(_) => 250,
            Popup::Message(..) => 180,
            Popup::SpellBonus(_) => 280,
        };
        if self.t >= life {
            return false;
        }
        self.t += 1;
        true
    }
}

// Stage-clear panel state (10 §9, 12 §5.6).
struct ClearPanel {
    // impl+0x22E10: clock-animation delay timer (op 9 := 0).
    timer: u32,
    stage: usize,
    // Stored by msg op 9 (10 §9): clear bonus, point items, graze, time.
    clear: i32,
    points: i32,
    graze: i32,
    time: i32,
    // impl+0x22D84 = 1: bonus not computed yet; Hud_Update does it once, on the first frame after op 9 (12 §5.1b).
    pending: bool,
    // Night clock in minutes (660 + 30·clock) before / after the advance (impl+0x22E04 / +0x22E08), and the animated
    // value (impl+0x22E0C).
    from: i32,
    to: i32,
    shown: i32,
}

impl ClearPanel {
    // Clear trigger then clock animation (12 §5.6), once per Hud_Update after the bonus step; `end` = GM flags bits
    // 5–6, `fast` = SHOT or SKIP in g_Input. Returns true when the trigger sets bits 5–6 := 2.
    fn step(&mut self, end: u32, fast: bool) -> bool {
        // Tested before the step and without a 60-frame gate: fires the frame after `shown` reaches `to`, or on the
        // bonus frame when the clock did not advance.
        let fire = self.stage < 6 && self.shown != 0 && self.shown >= self.to && end == 0;
        if self.shown != 0 && self.shown != self.to {
            if self.timer < 60 {
                self.timer += 1;
            } else if self.shown < self.to {
                self.shown += 1;
                if fast {
                    self.shown += 3;
                }
                self.shown = self.shown.min(self.to);
            } else {
                self.timer += 1;
            }
        }
        fire
    }
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
    // +0x1C boss bar alpha (i32) and impl+0x2A40 bar state: 0 off, 1 fading in, 2 shown, 3 fading out (12 §5.1b).
    bar_alpha: i32,
    bar_state: u8,
    // +0x30 / +0x34 target and displayed life fraction (+0.01 up, −0.02 down per frame).
    bar_target: f32,
    bar_disp: f32,
    // +0x28 raw boss timer value last drawn (12 §5.4); 0 after Hud_Register.
    timer_last: i32,
    // Bonus / message / spell-bonus popups (12 §5.5).
    popups: [Option<ActivePopup>; 3],
    clear: Option<ClearPanel>,
    // impl+0x22D88: stored clear-panel total (12 §5.6).
    clear_total: i32,
    // Raw fresh-start flag 0x17CE8BC at registration (12 §5.1a).
    fresh: bool,
    // Msg op 9 stage-clear VMs (10 §9): impl+0x3778 loadingNN.anm (its script index +0x398C ≥ 0 gates all of them),
    // impl+0x3CC0 capture.anm script 1 and the 8 ghosts impl+0x3F64 + k·0x2A4.
    load_vm: AnmVm,
    cap_vm: AnmVm,
    ghosts: Vec<AnmVm>,
    // Capture queued by op 9; applied after the next Present (12 §3).
    capture_pending: bool,
    // 168 stage-start tiles impl+0x5728 (r·12 + c) and the live count impl+0x21810 (12 §5.1a, §5.7).
    tiles: Vec<AnmVm>,
    tiles_live: u32,
}

// The HUD impl block survives a continuing stage (12 §5.1a): front VMs 0–15, the difficulty VM, the stored total.
struct Kept {
    front: Vec<AnmVm>,
    diff_vm: AnmVm,
    clear_total: i32,
}

thread_local! {
    static KEPT: RefCell<Option<Kept>> = const { RefCell::new(None) };
}

// FUN_00437d87 HUD cover (06 §10.2): impl+0x398C ≥ 0 and VM impl+0x3778 waits for an interrupt (flag bit 14).
fn hud_cover(load_vm: &AnmVm) -> bool {
    load_vm.script_index >= 0 && load_vm.flags & anmvm::F_WAIT_INT != 0
}

// Hud_Load continuing branch (12 §5.1a): tile (r, c) = capture.anm script 3 + ((r + c) & 1), I4 (+0x120) := r + 2c,
// pos (32c − 0.5 + 16, 32r − 0.5 + 16, 0), uv offset (32c/512, 32r/512).
fn stage_start_tiles(anm: &mut AnmManager) -> Vec<AnmVm> {
    let mut tiles = Vec::with_capacity(168);
    for r in 0..14 {
        for c in 0..12 {
            let mut vm = AnmVm::default();
            anm.set_script(&mut vm, crate::ascii::CAPTURE_SLOT, 3 + ((r + c) & 1));
            vm.ivars[4] = (r + 2 * c) as i32;
            vm.pos = [
                32.0 * c as f32 - 0.5 + 16.0,
                32.0 * r as f32 - 0.5 + 16.0,
                0.0,
            ];
            vm.uv_off = [32.0 * c as f32 / 512.0, 32.0 * r as f32 / 512.0];
            tiles.push(vm);
        }
    }
    tiles
}

impl Hud {
    pub fn register(
        ctx: &mut Ctx,
        player: PlayerRef,
        enemies: Rc<RefCell<EnemyManager>>,
        stage: Rc<RefCell<Stage>>,
        fresh: bool,
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
            bar_state: 0,
            bar_target: 0.0,
            bar_disp: 0.0,
            timer_last: 0,
            popups: [None, None, None],
            clear: None,
            clear_total: 0,
            fresh,
            load_vm: AnmVm::default(),
            cap_vm: AnmVm::default(),
            ghosts: Vec::new(),
            capture_pending: false,
            tiles: Vec::new(),
            tiles_live: 0,
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
        // The script-index shorts of VMs impl+0x3778 / +0x3A1C / +0x3CC0 := −1 and impl+0x21810 := 0 in both branches.
        self.load_vm.script_index = -1;
        self.cap_vm.script_index = -1;
        self.tiles_live = 0;
        // +0x2C := 0, impl+0x2A40 := 0, +0x30 := +0x34 := 0 (12 §5.1a).
        {
            let mut e = self.enemies.borrow_mut();
            e.g.hud_boss = false;
            e.g.hud_bar_target = 0.0;
        }
        self.bar_state = 0;
        self.bar_disp = 0.0;
        let kept = KEPT.with_borrow_mut(Option::take).filter(|_| !self.fresh);
        if let Some(k) = kept {
            // Continuing stage: no VM-13 repaint, no panel fade-in; the stage-start transition over the old capture.
            self.front = k.front;
            self.diff_vm = k.diff_vm;
            self.clear_total = k.clear_total;
            anm.set_script(&mut self.cap_vm, crate::ascii::CAPTURE_SLOT, 1);
            self.cap_vm.sprite_index = 1;
            self.tiles = stage_start_tiles(anm);
            self.tiles_live = self.tiles.len() as u32;
        } else {
            self.front = (0..16)
                .map(|i| {
                    let mut vm = AnmVm::default();
                    anm.set_script(&mut vm, FRONT_SLOT, i);
                    vm
                })
                .collect();
            anm.set_script(&mut self.diff_vm, crate::ascii::ANM_SLOT, 0x19);
            self.diff_vm
                .set_sprite(anm, crate::ascii::difficulty_sprite(g));
        }
        self.title = (0..4)
            .map(|i| {
                let mut vm = AnmVm::default();
                if !g.spell_practice() || i == 3 {
                    anm.set_script(&mut vm, TXT_SLOT, i);
                }
                vm
            })
            .collect();
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
                // Msg op 7 (10 §5.1): song-title card = stgNtxt script 3 with sprite n + 3, then preloaded BGM slot n.
                Event::Bgm(n) => {
                    let vm = &mut self.title[3];
                    ctx.anm.set_script(vm, TXT_SLOT, 3);
                    vm.set_sprite(&ctx.anm, n + 3);
                    ctx.audio.play_slot(n as usize);
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
        // Stages 1–5: loading VM, capture VM, 8 ghosts (I4 := 4k + 3, alpha := 64 − 2k) and the playfield capture.
        if stage < 6 {
            let anm = &mut ctx.anm;
            anm.set_script(&mut self.load_vm, LOADING_SLOT, 0);
            anm.set_script(&mut self.cap_vm, crate::ascii::CAPTURE_SLOT, 1);
            self.ghosts = (0..8)
                .map(|k| {
                    let mut vm = AnmVm::default();
                    anm.set_script(&mut vm, crate::ascii::CAPTURE_SLOT, 2);
                    vm.ivars[4] = 4 * k + 3;
                    vm.color[anmvm::A] = (64 - 2 * k) as u8;
                    vm
                })
                .collect();
            self.capture_pending = true;
        }
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
            timer: 0,
            stage,
            clear: CLEAR_BONUS[stage] as i32,
            points: st.point_items,
            graze: st.graze,
            time: st.time,
            pending: true,
            from,
            to,
            shown: from,
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
        t = match g.game_lives() {
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

    // Clear trigger and clock animation (12 §5.6); `None` before op 9 stands for `shown` = 0.
    fn clear_calc(&mut self, g: &mut Game, inp: &crate::input::Input) {
        let Some(c) = &mut self.clear else { return };
        if c.step(
            g.end_state(),
            inp.down(crate::input::SHOT | crate::input::SKIP),
        ) {
            g.set_end_state(2);
        }
    }

    // Hud_Update step 1 (12 §5.1b): boss-bar state machine; the caller skips it while a dialogue runs.
    fn boss_calc(&mut self) {
        let (req, target) = {
            let e = self.enemies.borrow();
            (e.g.hud_boss, e.g.hud_bar_target)
        };
        self.bar_target = target;
        let label = &mut self.front[12];
        // W: L's wait-for-interrupt flag as left by its run of the previous frame (L runs after this step).
        let waiting = label.flags & anmvm::F_WAIT_INT != 0;
        (self.bar_state, self.bar_alpha, self.bar_disp) = bar_step(
            req,
            waiting,
            (
                self.bar_state,
                self.bar_alpha,
                self.bar_disp,
                self.bar_target,
            ),
            &mut label.pending_interrupt,
        );
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
            Self::bar_quad(gfx, x0, x1, y0, y0 + h, top, bot);
        }
        let d = &mut *d;
        for vm in d.lines.iter_mut().chain(std::iter::once(&mut d.boss_card)) {
            if vm.script.is_some() {
                anm.draw_mode(gfx, vm, 0);
            }
        }
    }

    // Screen_FillRect3 0x45B490 (12 §5.4, 10 §7): untextured quad at exact D3D coordinates (+0.5 for wgpu).
    fn bar_quad(gfx: &mut Gfx, x0: f32, x1: f32, y0: f32, y1: f32, top: [u8; 4], bot: [u8; 4]) {
        let uv = [0.0, 0.0];
        let (x0, x1, y0, y1) = (x0 + 0.5, x1 + 0.5, y0 + 0.5, y1 + 0.5);
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

    // Hud_DrawBoss 0x43741D (12 §5.4): unguarded VMs, the guarded bar part, the stage-clear VM last.
    fn draw_boss(&mut self, ctx: &mut Ctx) {
        let (anm, gfx) = (&mut ctx.anm, &mut ctx.gfx);
        gfx.set_viewport(PLAYFIELD);
        // Stage title VMs, the stage-start clock, then the night clock with Draw2DRot (12 §5.1, §5.7).
        for vm in self
            .title
            .iter_mut()
            .chain(std::iter::once(&mut self.start_clock_vm))
        {
            if vm.script.is_some() {
                anm.draw_mode(gfx, vm, 0);
            }
        }
        if self.night_vm.script.is_some() {
            anm.draw_mode(gfx, &mut self.night_vm, 2);
        }
        // Op 9 VMs: loading picture Draw2DSnap, capture and ghosts Draw3DQuad; impl+0x3A1C is never started (10 §9).
        if self.load_vm.script_index >= 0 {
            anm.draw_mode(gfx, &mut self.load_vm, 0);
            for vm in std::iter::once(&mut self.cap_vm).chain(self.ghosts.iter_mut()) {
                anm.draw_mode(gfx, vm, 5);
            }
        }
        if self.tiles_live != 0 {
            for vm in &mut self.tiles {
                anm.draw_mode(gfx, vm, 5);
            }
        }
        if !self.msg.borrow().active() {
            self.draw_bar(ctx);
        }
        if self.clear_vm.script.is_some() {
            ctx.anm.draw_mode(&mut ctx.gfx, &mut self.clear_vm, 0);
        }
    }

    // Guarded part of Hud_DrawBoss (12 §5.4): no dialogue, +0x2C (read live) + bar state ≠ 0.
    fn draw_bar(&mut self, ctx: &mut Ctx) {
        if !self.enemies.borrow().g.hud_boss && self.bar_state == 0 {
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
        // ECL op 158 markers: drawn when end ≠ 0 and start < displayed; bottom colour (c >> 2) & 0x3F3F3F.
        for (end, start, c) in e.g.hud_markers {
            if end != 0.0 && start < disp {
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
            Self::bar_quad(gfx, x0, x1, 19.0, 23.0, c, Self::rgb(0x202020, a));
        }
        // Timer (12 §5.4): every guarded draw, HUD +0x24 as stored by Enemy_CheckTimeout, shown as min(·, 99).
        let raw = e.g.hud_timer;
        drop(e);
        ctx.anm.draw_mode(&mut ctx.gfx, &mut self.front[12], 0);
        let v = raw.min(99);
        ctx.ascii
            .push(Text::new(384.0, 16.0, fmt_02d(v)).color(Self::rgb(timer_colour(raw), a)));
        // Sound on a change of the raw value: v < 3 se_timeout2 (0x26), else v < 10 se_timeout (0x1D).
        if self.timer_last != raw {
            if v < 3 {
                crate::sound::play(crate::sound::TIMEOUT2);
            } else if v < 10 {
                crate::sound::play(crate::sound::TIMEOUT);
            }
        }
        self.timer_last = raw;
        // Popup_LargeStatic (12 §5.4, 13 §4): re-issued every guarded draw, "+0" without children too.
        let g = &ctx.game;
        let e = self.enemies.borrow();
        if let Some(i) = e.boss(0)
            && g.paused == 0
            && g.game_over == 0
            && !self.player.borrow().freeze_bullets
        {
            crate::ascii::popup_large_static(
                [2.0, 29.0],
                e.chain_count(i),
                e.enemies[i].children,
                0xFFF0F00F,
            );
        }
    }

    // Hud_DrawPopups 0x438A89 (12 §5.5): every string with the clip flag (playfield viewport); bonus / message
    // popups at the slide x of the last Hud_Update.
    fn draw_popups(&mut self, ctx: &mut Ctx) {
        for p in self.popups.iter().flatten() {
            match p.kind {
                Popup::Bonus(n) => ctx.ascii.push(
                    Text::new(p.x, 48.0, format!(" BONUS {n:8}"))
                        .argb(0xFFFFFF80)
                        .clip(),
                ),
                Popup::Message(t, v) => {
                    let (s, small) = match t {
                        1 => ("Full Power Mode!".to_string(), false),
                        // SPEC-GAP: no spec names a caller of types 2–4 (IMPL97 Q1).
                        2 => ("Supernatural Border!!".to_string(), true),
                        3 => ("CherryPoint Max!".to_string(), false),
                        4 => (format!("Border Bonus {v:7}"), true),
                        5 => ("Spell Bonus Failed".to_string(), true),
                        _ => ("Last Spell Failed".to_string(), true),
                    };
                    // Types 2/4–6 set Ascii_SetAdvance(11) but reset it to 13 before the draw pass reads it
                    // (12 §2.9), so only the 0.9 x scale narrows them.
                    let tx = Text::new(p.x, 168.0, s).clip();
                    ctx.ascii.push(if small {
                        tx.scale_xy(0.9, 1.0).argb(0xFFE0B0FF)
                    } else {
                        tx.argb(0xFFC0B0FF)
                    });
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
        // 6B / Extra lower the pen by 16 before the title: the whole panel starts at y 80.
        if c.stage > 6 {
            y -= 16.0;
        }
        line(q, y, title.into(), 0xFFFFFF40);
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
            let pen = match g.game_lives() {
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
        // Score row (y 56) queued before the hiscore row (y 40) (12 §5.3 step 6).
        q.push(Text::new(
            488.0,
            56.0,
            format!("{:09}", st.score_disp.min(999_999_999)),
        ));
        q.push(Text::new(605.0, 56.0, format!("{}", st.continues.min(9))));
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
            KEPT.set(Some(Kept {
                front: std::mem::take(&mut self.front),
                diff_vm: std::mem::take(&mut self.diff_vm),
                clear_total: self.clear_total,
            }));
            for s in [FRONT_SLOT, TIMES_SLOT, TXT_SLOT, LOADING_SLOT]
                .into_iter()
                .chain(msg::FACE_SLOTS)
            {
                ctx.anm.unload(&mut ctx.gfx, s);
            }
        }
    }

    // Hud_Calc 0x4338CA (12 §5.1b): Hud_Update, then Msg_Run, then the frame counter. Hud_Update runs first, so what
    // a msg op sets this frame (op 9 bonus, trigger, clock animation) is acted on the next frame.
    fn calc(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        // AnmManager_ApplyPendingCaptures runs right after the Present that follows op 9's calc (12 §3); the port
        // copies from that presented frame at the next calc.
        if std::mem::take(&mut self.capture_pending) {
            crate::ascii::apply_capture(&ctx.anm, &mut ctx.gfx, &self.cap_vm);
        }
        let inp = ctx.input.game_view();
        // HUD requests queued by ECL ops this frame (ECL runs before the HUD).
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
        // Hud_Update 0x435900, steps 1–7 of 12 §5.1b.
        if !self.msg.borrow().active() {
            self.boss_calc();
        }
        // Front VMs, stage-title VMs, the stage-start clock, the stage-clear VM, the night clock.
        let anm = &mut ctx.anm;
        for vm in self.front.iter_mut().chain(self.title.iter_mut()) {
            if vm.script.is_some() {
                vm.run(anm);
            }
        }
        // The clock VM ticks only while stage-title VM 0 has alpha ≠ 0, never in spell practice (12 §5.1).
        let title_on = self.title.first().is_some_and(|v| v.color[anmvm::A] != 0);
        if self.start_clock_vm.script.is_some() && title_on && !ctx.game.spell_practice() {
            self.start_clock_vm.run(anm);
        }
        for vm in [&mut self.clear_vm, &mut self.night_vm] {
            if vm.script.is_some() {
                vm.run(anm);
            }
        }
        // Player-overlap fade of the night clock (12 §5.7): while alpha ≠ 0, −4 down to 0x40 with the player at
        // x ≥ 64, y < 128, else +4 up to 0xFF (≥ 0xFC snaps to 0xFF).
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
        // Difficulty name VM impl+0x22E14.
        if self.diff_vm.script.is_some() {
            self.diff_vm.run(&mut ctx.anm);
        }
        if self.load_vm.script_index >= 0 {
            let anm = &mut ctx.anm;
            for vm in [&mut self.load_vm, &mut self.cap_vm]
                .into_iter()
                .chain(self.ghosts.iter_mut())
            {
                vm.run(anm);
            }
        }
        if self.tiles_live != 0 {
            for vm in &mut self.tiles {
                // SPEC-GAP: 12 §5.7 only says +0x21810 counts the live tiles; decremented here when a tile's script ends.
                if vm.ip.is_some() && vm.run(&mut ctx.anm) {
                    self.tiles_live -= 1;
                }
            }
        }
        // Show calls made earlier this frame restart their slot; then the popup timers (12 §5.1b step 5).
        for p in self.player.borrow_mut().stats.popups.drain(..) {
            self.popups[ActivePopup::slot(p)] = Some(ActivePopup::show(p));
        }
        for slot in &mut self.popups {
            if slot.as_mut().is_some_and(|p| !p.update()) {
                *slot = None;
            }
        }
        self.clear_bonus(&ctx.game);
        self.clear_calc(&mut ctx.game, &inp);
        // Msg_Run (10 §5), including op 9.
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
        self.stage.borrow_mut().hud_cover = hud_cover(&self.load_vm);
        self.frame += 1;
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
        self.draw_boss(ctx);
        let st = self.player.borrow().stats.clone();
        self.draw_panel(ctx, &st);
        self.draw_popups(ctx);
        Ret::Next
    }
}

// Hud_Update step 1 body (12 §5.1b) on (state, alpha, displayed, target); `irq` is the label VM's interrupt.
fn bar_step(
    req: bool,
    waiting: bool,
    (mut state, mut alpha, mut disp, target): (u8, i32, f32, f32),
    irq: &mut u16,
) -> (u8, i32, f32) {
    if !req {
        if state != 0 {
            if state < 3 {
                *irq = 2;
                state = 3;
            }
            if alpha != 0 {
                alpha -= 4;
            }
            // A fully shown bar (L waiting at its t 30) hides at once, no fade-out.
            if waiting {
                state = 0;
                disp = 0.0;
                alpha = 0;
            }
        }
    } else if state == 0 {
        *irq = 1;
        state = 1;
        alpha = 0;
    } else {
        if waiting {
            state = 2;
        }
        alpha = if (alpha as u32) < 0xFC {
            alpha + 4
        } else {
            0xFF
        };
    }
    // The fill moves only once the bar is shown: +0.01 up, −0.02 down, clamped at the target.
    if state >= 2 {
        if disp < target {
            disp = (disp + 0.01).min(target);
        } else if disp > target {
            disp = (disp - 0.02).max(target);
        }
    }
    (state, alpha, disp)
}

// Timer colour from the unclamped +0x24 (12 §5.4).
fn timer_colour(raw: i32) -> u32 {
    match raw {
        20.. => 0xA0D0FF,
        10..=19 => 0xA080FF,
        5..=9 => 0xE080C0,
        _ => 0xFF4040,
    }
}

// C `%.2d`: at least two digits, the sign not counted ("-01").
fn fmt_02d(v: i32) -> String {
    if v < 0 {
        format!("-{:02}", v.unsigned_abs())
    } else {
        format!("{v:02}")
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

#[cfg(test)]
mod tests {
    use super::*;

    fn panel(stage: usize, from: i32, to: i32) -> ClearPanel {
        ClearPanel {
            timer: 0,
            stage,
            clear: 0,
            points: 0,
            graze: 0,
            time: 0,
            pending: false,
            from,
            to,
            shown: from,
        }
    }

    // 12 §5.6: no clock advance → the trigger fires on the first step (the bonus frame), no 60-frame wait.
    #[test]
    fn trigger_without_advance_fires_at_once() {
        let mut c = panel(2, 720, 720);
        assert!(c.step(0, false));
        assert_eq!((c.shown, c.timer), (720, 0));
        // Bits 5–6 already set: no second trigger.
        assert!(!c.step(2, false));
    }

    // 12 §5.6: 60 delay steps, then +1 per step; the trigger is tested before the step, so it fires one step after
    // `shown` reaches `to`.
    #[test]
    fn trigger_after_animation() {
        let mut c = panel(0, 660, 663);
        for _ in 0..60 {
            assert!(!c.step(0, false));
        }
        assert_eq!((c.timer, c.shown), (60, 660));
        for m in 661..=663 {
            assert!(!c.step(0, false));
            assert_eq!(c.shown, m);
        }
        assert!(c.step(0, false));
    }

    // 12 §5.6: SHOT/SKIP adds 3 more, clamped to `to`.
    #[test]
    fn fast_step_clamps() {
        let mut c = panel(0, 660, 666);
        c.timer = 60;
        c.step(0, true);
        assert_eq!(c.shown, 664);
        c.step(0, true);
        assert_eq!(c.shown, 666);
    }

    // 12 §5.6: final stages never trigger here; `shown` > `to` triggers (≥) and only ticks the timer.
    #[test]
    fn final_stage_and_backwards() {
        let mut c = panel(6, 900, 900);
        assert!(!c.step(0, false));
        let mut c = panel(1, 700, 690);
        c.timer = 60;
        assert!(c.step(0, false));
        assert_eq!((c.shown, c.timer), (700, 61));
    }

    // 06 §10.2 FUN_00437d87: the cover needs both the started VM impl+0x3778 and its wait-for-interrupt flag.
    #[test]
    fn hud_cover_condition() {
        let mut vm = AnmVm::default();
        vm.flags |= anmvm::F_WAIT_INT;
        assert!(!hud_cover(&vm));
        vm.script_index = 0;
        assert!(hud_cover(&vm));
        vm.flags &= !anmvm::F_WAIT_INT;
        assert!(!hud_cover(&vm));
    }

    // 12 §5.5: x from the timer before its tick, lifetime test before the tick.
    #[test]
    fn popup_slide_and_lifetime() {
        let mut p = ActivePopup::show(Popup::Message(1, 0));
        let mut xs = Vec::new();
        while p.update() {
            xs.push(p.x);
        }
        // Drawn for 180 frames: 416 on the first, 405.6 on the second, 104 from frame 31.
        assert_eq!(xs.len(), 180);
        assert_eq!(xs[0], 416.0);
        assert_eq!(xs[1], 1.0 * -312.0 / 30.0 + 416.0);
        assert_eq!(xs[30], 104.0);
        let mut b = ActivePopup::show(Popup::Bonus(5));
        assert_eq!(std::iter::from_fn(|| b.update().then_some(())).count(), 250);
    }

    // 12 §5.1a: 14 × 12 tiles, I4 = r + 2c, pos (32c + 15.5, 32r + 15.5), uv offset (32c/512, 32r/512).
    #[test]
    fn stage_start_tile_layout() {
        let tiles = stage_start_tiles(&mut AnmManager::default());
        assert_eq!(tiles.len(), 168);
        let t = &tiles[13 * 12 + 11];
        assert_eq!(t.ivars[4], 13 + 22);
        assert_eq!(t.pos, [32.0 * 11.0 + 15.5, 32.0 * 13.0 + 15.5, 0.0]);
        assert_eq!(t.uv_off, [352.0 / 512.0, 416.0 / 512.0]);
    }

    // 12 §5.1b step 1: request → alpha 0, fill frozen until shown; hide while L waits ends at once.
    #[test]
    fn boss_bar_states() {
        let mut irq = 0;
        let r = bar_step(true, false, (0, 40, 0.0, 1.0), &mut irq);
        assert_eq!((r, irq), ((1, 0, 0.0), 1));
        let r = bar_step(true, false, (1, 116, 0.0, 1.0), &mut irq);
        assert_eq!(r, (1, 120, 0.0));
        let r = bar_step(true, true, (1, 120, 0.0, 1.0), &mut irq);
        assert_eq!(r, (2, 124, 0.01));
        assert_eq!(
            bar_step(true, true, (2, 0xFC, 0.5, 0.4), &mut irq),
            (2, 0xFF, 0.48)
        );
        irq = 0;
        assert_eq!(
            bar_step(false, true, (2, 0xFF, 0.5, 0.4), &mut irq),
            (0, 0, 0.0)
        );
        assert_eq!(irq, 2);
        assert_eq!(
            bar_step(false, false, (1, 8, 0.0, 1.0), &mut irq),
            (3, 4, 0.01)
        );
        assert_eq!(
            bar_step(false, false, (3, 4, 0.0, 1.0), &mut irq),
            (3, 0, 0.01)
        );
        assert_eq!(
            bar_step(false, false, (0, 0, 0.0, 1.0), &mut irq),
            (0, 0, 0.0)
        );
    }

    // 12 §5.4: %.2d and the colour from the unclamped value.
    #[test]
    fn boss_timer_text() {
        assert_eq!(fmt_02d(7), "07");
        assert_eq!(fmt_02d(99), "99");
        assert_eq!(fmt_02d(-1), "-01");
        assert_eq!(timer_colour(150), 0xA0D0FF);
        assert_eq!(timer_colour(10), 0xA080FF);
        assert_eq!(timer_colour(5), 0xE080C0);
        assert_eq!(timer_colour(-3), 0xFF4040);
    }
}
