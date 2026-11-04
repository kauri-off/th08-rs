//! HUD g_Hud 0x160F428 (Hud_Calc / Hud_Draw): side panel, boss UI, popups, clear panel, dialogue host (12 §5, 10).

use crate::anmmgr::AnmManager;
use crate::anmvm::{self, AnmVm, table_index};
use crate::ascii::Text;
use crate::drawlist::{
    Align, Blend, FULLSCREEN, PLAYFIELD, Recorder, TexId, TextCmd, TextSite, Vertex,
};
use crate::enemy::EnemyManager;
use crate::game::Ctx;
use crate::gamemgr::{self, Game};
use crate::msg::{self, Dialogue, Event};
use crate::player::{Player, Popup, Stats};
use crate::task::{self, Chain, Ret, Task};
use crate::types::lo8;
use anyhow::Result;

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

// Side-panel layout, screen pixels (12 §5.3): text column x, continue digit x, row y per value.
const PANEL_X: f32 = 488.0;
const CONT_X: f32 = 605.0;
const HISCORE_Y: f32 = 40.0;
const SCORE_Y: f32 = 56.0;
const LIVES_Y: f32 = 88.0;
const BOMBS_Y: f32 = 104.0;
const POWER_Y: f32 = 136.0;
const GRAZE_Y: f32 = 152.0;
const POINT_Y: f32 = 168.0;
const TIME_Y: f32 = 184.0;
// Lives / bombs stamps: 16 px apart, z 0.46, at most 8 (12 §5.3).
const STOCK_DX: f32 = 16.0;
const STOCK_Z: f32 = 0.46;
const STOCK_MAX: i32 = 8;
// Panel tile depths: score rows 0.49, other rows 0.48 (12 §5.3).
const TILE_Z_TOP: f32 = 0.49;
const TILE_Z_ROW: f32 = 0.48;

// Boss bar (12 §5.4): fill x 64 … 64 + 320·fraction, y 19..23; remaining-bars gauge x 35 … 61; timer text (384, 16).
const BAR_X: f32 = 64.0;
const BAR_W: f32 = 320.0;
const BAR_Y0: f32 = 19.0;
const BAR_Y1: f32 = 23.0;
const GAUGE_X: f32 = 35.0;
const GAUGE_W: f32 = 26.0;
const TIMER_POS: [f32; 2] = [384.0, 16.0];
// Bar colours 0xRRGGBB: fill top / bottom, gauge bottom; gauge top = white − i·255/9.
const BAR_TOP: u32 = 0x00FF_FFFF;
const BAR_BOT: u32 = 0x0020_2060;
const GAUGE_BOT: u32 = 0x0020_2020;

// Clear panel (12 §5.6): pen (120, 96), line step 16, section gap 32; clock row x 120 / 219 / 253.
const CLEAR_X: f32 = 120.0;
const CLEAR_Y: f32 = 96.0;
const CLEAR_DY: f32 = 16.0;
const CLEAR_GAP: f32 = 32.0;
const CLEAR_ARROW_X: f32 = 219.0;
const CLEAR_TO_X: f32 = 253.0;
// Clear panel line colours (ARGB).
const CLEAR_TITLE: u32 = 0xFFFF_FF40;
const CLEAR_WHITE: u32 = 0xFFFF_FFFF;
const CLEAR_POINT: u32 = 0xFFE0_E0FF;
const CLEAR_ROW: u32 = 0xFFD0_D0FF;
const CLEAR_FINAL: u32 = 0xFFFF_FF80;
const CLEAR_RANK: u32 = 0xFFFF_8080;

// Popups (12 §5.5): bonus row y 48, message row y 168, spell bonus rows y 80 / 96 centred in the 384-px playfield + 32.
const POPUP_BONUS_Y: f32 = 48.0;
const POPUP_MSG_Y: f32 = 168.0;
const POPUP_SPELL_Y: f32 = 80.0;
const POPUP_SPELL_VALUE_Y: f32 = 96.0;
const POPUP_BONUS: u32 = 0xFFFF_FF80;
const POPUP_MSG_SMALL: u32 = 0xFFE0_B0FF;
const POPUP_MSG: u32 = 0xFFC0_B0FF;
const POPUP_SPELL: u32 = 0xFFFF_0000;
const POPUP_SPELL_VALUE: u32 = 0xFFFF_8080;

// Hud_ShowBonus / Hud_ShowMessage / Hud_ShowSpellBonus popup: timer t and slide x (12 §5.5).
#[derive(Clone)]
struct ActivePopup {
    kind: Popup,
    // Timer impl+0x22DA0 / +0x22DC0 / +0x22DE0, ticked at game speed (12 §5.5).
    t: anmvm::Timer,
    x: f32,
}

impl ActivePopup {
    // The show call: x := 416, t := 0.
    fn show(kind: Popup) -> Self {
        let mut t = anmvm::Timer::default();
        t.set(0);
        Self { kind, t, x: 416.0 }
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
    // x from Timer_GetF in f32 per operation (05 §10); the tests on the integer cur. False = switched off.
    fn update(&mut self, gs: f32) -> bool {
        self.x = if self.t.cur < 30 {
            self.t.getf() * -312.0 / 30.0 + 416.0
        } else {
            104.0
        };
        let life = match self.kind {
            Popup::Bonus(_) => 250,
            Popup::Message(..) => 180,
            Popup::SpellBonus(_) => 280,
        };
        if self.t.cur >= life {
            return false;
        }
        self.t.tick(gs);
        true
    }
}

// Stage-clear panel state (10 §9, 12 §5.6).
#[derive(Clone)]
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

// g_Hud 0x160F428; `Ctx.hud`, its chain nodes are `HudTask`. The dialogue it hosts (HudImpl+0x21814) is `Ctx.msg`.
#[derive(Clone)]
pub struct Hud {
    frame: u32,
    // front.anm VMs 0–15 (impl+i·0x2A4, 12 §5.2).
    front: Vec<AnmVm>,
    // stgNtxt.anm scripts 0–3 at impl+0x2A44 (stage title, song-title card = script 3).
    title: Vec<AnmVm>,
    // ascii.anm script 0x19 at impl+0x22E14 (difficulty name), script 3 at impl+0x212C8 (stage clear).
    diff_vm: AnmVm,
    // ascii.anm script 0x1A at impl+0x5484: last-spell label over the lives/bombs rows (12 §5.3 step 4).
    label_vm: AnmVm,
    // Label latch for a panel repainted every frame (12 §5.3 step 4); recomputed on lives/bombs repaint frames.
    label_latch: bool,
    clear_vm: AnmVm,
    // times.anm night clock impl+0x2156C (ECL 179/181) and the stage-start clock impl+0x34D4 (12 §5.1).
    night_vm: AnmVm,
    start_clock_vm: AnmVm,
    // +0x1C boss bar alpha (i32) and impl+0x2A40 bar state: 0 off, 1 fading in, 2 shown, 3 fading out (12 §5.1b).
    bar_alpha: i32,
    bar_state: u8,
    // +0x30 / +0x34 target and displayed life fraction (+0.01 up, −0.02 down per frame).
    bar_target: f32,
    bar_disp: f32,
    // +0x28 raw boss timer value last drawn (12 §5.4); 0 after a fresh start, kept across stages (12 §5.1).
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

// The HUD impl block survives a continuing stage (12 §5.1a): front VMs 0–15, the difficulty VM, the stored total;
// HUD +0x24 / +0x28 are zeroed only by Hud_Register's fresh-start fill, so they carry over too (12 §5.1).
#[derive(Clone)]
pub struct Kept {
    front: Vec<AnmVm>,
    diff_vm: AnmVm,
    label_vm: AnmVm,
    clear_total: i32,
    timer: i32,
    timer_last: i32,
}

// FUN_00437d87 HUD cover (06 §10.2): impl+0x398C ≥ 0 and VM impl+0x3778 waits for an interrupt (flag bit 14).
fn hud_cover(load_vm: &AnmVm) -> bool {
    load_vm.script_index >= 0 && load_vm.flags & anmvm::F_WAIT_INT != 0
}

// HUD +0x04 lives / bombs dirty fields and the full-repaint counter 0x17CE8CC (12 §5.3). The port repaints the panel
// every frame; they only decide on which frames the last-spell label latch is recomputed.
#[derive(Clone, Copy, Default)]
pub struct Panel {
    lives: u8,
    bombs: u8,
    repaint: i32,
}

// One Hud_DrawPanel frame of the label latch (12 §5.3 steps 3, 4, 8): full repaint (tile VM 13 running or counter
// ≠ 0; cfg bit 12 is never set) sets the fields to 2; a lives/bombs repaint frame latches `cond`; every non-zero field
// −1; Frame_Present decrements the counter once per frame while not paused.
fn label_step(mut pn: Panel, vm13: bool, cond: bool, latch: bool, paused: bool) -> (Panel, bool) {
    if vm13 || pn.repaint != 0 {
        (pn.lives, pn.bombs) = (2, 2);
    }
    let latch = if pn.lives != 0 || pn.bombs != 0 {
        cond
    } else {
        latch
    };
    pn.lives = pn.lives.saturating_sub(1);
    pn.bombs = pn.bombs.saturating_sub(1);
    if !paused && pn.repaint > 0 {
        pn.repaint -= 1;
    }
    (pn, latch)
}

// Dirty-field writers (12 §5.3 table): `v` = 2, or 3 for `Spell_Start` / `Spell_End` (+0x04 |= 0xF).
impl Panel {
    pub fn mark_lives(&mut self, v: u8) {
        self.lives = v;
    }

    pub fn mark_bombs(&mut self, v: u8) {
        self.bombs = v;
    }

    // 0x17CE8CC := n (12 §5.3 counter writers).
    pub fn set_repaint(&mut self, n: i32) {
        self.repaint = n;
    }
}

// Hud_Load continuing branch (12 §5.1a): tile (r, c) = capture.anm script 3 + ((r + c) & 1), I4 (+0x120) := r + 2c,
// pos (32c − 0.5 + 16, 32r − 0.5 + 16, 0), uv offset (32c/512, 32r/512).
fn stage_start_tiles(anm: &mut AnmManager, rng: &mut crate::rng::Rng) -> Vec<AnmVm> {
    let mut tiles = Vec::with_capacity(168);
    for r in 0..14u8 {
        for c in 0..12u8 {
            let mut vm = AnmVm::default();
            anm.set_script(
                rng,
                &mut vm,
                crate::ascii::CAPTURE_SLOT,
                3 + usize::from((r + c) & 1),
            );
            vm.ivars[4] = i32::from(r + 2 * c);
            let (r, c) = (f32::from(r), f32::from(c));
            vm.pos = [32.0 * c - 0.5 + 16.0, 32.0 * r - 0.5 + 16.0, 0.0];
            vm.uv_off = [32.0 * c / 512.0, 32.0 * r / 512.0];
            tiles.push(vm);
        }
    }
    tiles
}

impl Hud {
    pub fn register(ctx: &mut Ctx, fresh: bool) -> Result<crate::chain::Node> {
        ctx.w.msg = Some(Box::new(Dialogue::default()));
        ctx.w.hud = Some(Box::new(Hud::new(fresh)));
        let t = crate::chain::Node::Hud;
        task::add(ctx, Chain::Calc, CALC_PRIO, 0, t)?;
        task::add(ctx, Chain::Draw, DRAW_PRIO, 0, t)?;
        Ok(t)
    }

    // The zero-filled HUD of Hud_Register (12 §5.1); Hud_Load fills it on registration.
    pub fn new(fresh: bool) -> Hud {
        Hud {
            frame: 0,
            // Hud_Load replaces all 16 in both branches.
            front: vec![AnmVm::default(); 16],
            title: Vec::new(),
            diff_vm: AnmVm::default(),
            label_vm: AnmVm::default(),
            label_latch: false,
            clear_vm: AnmVm::default(),
            night_vm: AnmVm::default(),
            start_clock_vm: AnmVm::default(),
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
        }
    }

    // Hud_Update step 5 (12 §5.1b): show calls made earlier this frame restart their slot, then the popup timers at
    // game speed `gs`.
    pub(crate) fn popup_step(&mut self, st: &mut Stats, panel: &mut Panel, gs: f32) {
        for p in st.popups.drain(..) {
            // Hud_ShowBonus / Hud_ShowMessage / Hud_ShowSpellBonus: 0x17CE8CC := 2 (12 §5.3).
            panel.set_repaint(2);
            self.popups[ActivePopup::slot(p)] = Some(ActivePopup::show(p));
        }
        for slot in &mut self.popups {
            if slot.as_mut().is_some_and(|p| !p.update(gs)) {
                *slot = None;
            }
        }
    }

    // Clone test state (worldclone.rs): boss bar, popups as raw bits.
    #[cfg(test)]
    pub fn trace(&self) -> Vec<u64> {
        let mut t = vec![
            u64::from(self.bar_state),
            i64::from(self.bar_alpha).cast_unsigned(),
            u64::from(self.bar_target.to_bits()),
            u64::from(self.bar_disp.to_bits()),
        ];
        t.extend(self.popups.iter().map(|p| {
            p.as_ref().map_or(0, |p| {
                i64::from(p.t.cur).cast_unsigned() << 32 | u64::from(p.x.to_bits())
            })
        }));
        t
    }

    // Hud_StageTransition 0x438F58 (12 §3, §5.7): VM impl+0x3CC0 := capture.anm script 1, then a playfield capture into
    // its sprite rect (the return value is ignored); overwrites the op-9 image that the next stage's tiles show.
    pub fn stage_transition(&mut self, anm: &mut AnmManager, rng: &mut crate::rng::Rng) {
        anm.file_set_script(rng, &mut self.cap_vm, crate::ascii::CAPTURE_SLOT, 1);
        crate::ascii::queue_capture(anm, &self.cap_vm);
    }

    // Hud_Load + Gui_LoadFaces 0x414590 + Msg_Load 0x439710 (10 §1); no msg file in spell practice.
    fn load(&mut self, ctx: &mut Ctx) -> Result<()> {
        Self::load_files(ctx)?;
        self.load_reset(ctx);
        self.load_front(ctx);
        self.load_title(ctx);
        Ok(())
    }

    // Hud_Load files: front / times / stgNtxt / loadingNN.anm, the face files, the msg file and Msg_Start's info.
    fn load_files(ctx: &mut Ctx) -> Result<()> {
        let g = &ctx.w.game;
        let (stage, shot) = (g.stage % 9, g.shot);
        let (rec, ar) = (&mut ctx.rec, &ctx.archive);
        ctx.w.anm.load(rec, ar, FRONT_SLOT, "front.anm")?;
        if let Some(n) = stage_plate(stage, g) {
            set_boss_plate(&ctx.w.anm, rec, n);
        }
        ctx.w.anm.load(rec, ar, TIMES_SLOT, "times.anm")?;
        ctx.w.anm.load(
            rec,
            ar,
            TXT_SLOT,
            // stg8txt.anm in spell practice with card id >= 0xCD (12 §5.1a).
            &format!(
                "stg{}txt.anm",
                TXT_NAMES[if g.spell_practice() && g.card >= 0xCD {
                    8
                } else {
                    stage
                }]
            ),
        )?;
        let team = th08_formats::sht::team(shot);
        let variant = match shot {
            0..=3 => "",
            s if s % 2 == 0 => "h",
            _ => "a",
        };
        if let Err(e) = ctx.w.anm.load(
            rec,
            ar,
            LOADING_SLOT,
            &format!("loading{team:02}{variant}.anm"),
        ) {
            log::debug!("loading anm: {e:#}");
        }
        let card = g.spell_practice().then_some(g.card);
        for (k, f) in msg::face_files(stage, shot, card).into_iter().enumerate() {
            ctx.w.anm.unload(rec, msg::FACE_SLOTS[k]);
            if let Some(f) = f {
                ctx.w.anm.load(rec, ar, msg::FACE_SLOTS[k], &f)?;
            }
        }
        // Hud::register creates the dialogue before the HUD's calc node is registered.
        let Some(d) = ctx.w.msg.as_deref_mut() else {
            anyhow::bail!("HUD loaded without its dialogue");
        };
        if !g.spell_practice() {
            d.file = Some(msg::parse(ar.read(&msg::file_name(stage, shot))?)?.into());
        }
        d.info = msg::StartInfo {
            stage,
            shot,
            continues: g.stats.continues,
            replay_route: replay_route(g),
            cleared_6a: g.cleared(true, 6),
            cleared_6b: g.cleared(false, 7),
            clock: 0,
        };
        Ok(())
    }

    // Hud_Load resets shared by both branches (12 §5.1a).
    fn load_reset(&mut self, ctx: &mut Ctx) {
        // The script-index shorts of VMs impl+0x3778 / +0x3A1C / +0x3CC0 := −1 and impl+0x21810 := 0 in both branches.
        self.load_vm.script_index = -1;
        self.cap_vm.script_index = -1;
        self.tiles_live = 0;
        // +0x2C := 0, impl+0x2A40 := 0, +0x30 := +0x34 := 0 (12 §5.1a).
        {
            let e = crate::em!(ctx.w);
            e.g.hud_boss = false;
            e.g.hud_bar_target = 0.0;
        }
        self.bar_state = 0;
        self.bar_disp = 0.0;
        // All six dirty fields := 2, 0x17CE8CC := 0x10 (12 §5.1a).
        ctx.w.panel.mark_lives(2);
        ctx.w.panel.mark_bombs(2);
        ctx.w.panel.set_repaint(0x10);
    }

    // Hud_Load branch (12 §5.1a): a continuing stage keeps the front VMs and starts the tiles, a fresh one restarts them.
    fn load_front(&mut self, ctx: &mut Ctx) {
        let g = &ctx.w.game;
        let anm = &mut ctx.w.anm;
        let kept = ctx.w.hud_kept.take().filter(|_| !self.fresh);
        if let Some(k) = kept {
            // Continuing stage: no VM-13 repaint, no panel fade-in; the stage-start transition over the old capture.
            self.front = k.front;
            self.diff_vm = k.diff_vm;
            self.label_vm = k.label_vm;
            self.clear_total = k.clear_total;
            crate::em!(ctx.w).g.hud_timer = k.timer;
            self.timer_last = k.timer_last;
            anm.set_script(
                &mut ctx.w.rng,
                &mut self.cap_vm,
                crate::ascii::CAPTURE_SLOT,
                1,
            );
            self.cap_vm.sprite_index = 1;
            self.tiles = stage_start_tiles(anm, &mut ctx.w.rng);
            self.tiles_live = self.tiles.len() as u32;
        } else {
            self.front = (0..16)
                .map(|i| {
                    let mut vm = AnmVm::default();
                    anm.set_script(&mut ctx.w.rng, &mut vm, FRONT_SLOT, i);
                    vm
                })
                .collect();
            anm.set_script(
                &mut ctx.w.rng,
                &mut self.label_vm,
                crate::ascii::ANM_SLOT,
                0x1A,
            );
            anm.set_script(
                &mut ctx.w.rng,
                &mut self.diff_vm,
                crate::ascii::ANM_SLOT,
                0x19,
            );
            self.diff_vm
                .set_sprite(anm, crate::ascii::difficulty_sprite(g));
        }
    }

    // Hud_Load tail (12 §5.1a, §5.7): stage-title VMs, the hidden night clock, the stage-start clock.
    fn load_title(&mut self, ctx: &mut Ctx) {
        let g = &ctx.w.game;
        let anm = &mut ctx.w.anm;
        let title_on = !g.reload_same || crate::gamemgr::spell_bgm_restarts(g.card);
        self.title = (0..4)
            .map(|i| {
                let mut vm = AnmVm::default();
                if !g.spell_practice() {
                    anm.set_script(&mut ctx.w.rng, &mut vm, TXT_SLOT, i);
                } else if i == 0 && title_on {
                    // Spell practice: VM 0 (not VM 3) := script 3 with the card's song-title sprite; VMs 1–3 untouched.
                    anm.set_script(&mut ctx.w.rng, &mut vm, TXT_SLOT, 3);
                    vm.set_sprite(anm, crate::gamemgr::spell_title_sprite(g.card) + 3);
                }
                vm
            })
            .collect();
        // Hud_Load 0x4390D6: the night-clock VM starts hidden with alpha 0 (12 §5.7).
        self.night_vm = AnmVm::default();
        self.night_vm.color[anmvm::A] = 0;
        // Clock sprite = clock counter stats+0x28 as it is now (12 §5.1a): the live player stats, since the
        // game-manager copy was taken at the stage start. The clamp is a port guard.
        anm.set_script(&mut ctx.w.rng, &mut self.start_clock_vm, TIMES_SLOT, 0);
        let clock = crate::pl!(ctx.w).stats.clock;
        self.start_clock_vm.set_sprite(anm, clock.clamp(0, 15));
    }

    // The line VM's strip of the text.anm render target: texture, row width and strip top y in texture pixels.
    #[expect(
        clippy::cast_sign_loss,
        reason = "ftol of the strip's v0 in pixels (10 §6)"
    )]
    fn line_texture(
        anm: &AnmManager,
        rec: &Recorder,
        vm: &AnmVm,
    ) -> Option<(TexId, u32, u32, f32)> {
        let sp = anm.sprite(
            msg::TEXT_SLOT,
            usize::from(vm.sprite_index.max(0).cast_unsigned()),
        )?;
        let t = rec.texture(sp.tex);
        Some((
            sp.tex,
            t.width,
            (sp.uv[1] * t.height as f32).round() as u32,
            sp.uv[0] * t.width as f32 + sp.w,
        ))
    }

    // Text_RenderToVm 0x4663B0 for the queued dialogue lines (10 §6).
    fn flush_texts(ctx: &mut Ctx) {
        // No dialogue = no queued lines.
        let Some(d) = ctx.w.msg.as_deref_mut() else {
            return;
        };
        let size = d.font_size;
        for (line, s, colour) in std::mem::take(&mut d.texts) {
            // Line 2 renders into boss_card's strip (text.anm sprite 2, 10 §5.1).
            let Some((tex, w, y, edge)) = d
                .line_vm(line)
                .and_then(|vm| Self::line_texture(&ctx.w.anm, &ctx.rec, vm))
            else {
                continue;
            };
            // Recorded in calc; it runs at the next submit, before any draw recorded after it.
            ctx.rec.text(TextCmd {
                dst: tex,
                y,
                w,
                size: size as u32,
                text: s.raw,
                key: s.key,
                color: colour,
                x: 0.0,
                edge,
                outline: 0,
                align: Align::Left,
                site: TextSite::Dialogue,
            });
        }
    }

    fn handle_events(&mut self, ctx: &mut Ctx, events: Vec<Event>) {
        for ev in events {
            match ev {
                Event::Plate(n) => set_boss_plate(&ctx.w.anm, &mut ctx.rec, n),
                Event::SpellBase(n) => crate::stg!(ctx.w).spell_first = n,
                Event::Bgm(n) if n < 0 => ctx.w.bgm.stop_bgm(),
                // Msg op 7 (10 §5.1): song-title card = stgNtxt script 3 with sprite n + 3, then preloaded BGM slot n.
                Event::Bgm(n) => {
                    let vm = &mut self.title[3];
                    ctx.w.anm.set_script(&mut ctx.w.rng, vm, TXT_SLOT, 3);
                    vm.set_sprite(&ctx.w.anm, n + 3);
                    ctx.w.bgm.play_slot(table_index(n));
                }
                Event::BgmFade => ctx.w.bgm.fade_bgm(4.0, ctx.w.anm.speed),
                // Msg op 14 (10 §5.1, 13 §3): ScreenEffect type 4, 442 frames to white, draw prio 0x15.
                // Full-repaint counter 0x17CE8CC := 442 (10 §5.1).
                Event::Flash => {
                    ctx.w.effects.screen(4, 442, 0x00FF_FFFF, 0, 0, 0x15);
                    ctx.w.panel.set_repaint(442);
                }
                Event::StageEnd => self.stage_end(ctx),
                // Msg op 11: GM flags & ~0x60 | 0x40 (game-clear trigger).
                Event::HaltFinal => ctx.w.game.set_end_state(2),
                Event::Route(c) => {
                    log::info!("final route choice {c} at frame {}", ctx.w.frame);
                    ctx.w.game.set_route(c);
                }
                Event::Sfx(n) => ctx.w.sound.play(i32::from(n)),
            }
        }
    }

    // Msg op 9 (10 §9): stores the panel values, advances the clock, restarts the stage-clear VM.
    fn stage_end(&mut self, ctx: &mut Ctx) {
        log::info!(
            "stage clear (msg op 9) at frame {} score {}",
            ctx.w.frame,
            crate::pl!(ctx.w).stats.score10
        );
        let g = &mut ctx.w.game;
        g.flags |= gamemgr::F_CLEAR_PANEL;
        let stage = g.stage % 9;
        let p = crate::pl!(ctx.w);
        let shot = p.shot_type;
        let adv = gamemgr::clock_advance(stage, p.stats.time, p.stats.time_threshold);
        // Stage-clear VM impl+0x212C8 (10 §9): stages 1–5 restart ascii.anm script 3 with sprite 0x80 + clock advance;
        // 6A/6B/Ex only clear its ip; interrupt 1 in every case.
        if stage < 6 {
            ctx.w.anm.set_script(
                &mut ctx.w.rng,
                &mut self.clear_vm,
                crate::ascii::ANM_SLOT,
                3,
            );
            self.clear_vm.set_sprite(&ctx.w.anm, 0x80 + adv);
        } else {
            self.clear_vm.script = None;
        }
        self.clear_vm.pending_interrupt = 1;
        // Stages 1–5: loading VM, capture VM, 8 ghosts (I4 := 4k + 3, alpha := 64 − 2k) and the playfield capture.
        if stage < 6 {
            let anm = &mut ctx.w.anm;
            anm.set_script(&mut ctx.w.rng, &mut self.load_vm, LOADING_SLOT, 0);
            anm.set_script(
                &mut ctx.w.rng,
                &mut self.cap_vm,
                crate::ascii::CAPTURE_SLOT,
                1,
            );
            self.ghosts = (0..8)
                .map(|k| {
                    let mut vm = AnmVm::default();
                    anm.set_script(&mut ctx.w.rng, &mut vm, crate::ascii::CAPTURE_SLOT, 2);
                    vm.ivars[4] = 4 * k + 3;
                    vm.color[anmvm::A] = lo8(64 - 2 * k);
                    vm
                })
                .collect();
            self.capture_pending = true;
        }
        let rng = &mut ctx.w.rng;
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
                ctx.w.panel.mark_bombs(2);
                ctx.w.sound.play(crate::sound::CARDGET);
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
    fn clear_bonus(&mut self, g: &Game, p: &mut Player) {
        let Some(c) = &mut self.clear else { return };
        if !std::mem::take(&mut c.pending) {
            return;
        }
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
    pub(crate) fn boss_calc(&mut self, e: &EnemyManager) {
        let (req, target) = (e.g.hud_boss, e.g.hud_bar_target);
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
    // Emit only: mode 0 writes no VM state (05 §9).
    fn emit_msg(d: &Dialogue, anm: &AnmManager, rec: &mut Recorder) {
        if !d.active() {
            return;
        }
        rec.set_viewport(PLAYFIELD);
        for pair in [[0usize, 1], [2, 3]] {
            let (a, b) = (pair[0], pair[1]);
            let order = if d.faces[a].pos[2] >= d.faces[b].pos[2] {
                [a, b]
            } else {
                [b, a]
            };
            for k in order {
                if d.faces[k].script.is_some() {
                    anm.emit_mode(rec, &d.faces[k], 0);
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
            Self::bar_quad(rec, x0, x1, y0, y0 + h, top, bot);
        }
        for vm in d.lines.iter().chain(std::iter::once(&d.boss_card)) {
            if vm.script.is_some() {
                anm.emit_mode(rec, vm, 0);
            }
        }
    }

    // Screen_FillRect3 0x45B490 (12 §5.4, 10 §7): untextured quad at exact D3D coordinates (+0.5 for wgpu).
    fn bar_quad(
        rec: &mut Recorder,
        x0: f32,
        x1: f32,
        y0: f32,
        y1: f32,
        top: [u8; 4],
        bot: [u8; 4],
    ) {
        let uv = [0.0, 0.0];
        let (x0, x1, y0, y1) = (x0 + 0.5, x1 + 0.5, y0 + 0.5, y1 + 0.5);
        let v = [
            Vertex::screen(x0, y0, 0.0, uv, top),
            Vertex::screen(x1, y0, 0.0, uv, top),
            Vertex::screen(x0, y1, 0.0, uv, bot),
            Vertex::screen(x1, y1, 0.0, uv, bot),
        ];
        rec.strip(rec.white(), Blend::Alpha, false, v);
    }

    // 0xRRGGBB with an explicit alpha → RGBA.
    fn rgb(c: u32, a: u8) -> [u8; 4] {
        [(c >> 16) as u8, (c >> 8) as u8, c as u8, a]
    }

    // Hud_DrawBoss 0x43741D (12 §5.4), step: the mode-5 matrices of the op 9 VMs and tiles, then the bar's logic.
    fn boss_step(&mut self, ctx: &mut Ctx) {
        let anm = &ctx.w.anm;
        if self.load_vm.script_index >= 0 {
            for vm in std::iter::once(&mut self.cap_vm).chain(self.ghosts.iter_mut()) {
                anm.prepare(vm, 5);
            }
        }
        if self.tiles_live != 0 {
            for vm in &mut self.tiles {
                anm.prepare(vm, 5);
            }
        }
        if !dialogue_active(ctx) {
            self.bar_step(ctx);
        }
    }

    // Hud_DrawBoss emit: unguarded VMs, the guarded bar part, the stage-clear VM last.
    fn emit_boss(&self, ctx: &Ctx, rec: &mut Recorder) {
        let anm = &ctx.w.anm;
        rec.set_viewport(PLAYFIELD);
        // Stage title VMs, the stage-start clock, then the night clock with Draw2DRot (12 §5.1, §5.7).
        for vm in self
            .title
            .iter()
            .chain(std::iter::once(&self.start_clock_vm))
        {
            if vm.script.is_some() {
                anm.emit_mode(rec, vm, 0);
            }
        }
        if self.night_vm.script.is_some() {
            anm.emit_mode(rec, &self.night_vm, 2);
        }
        // Op 9 VMs: loading picture Draw2DSnap, capture and ghosts Draw3DQuad; impl+0x3A1C is never started (10 §9).
        if self.load_vm.script_index >= 0 {
            anm.emit_mode(rec, &self.load_vm, 0);
            for vm in std::iter::once(&self.cap_vm).chain(self.ghosts.iter()) {
                anm.emit_mode(rec, vm, 5);
            }
        }
        if self.tiles_live != 0 {
            for vm in &self.tiles {
                anm.emit_mode(rec, vm, 5);
            }
        }
        if !dialogue_active(ctx) {
            self.emit_bar(ctx, rec);
        }
        if self.clear_vm.script.is_some() {
            anm.emit_mode(rec, &self.clear_vm, 0);
        }
    }

    // Guard of the bar part of Hud_DrawBoss (12 §5.4): no dialogue (caller), +0x2C (read live) + bar state ≠ 0.
    fn bar_shown(&self, e: &EnemyManager) -> bool {
        e.g.hud_boss || self.bar_state != 0
    }

    // Bar part, step: the timer string, its sounds and Popup_LargeStatic (logic, also with emit off).
    fn bar_step(&mut self, ctx: &mut Ctx) {
        // The HUD runs only inside a stage, which has its enemy manager; without one there is no boss to show.
        let Some(e) = ctx.w.enemies.as_deref() else {
            return;
        };
        if !self.bar_shown(e) {
            return;
        }
        let a = lo8(self.bar_alpha);
        // Timer (12 §5.4): every guarded draw, HUD +0x24 as stored by Enemy_CheckTimeout, shown as min(·, 99).
        let raw = e.g.hud_timer;
        let v = raw.min(99);
        let [tx, ty] = TIMER_POS;
        ctx.w
            .ascii
            .push(Text::new(tx, ty, fmt_02d(v)).color(Self::rgb(timer_colour(raw), a)));
        // Sound on a change of the raw value: v < 3 se_timeout2 (0x26), else v < 10 se_timeout (0x1D).
        if self.timer_last != raw {
            if v < 3 {
                ctx.w.sound.play(crate::sound::TIMEOUT2);
            } else if v < 10 {
                ctx.w.sound.play(crate::sound::TIMEOUT);
            }
        }
        self.timer_last = raw;
        // Popup_LargeStatic (12 §5.4, 13 §4): re-issued every guarded draw, "+0" without children too.
        let g = &ctx.w.game;
        if let Some(i) = e.boss(0)
            && g.paused == 0
            && g.game_over == 0
            && !crate::pl!(ctx.w).freeze_bullets(g.flags)
        {
            ctx.w.overlays.popup_large_static(
                [2.0, 29.0],
                e.chain_count(i),
                e.enemies[i].children,
                0xFFF0_F00F,
            );
        }
    }

    // Bar part, emit: fill, ECL markers, remaining-bars gauge, the timer frame VM.
    fn emit_bar(&self, ctx: &Ctx, rec: &mut Recorder) {
        let Some(e) = ctx.w.enemies.as_deref() else {
            return;
        };
        if !self.bar_shown(e) {
            return;
        }
        let a = lo8(self.bar_alpha);
        rec.set_viewport(PLAYFIELD);
        let disp = self.bar_disp;
        Self::bar_quad(
            rec,
            BAR_X,
            BAR_X + BAR_W * disp,
            BAR_Y0,
            BAR_Y1,
            Self::rgb(BAR_TOP, a),
            Self::rgb(BAR_BOT, a),
        );
        // ECL op 158 markers: drawn when end ≠ 0 and start < displayed; bottom colour (c >> 2) & 0x3F3F3F.
        for (end, start, c) in e.g.hud_markers {
            if end != 0.0 && start < disp {
                Self::bar_quad(
                    rec,
                    BAR_X + BAR_W * start,
                    BAR_X + BAR_W * end.min(disp),
                    BAR_Y0,
                    BAR_Y1,
                    Self::rgb(c, a),
                    Self::rgb((c >> 2) & 0x003F_3F3F, a),
                );
            }
        }
        // Remaining-bars gauge (+0x20, ECL op 148) between x 35 and 61.
        let n = e.g.hud_bars.max(0);
        let gap = if n < 6 { 2.0 } else { 1.0 };
        for i in 0..n {
            let x0 = GAUGE_X + GAUGE_W * i as f32 / n as f32;
            let x1 = GAUGE_X + GAUGE_W * (i + 1) as f32 / n as f32 - gap;
            let c = Self::rgb(BAR_TOP - (i.cast_unsigned() * 255 / 9), a);
            Self::bar_quad(rec, x0, x1, BAR_Y0, BAR_Y1, c, Self::rgb(GAUGE_BOT, a));
        }
        ctx.w.anm.emit_mode(rec, &self.front[12], 0);
    }

    // Hud_DrawPopups 0x438A89 (12 §5.5): every string with the clip flag (playfield viewport); bonus / message
    // popups at the slide x of the last Hud_Update.
    fn draw_popups(&mut self, ctx: &mut Ctx) {
        for p in self.popups.iter().flatten() {
            match p.kind {
                Popup::Bonus(n) => ctx.w.ascii.push(
                    Text::new(p.x, POPUP_BONUS_Y, format!(" BONUS {n:8}"))
                        .argb(POPUP_BONUS)
                        .clip(),
                ),
                Popup::Message(t, v) => {
                    let (s, small) = match t {
                        1 => ("Full Power Mode!".to_string(), false),
                        // Types 2–4 have no caller and are never shown (12 §5.5 Hud_ShowMessage).
                        2 => ("Supernatural Border!!".to_string(), true),
                        3 => ("CherryPoint Max!".to_string(), false),
                        4 => (format!("Border Bonus {v:7}"), true),
                        5 => ("Spell Bonus Failed".to_string(), true),
                        _ => ("Last Spell Failed".to_string(), true),
                    };
                    // Types 2/4–6 set Ascii_SetAdvance(11) but reset it to 13 before the draw pass reads it
                    // (12 §2.9), so only the 0.9 x scale narrows them.
                    let tx = Text::new(p.x, POPUP_MSG_Y, s).clip();
                    ctx.w.ascii.push(if small {
                        tx.scale_xy(0.9, 1.0).argb(POPUP_MSG_SMALL)
                    } else {
                        tx.argb(POPUP_MSG)
                    });
                }
                // "Spell Card Bonus!" at x (384 − 17·14)/2 + 32 = 105 (14 px per glyph, not the 13 px advance), y 80;
                // "+%d" 2× at y 96, x (384 − len·28)/2 + 32.
                Popup::SpellBonus(n) => {
                    let s = "Spell Card Bonus!";
                    let x = ((384 - len_i32(s) * 14) / 2 + 32) as f32;
                    ctx.w
                        .ascii
                        .push(Text::new(x, POPUP_SPELL_Y, s).argb(POPUP_SPELL).clip());
                    let v = format!("+{n}");
                    let x = ((384 - len_i32(&v) * 28) / 2 + 32) as f32;
                    ctx.w.ascii.push(
                        Text::new(x, POPUP_SPELL_VALUE_Y, v)
                            .scale(2.0)
                            .argb(POPUP_SPELL_VALUE)
                            .clip(),
                    );
                }
            }
        }
    }

    // Hud_DrawClearPanel 0x43826B (12 §5.6): ASCII strings only, full-screen (clip flag 0); pen from (120, 96).
    fn draw_clear(&mut self, ctx: &mut Ctx) {
        let Some(c) = &self.clear else { return };
        // The clear panel exists only after msg op 9 of a stage with its player.
        let Some(p) = ctx.w.player.as_deref() else {
            return;
        };
        let (g, st, q) = (&ctx.w.game, &p.stats, &mut ctx.w.ascii);
        let mut y = clear_scores(q, c, st);
        if c.stage >= 6 && !g.practice() && !g.practice_replay() {
            y = clear_final(q, c, st, y);
        }
        y = clear_rank(q, g, self.clear_total, y);
        if c.stage < 6 {
            clear_clock(q, c, y);
        }
    }

    // Hud_DrawPanel (12 §5.3), step: the panel strings and queue colour, the lives / bombs VM positions, the label latch.
    fn panel_step(&mut self, ctx: &mut Ctx, st: &Stats) {
        let q = &mut ctx.w.ascii;
        // Score row (y 56) queued before the hiscore row (y 40) (12 §5.3 step 6). The panel strings take the current
        // queue colour, so a "Press Shot Button" alpha leaks into them on the first gameplay frame (12 §1).
        q.push(q.text(
            PANEL_X,
            SCORE_Y,
            format!("{:09}", st.score_disp.min(999_999_999)),
        ));
        q.push(q.text(CONT_X, SCORE_Y, format!("{}", st.continues.min(9))));
        q.push(q.text(
            PANEL_X,
            HISCORE_Y,
            format!("{:09}", st.hiscore.min(999_999_999)),
        ));
        q.push(q.text(CONT_X, HISCORE_Y, format!("{}", st.hiscore_cont.min(9))));
        // The row VMs keep the position of their last stamp; the emit draws every stamp from a copy.
        for (row, vm, n) in Self::stock_rows(st) {
            if n > 0 {
                self.front[vm].pos = Self::stock_pos(row, n - 1);
            }
        }
        // Step 4 label: the original draws it only when it repaints the lives or bombs row (field ≠ 0 or full
        // repaint) and it stays in the back buffer, so the port latches the condition on those frames.
        let vm13 = self.front.get(13).is_some_and(|v| v.ip.is_some());
        // No enemy manager = no card running.
        let cond = ctx.w.game.flags >> 7 & 3 == 1
            && ctx
                .w
                .enemies
                .as_deref()
                .is_some_and(|e| e.g.spell.is_some());
        let (pn, latch) = label_step(
            ctx.w.panel,
            vm13,
            cond,
            self.label_latch,
            ctx.w.game.paused != 0,
        );
        ctx.w.panel = pn;
        self.label_latch = latch;
        let q = &mut ctx.w.ascii;
        q.push(q.text(PANEL_X, GRAZE_Y, format!("{}", st.graze_total)));
        for t in ratio_row(POINT_Y, st.point_items2, st.next_extend, q.color) {
            q.push(t);
        }
        let col = if st.time >= st.time_threshold {
            [0xFF, 0xF0, 0xC0, 0xFF]
        } else {
            q.color
        };
        for t in ratio_row(TIME_Y, st.time, st.time_threshold, col) {
            q.push(t);
        }
        // Ascii_SetColor(0xFFFFFFFF) ends the time-row block (12 §5.3 step 6); the port draws that row every frame.
        q.color = [255; 4];
        // Step 7: the power text is the last panel string, so it takes the colour left by step 6 (12 §5.3).
        let pw = st.power.trunc();
        q.push(q.text(
            PANEL_X,
            POWER_Y,
            if pw >= 128.0 {
                "MAX".to_string()
            } else {
                format!("{}", pw as i32)
            },
        ));
    }

    // Lives (front VM 10, y 88) and bombs (VM 11, y 104) rows: up to 8 stamps 16 px apart (12 §5.3).
    fn stock_rows(st: &Stats) -> [(f32, usize, i32); 2] {
        [
            (LIVES_Y, 10, (st.lives as i32).clamp(0, STOCK_MAX)),
            (BOMBS_Y, 11, (st.bombs as i32).clamp(0, STOCK_MAX)),
        ]
    }

    fn stock_pos(row: f32, i: i32) -> [f32; 3] {
        [PANEL_X + STOCK_DX * i as f32, row, STOCK_Z]
    }

    // Hud_DrawPanel emit; flushes and sets the full-screen viewport first.
    fn emit_panel(&self, anm: &AnmManager, rec: &mut Recorder, st: &Stats) {
        rec.set_viewport(FULLSCREEN);
        // Tiles (scripts 13–15) keep z-write on; depths 0.49 / 0.48 / 0.46 (12 §5.3).
        let spr = |rec: &mut Recorder, i: usize, x: f32, y: f32, z: f32| {
            if let Some(s) = anm.sprite(FRONT_SLOT, i) {
                let [u0, v0, u1, v1] = s.uv;
                // Draw2DSnap → EmitQuad: the global tint applies (12 §5.3, 05 §12.1); the gauge quad stays raw.
                let c = anm.tinted([255; 4]);
                let v = |x: f32, y: f32, u: f32, vv: f32| Vertex::screen(x, y, z, [u, vv], c);
                rec.strip(
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
        for y in [HISCORE_Y, SCORE_Y] {
            spr(rec, 15, 480.0, y, TILE_Z_TOP);
        }
        for y in [LIVES_Y, BOMBS_Y, POWER_Y, GRAZE_Y, POINT_Y, TIME_Y] {
            spr(rec, 15, 480.0, y, TILE_Z_ROW);
        }
        // Always: the corner under the fps text.
        spr(rec, 15, 512.0, 464.0, TILE_Z_ROW);
        // Full repaint (cfg bit 12, tile VM 13 still running = first 200 frames, or counter 0x17CE8CC ≠ 0) and the
        // dirty rows are not tracked: the port clears the screen every frame, so it repaints in full every frame.
        for y in (0..480).step_by(32) {
            spr(rec, 13, 0.0, y as f32, TILE_Z_TOP);
        }
        for x in (416..640).step_by(32) {
            for y in (16..480).step_by(32) {
                spr(rec, 13, x as f32, y as f32, TILE_Z_TOP);
            }
        }
        for x in (0..640).step_by(128) {
            spr(rec, 14, x as f32, 0.0, TILE_Z_TOP);
            spr(rec, 14, x as f32, 464.0, TILE_Z_TOP);
        }
        for vm in self.front.iter().take(10) {
            anm.emit_mode(rec, vm, if vm.script_index == 1 { 2 } else { 0 });
        }
        anm.emit_mode(rec, &self.diff_vm, 0);
        for (row, vm, n) in Self::stock_rows(st) {
            for i in 0..n {
                let mut v = self.front[vm].clone();
                v.pos = Self::stock_pos(row, i);
                anm.emit_mode(rec, &v, 0);
            }
        }
        if self.label_latch {
            anm.emit_mode(rec, &self.label_vm, 0);
        }
        // Strip 14 along y = 464 for x 32..368 every frame (12 §5.3).
        for x in [32.0, 160.0, 288.0] {
            spr(rec, 14, x, 464.0, TILE_Z_TOP);
        }
        // Power gauge x 488 … 488 + power, y 136..152, left 0xE0E0E0FF / right 0x80E0E0FF (12 §5.3).
        let pw = st.power.trunc();
        let uv = [0.0, 0.0];
        let (l, r) = ([0xE0, 0xE0, 0xFF, 0xE0], [0xE0, 0xE0, 0xFF, 0x80]);
        rec.strip(
            rec.white(),
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
    }
}

// Msg_IsActive on the HUD's dialogue (10 §1).
fn dialogue_active(ctx: &Ctx) -> bool {
    ctx.w.msg.as_deref().is_some_and(Dialogue::active)
}

// The HUD lent out of `ctx.w.hud` for a step that also takes the `Ctx` (nothing it calls reads `ctx.w.hud`).
// None = no HUD (Hud::register sets it before adding the nodes): the node does nothing.
fn with_hud<R>(ctx: &mut Ctx, f: impl FnOnce(&mut Hud, &mut Ctx) -> R) -> Option<R> {
    let mut h = ctx.w.hud.take()?;
    let r = f(&mut h, ctx);
    ctx.w.hud = Some(h);
    Some(r)
}

// Chain nodes of `ctx.w.hud` (calc prio 15, draw prio 17): stateless forwarder.
pub struct HudTask;

impl Task<Ctx> for HudTask {
    fn on_register(&mut self, ctx: &mut Ctx, chain: Chain, tag: u8) -> Result<()> {
        with_hud(ctx, |h, ctx| h.on_register(ctx, chain, tag)).unwrap_or(Ok(()))
    }

    fn on_remove(&mut self, ctx: &mut Ctx, chain: Chain, tag: u8) {
        with_hud(ctx, |h, ctx| h.on_remove(ctx, chain, tag));
    }

    fn calc(&mut self, ctx: &mut Ctx, tag: u8) -> Ret {
        with_hud(ctx, |h, ctx| h.calc(ctx, tag)).unwrap_or(Ret::Next)
    }

    fn draw_step(&mut self, ctx: &mut Ctx, tag: u8) -> Ret {
        with_hud(ctx, |h, ctx| h.draw_step(ctx, tag)).unwrap_or(Ret::Next)
    }

    fn emit(&self, ctx: &Ctx, rec: &mut Recorder, tag: u8) {
        if let Some(h) = ctx.w.hud.as_deref() {
            h.emit(ctx, rec, tag);
        }
    }
}

impl Hud {
    fn on_register(&mut self, ctx: &mut Ctx, chain: Chain, _: u8) -> Result<()> {
        if chain == Chain::Calc {
            self.load(ctx)?;
        }
        Ok(())
    }

    fn on_remove(&mut self, ctx: &mut Ctx, chain: Chain, _: u8) {
        if chain == Chain::Calc {
            ctx.w.hud_kept = Some(Kept {
                front: std::mem::take(&mut self.front),
                diff_vm: std::mem::take(&mut self.diff_vm),
                label_vm: std::mem::take(&mut self.label_vm),
                clear_total: self.clear_total,
                // Without an enemy manager the boss timer is 0 (Hud_Register's fresh-start value).
                timer: ctx.w.enemies.as_deref().map_or(0, |e| e.g.hud_timer),
                timer_last: self.timer_last,
            });
            for s in [FRONT_SLOT, TIMES_SLOT, TXT_SLOT, LOADING_SLOT]
                .into_iter()
                .chain(msg::FACE_SLOTS)
            {
                ctx.w.anm.unload(&mut ctx.rec, s);
            }
        }
    }

    // Hud_Calc 0x4338CA (12 §5.1b): Hud_Update, then Msg_Run, then the frame counter. Hud_Update runs first, so what
    // a msg op sets this frame (op 9 bonus, trigger, clock animation) is acted on the next frame.
    fn calc(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        // AnmManager_ApplyPendingCaptures runs right after the Present that follows op 9's calc (12 §3); the port
        // copies from that presented frame at the next calc.
        if std::mem::take(&mut self.capture_pending) {
            crate::ascii::apply_capture(&ctx.w.anm, &mut ctx.rec, &self.cap_vm);
        }
        let inp = ctx.w.input.game_view();
        self.ecl_requests(ctx);
        // Hud_Update 0x435900, steps 1–7 of 12 §5.1b.
        if !dialogue_active(ctx)
            && let Some(e) = ctx.w.enemies.as_deref()
        {
            self.boss_calc(e);
        }
        self.run_vms(ctx);
        let gs = ctx.w.anm.speed;
        self.popup_step(&mut crate::pl!(ctx.w).stats, &mut ctx.w.panel, gs);
        self.clear_bonus(&ctx.w.game, crate::pl!(ctx.w));
        self.clear_calc(&mut ctx.w.game, &inp);
        self.msg_run(ctx, &inp);
        // Step 3: SKIP held raises 0x17CE8CC to 8 (12 §5.1b).
        if inp.down(crate::input::SKIP) && ctx.w.panel.repaint < 8 {
            ctx.w.panel.set_repaint(8);
        }
        crate::stg!(ctx.w).hud_cover = hud_cover(&self.load_vm);
        self.frame += 1;
        Ret::Next
    }

    // HUD requests queued by ECL ops this frame (ECL runs before the HUD).
    fn ecl_requests(&mut self, ctx: &mut Ctx) {
        let reqs = std::mem::take(&mut crate::em!(ctx.w).g.hud_requests);
        let clock = crate::pl!(ctx.w).stats.clock;
        for r in reqs {
            match r {
                // ECL 179 / 181 (12 §5.7, 07 §7): night clock script 2 with sprite = clock; 181 interrupts 2 at 12,
                // else 1; 180 hides it with alpha := 0.
                179 => {
                    ctx.w
                        .anm
                        .set_script(&mut ctx.w.rng, &mut self.night_vm, TIMES_SLOT, 2);
                    self.night_vm.set_sprite(&ctx.w.anm, clock.clamp(0, 15));
                }
                180 => self.night_vm.color[anmvm::A] = 0,
                // ECL op 148: progress counter 0x164D30C += 1800 (10 §8.3, end panel 16 §2.5).
                148 => crate::pl!(ctx.w).stats.rec.progress += 1800,
                181 => {
                    self.night_vm.set_sprite(&ctx.w.anm, clock.clamp(0, 15));
                    self.night_vm.pending_interrupt = if clock >= 12 { 2 } else { 1 };
                }
                _ => {}
            }
        }
    }

    // Hud_Update step 1.x VM runs (12 §5.1b): front VMs, stage-title VMs, the stage-start clock, the stage-clear VM,
    // the night clock and its fade, the label and difficulty VMs, the op 9 VMs, the stage-start tiles.
    fn run_vms(&mut self, ctx: &mut Ctx) {
        let anm = &mut ctx.w.anm;
        for vm in self.front.iter_mut().chain(self.title.iter_mut()) {
            if vm.script.is_some() {
                vm.run(anm, &mut ctx.w.rng);
            }
        }
        // The clock VM ticks only while stage-title VM 0 has alpha ≠ 0, never in spell practice (12 §5.1).
        let title_on = self.title.first().is_some_and(|v| v.color[anmvm::A] != 0);
        if self.start_clock_vm.script.is_some() && title_on && !ctx.w.game.spell_practice() {
            self.start_clock_vm.run(anm, &mut ctx.w.rng);
        }
        for vm in [&mut self.clear_vm, &mut self.night_vm] {
            if vm.script.is_some() {
                vm.run(anm, &mut ctx.w.rng);
            }
        }
        self.night_fade(crate::pl!(ctx.w).pos);
        // Last-spell label VM impl+0x5484, then the difficulty name VM impl+0x22E14 (12 §5.1b step 1.4).
        if self.label_vm.script.is_some() {
            self.label_vm.run(&mut ctx.w.anm, &mut ctx.w.rng);
        }
        if self.diff_vm.script.is_some() {
            self.diff_vm.run(&mut ctx.w.anm, &mut ctx.w.rng);
        }
        if self.load_vm.script_index >= 0 {
            let anm = &mut ctx.w.anm;
            for vm in [&mut self.load_vm, &mut self.cap_vm]
                .into_iter()
                .chain(self.ghosts.iter_mut())
            {
                vm.run(anm, &mut ctx.w.rng);
            }
        }
        if self.tiles_live != 0 {
            for vm in &mut self.tiles {
                // +0x21810 = tiles still running (12 §5.7): a finished VM returns 1 every later call, so decrementing once matches.
                if vm.ip.is_some() && vm.run(&mut ctx.w.anm, &mut ctx.w.rng) {
                    self.tiles_live -= 1;
                }
            }
        }
    }

    // Player-overlap fade of the night clock (12 §5.7): while alpha ≠ 0, −4 down to 0x40 with the player at
    // x ≥ 64, y < 128, else +4 up to 0xFF (≥ 0xFC snaps to 0xFF).
    fn night_fade(&mut self, p: [f32; 3]) {
        let a = self.night_vm.color[anmvm::A];
        if a != 0 {
            self.night_vm.color[anmvm::A] = if p[0] >= 64.0 && p[1] < 128.0 {
                if a > 0x40 { (a - 4).max(0x40) } else { a }
            } else if a >= 0xFC {
                0xFF
            } else {
                a + 4
            };
        }
    }

    // Msg_Run (10 §5), including op 9, then its playfield side effects, the item attraction, the text renders and
    // the dialogue events.
    fn msg_run(&mut self, ctx: &mut Ctx, inp: &crate::input::Input) {
        let dying = crate::pl!(ctx.w).state == 2;
        // No dialogue = nothing collected, no events, no side effects.
        let (collect, events, side) = if let Some(d) = ctx.w.msg.as_deref_mut() {
            d.info.clock = crate::pl!(ctx.w).stats.clock;
            let c = d.run(&mut ctx.w.anm, &mut ctx.w.rng, inp, dying);
            ctx.w.game.dialogue_skippable = d.skippable_shown();
            (
                c,
                std::mem::take(&mut d.events),
                std::mem::take(&mut d.side_effects),
            )
        } else {
            (false, Vec::new(), false)
        };
        // Msg_Start playfield side effects (10 §4 step 6): cancel bullets, kill killable enemies.
        if side {
            crate::em!(ctx.w).msg_side_effects(&mut ctx.w.anm, &mut crate::sys!(ctx.w));
        }
        // All items auto-collect while a dialogue runs and the player is not dying (10 §5).
        if collect {
            crate::bm!(ctx.w).items.attract_all();
        }
        Self::flush_texts(ctx);
        self.handle_events(ctx, events);
        Self::flush_texts(ctx);
    }

    // Hud_Draw (12 §5.3): clear panel strings, Msg_Draw, Hud_DrawBoss, Hud_DrawPanel, Hud_DrawPopups. Everything
    // before Hud_DrawPanel inherits the playfield viewport set by Stage_DrawLow.
    // Step: ASCII queue strings and colour, timer sounds / popup, label latch, VM positions and mode-5 matrices.
    fn draw_step(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        if ctx.w.game.loading != 0 {
            return Ret::Next;
        }
        self.draw_clear(ctx);
        self.boss_step(ctx);
        let st = crate::pl!(ctx.w).stats.clone();
        self.panel_step(ctx, &st);
        self.draw_popups(ctx);
        Ret::Next
    }

    fn emit(&self, ctx: &Ctx, rec: &mut Recorder, _: u8) {
        if ctx.w.game.loading != 0 {
            return;
        }
        rec.set_viewport(PLAYFIELD);
        if let Some(d) = ctx.w.msg.as_deref() {
            Self::emit_msg(d, &ctx.w.anm, rec);
        }
        self.emit_boss(ctx, rec);
        if let Some(p) = ctx.w.player.as_deref() {
            self.emit_panel(&ctx.w.anm, rec, &p.stats);
        }
    }
}

// One clear-panel line at the pen x (12 §5.6).
fn clear_line(q: &mut crate::ascii::Queue, y: f32, s: String, col: u32) {
    q.push(Text::new(CLEAR_X, y, s).argb(col));
}

// Clear panel title, the four score rows and the two percentage rows; returns the pen y (12 §5.6).
fn clear_scores(q: &mut crate::ascii::Queue, c: &ClearPanel, st: &Stats) -> f32 {
    let mut y = CLEAR_Y;
    let title = if c.stage < 6 {
        "Stage Clear"
    } else {
        "All Clear!"
    };
    // 6B / Extra lower the pen by 16 before the title: the whole panel starts at y 80.
    if c.stage > 6 {
        y -= 16.0;
    }
    clear_line(q, y, title.into(), CLEAR_TITLE);
    y += CLEAR_GAP;
    clear_line(q, y, format!("Clear = {:8}0", c.clear), CLEAR_WHITE);
    y += CLEAR_DY;
    clear_line(
        q,
        y,
        format!("Point = {:8}0", c.points.wrapping_mul(5000)),
        CLEAR_POINT,
    );
    y += CLEAR_DY;
    clear_line(
        q,
        y,
        format!("Graze = {:8}0", c.graze.wrapping_mul(50)),
        CLEAR_ROW,
    );
    y += CLEAR_DY;
    clear_line(
        q,
        y,
        format!("Time  = {:8}0", c.time.wrapping_mul(100)),
        CLEAR_ROW,
    );
    // Integer percentages (h·100)/f and ((h·10000)/f) % 100, truncated.
    // f = 0 is an unguarded idiv in the original, unreachable (12 §5.6); the port divides by 1.
    let f = st.stage_frames.max(1).cast_signed();
    let pct = |h: u32| {
        let h = h.cast_signed();
        (h.wrapping_mul(100) / f, h.wrapping_mul(10000) / f % 100)
    };
    let (a, b) = pct(st.human_frames);
    y += CLEAR_GAP;
    clear_line(q, y, format!("over-80% = {a:3}.{b:02}%"), CLEAR_ROW);
    let (a, b) = pct(st.youkai_frames);
    y += CLEAR_DY;
    clear_line(q, y, format!("over 80% = {a:3}.{b:02}%"), CLEAR_ROW);
    y
}

// Stages 6A / 6B / Ex outside practice: lives and bombs rows, in 6B the last time and night bonus (12 §5.6).
fn clear_final(q: &mut crate::ascii::Queue, c: &ClearPanel, st: &Stats, mut y: f32) -> f32 {
    let (lv, bm) = (
        (st.lives as i32).wrapping_mul(2_500_000),
        (st.bombs as i32).wrapping_mul(500_000),
    );
    y += CLEAR_DY;
    clear_line(q, y, format!("Player ={lv:8}0"), CLEAR_FINAL);
    y += CLEAR_DY;
    clear_line(q, y, format!("Bomb   = {bm:7}0"), CLEAR_FINAL);
    if c.stage == 7 {
        let m = c.to;
        y += CLEAR_DY;
        clear_line(
            q,
            y,
            format!("Last Time = {:2}:{:02}", (m / 60) % 12, m % 60),
            CLEAR_FINAL,
        );
        y += CLEAR_DY;
        clear_line(q, y, "Night Bonus".into(), CLEAR_FINAL);
        y += CLEAR_DY;
        let night = (12 - st.clock).wrapping_mul(2_000_000);
        clear_line(q, y, format!("        {night:8}0"), CLEAR_FINAL);
    }
    y
}

// Rank line, the lives-option penalty line and the total (12 §5.6).
fn clear_rank(q: &mut crate::ascii::Queue, g: &Game, total: i32, mut y: f32) -> f32 {
    y += CLEAR_GAP;
    let rank = RANK_TEXT
        .get(table_index(g.difficulty))
        .copied()
        .unwrap_or("");
    clear_line(q, y, rank.into(), CLEAR_RANK);
    if g.difficulty < 4 && !g.practice() {
        y += CLEAR_DY;
        let pen = match g.game_lives() {
            3 => Some("Player Penalty*0.5"),
            4 => Some("Player Penalty*0.2"),
            5 => Some("Player Penalty*0.1"),
            6 => Some("Player Penalty*0.05"),
            _ => None,
        };
        if let Some(t) = pen {
            clear_line(q, y, t.into(), CLEAR_RANK);
            y += CLEAR_DY;
        }
    }
    y += CLEAR_DY;
    clear_line(q, y, format!("Total = {total:8}0"), CLEAR_WHITE);
    y
}

// Stages 1–5: night clock before >> after (12 §5.6). Prefix "PM" while minutes/60 < 12, else "AM"; hours
// (minutes/60) % 12.
fn clear_clock(q: &mut crate::ascii::Queue, c: &ClearPanel, mut y: f32) {
    let clock = |m: i32| {
        format!(
            "{}{:2}:{:02}",
            if m / 60 < 12 { "PM" } else { "AM" },
            (m / 60) % 12,
            m % 60
        )
    };
    y += 40.0;
    q.push(Text::new(CLEAR_X, y, clock(c.from)).argb(0xFFDF_DFDF));
    q.push(Text::new(CLEAR_ARROW_X, y, ">>").argb(0xFFAF_AFAF));
    q.push(Text::new(CLEAR_TO_X, y, clock(c.shown)).argb(0xFFFF_8F8F));
}

// A short ASCII string's length as the original's int (popup centring, 12 §5.5).
#[expect(
    clippy::cast_possible_wrap,
    reason = "popup strings are a few bytes long"
)]
fn len_i32(s: &str) -> i32 {
    s.len() as i32
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
        alpha = if alpha.cast_unsigned() < 0xFC {
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

// Point / time row (12 §5.3 step 6): value, "/" at scale (0.5, 1) (8 px glyph) 13 px per digit later, the second
// value 6 px after the "/".
fn ratio_row(
    y: f32,
    a: impl std::fmt::Display,
    b: impl std::fmt::Display,
    col: [u8; 4],
) -> [Text; 3] {
    let a = a.to_string();
    let x = PANEL_X + 13.0 * a.len() as f32;
    [
        Text::new(PANEL_X, y, a).color(col),
        Text::new(x, y, "/").scale_xy(0.5, 1.0).color(col),
        Text::new(x + 6.0, y, b.to_string()).color(col),
    ]
}

// Timer colour from the unclamped +0x24 (12 §5.4).
fn timer_colour(raw: i32) -> u32 {
    match raw {
        20.. => 0x00A0_D0FF,
        10..=19 => 0x00A0_80FF,
        5..=9 => 0x00E0_80C0,
        _ => 0x00FF_4040,
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
        3 if g.is_practice_card_in(0xD8, 0xDD) => 0x1A + table_index(i32::from(g.card - 0xD8)),
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
#[expect(clippy::cast_sign_loss, reason = "ftol of sprite uv / size in pixels")]
pub fn set_boss_plate(anm: &AnmManager, rec: &mut Recorder, n: usize) {
    if let (Some(&src), Some(&dst)) = (anm.sprite(FRONT_SLOT, n), anm.sprite(FRONT_SLOT, 10)) {
        let px = |s: &crate::anmmgr::SlotSprite, rec: &Recorder| {
            let t = rec.texture(s.tex);
            [
                (s.uv[0] * t.width as f32).round() as u32,
                (s.uv[1] * t.height as f32).round() as u32,
            ]
        };
        let (a, b) = (px(&src, rec), px(&dst, rec));
        rec.copy_texture(
            src.tex,
            a,
            dst.tex,
            b,
            [src.w.min(dst.w) as u32, src.h.min(dst.h) as u32],
        );
    }
}

// Msg_Start's route byte during playback (GM flag bit 3): replay header byte +0x11C (10 §4 step 3).
fn replay_route(g: &crate::gamemgr::Game) -> Option<u8> {
    if g.replay() {
        g.rpy
            .as_ref()
            .map(th08_formats::replay::Replay::clear_status)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 12 §5.3 step 4: the label latch changes only on lives/bombs repaint frames (field ≠ 0 or full repaint).
    #[test]
    fn last_spell_label_latch() {
        let p = |lives, bombs, repaint| Panel {
            lives,
            bombs,
            repaint,
        };
        // Spell_Start fields 3: latched for the next 3 frames, then kept when the condition changes.
        let (pn, l) = label_step(p(3, 3, 0), false, true, false, false);
        assert!(l && (pn.lives, pn.bombs) == (2, 2));
        let (pn, l) = label_step(pn, false, true, l, false);
        let (pn, l) = label_step(pn, false, true, l, false);
        assert!(l && (pn.lives, pn.bombs) == (0, 0));
        let (_, l2) = label_step(pn, false, false, l, false);
        assert!(l2);
        // A full repaint (counter or VM 13) re-evaluates; the counter does not tick while paused.
        let (pn, l) = label_step(p(0, 0, 2), false, false, true, true);
        assert!(!l && pn.repaint == 2);
        let (_, l) = label_step(p(0, 0, 0), true, true, false, false);
        assert!(l);
    }

    // 10 §4 step 3: the route byte +0x11C is offered only during replay playback.
    #[test]
    fn replay_route_from_header() {
        let mut g = crate::gamemgr::Game::default();
        let mut r = crate::replay::Replay::default();
        r.info[0x11C - 0x68] = 2;
        g.rpy = Some(r);
        let f = g.flags;
        g.flags = f & !crate::gamemgr::F_REPLAY;
        assert_eq!(replay_route(&g), None);
        g.flags = f | crate::gamemgr::F_REPLAY;
        assert_eq!(replay_route(&g), Some(2));
        g.flags = f;
    }

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
        while p.update(1.0) {
            xs.push(p.x);
        }
        // Drawn for 180 frames: 416 on the first, 405.6 on the second, 104 from frame 31.
        assert_eq!(xs.len(), 180);
        assert_eq!(xs[0], 416.0);
        assert_eq!(xs[1], 1.0 * -312.0 / 30.0 + 416.0);
        assert_eq!(xs[30], 104.0);
        let mut b = ActivePopup::show(Popup::Bonus(5));
        assert_eq!(
            std::iter::from_fn(|| b.update(1.0).then_some(())).count(),
            250
        );
        // At speed 0.5 the slide moves every frame (GetF) and the popup lives 500 frames.
        let mut b = ActivePopup::show(Popup::Bonus(5));
        let mut xs = Vec::new();
        while b.update(0.5) {
            xs.push(b.x);
        }
        assert_eq!(xs.len(), 500);
        assert_eq!(xs[1], 0.5f32 * -312.0 / 30.0 + 416.0);
        assert_eq!(xs[60], 104.0);
    }

    // 12 §5.1a: 14 × 12 tiles, I4 = r + 2c, pos (32c + 15.5, 32r + 15.5), uv offset (32c/512, 32r/512).
    #[test]
    fn stage_start_tile_layout() {
        let mut rng = crate::rng::Rng::default();
        let tiles = stage_start_tiles(&mut AnmManager::default(), &mut rng);
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

    // 12 §5.3 step 6: "/" at scale (0.5, 1) after 13 px per digit, second value +6 px.
    #[test]
    fn ratio_row_slash_half_width() {
        let [a, sl, b] = ratio_row(184.0, 123, 400, [0xFF, 0xF0, 0xC0, 0xFF]);
        assert_eq!((a.x, a.s.as_str()), (488.0, "123"));
        assert_eq!((sl.x, sl.y, sl.scale, sl.scale_y), (527.0, 184.0, 0.5, 1.0));
        assert_eq!((b.x, b.s.as_str()), (533.0, "400"));
        assert_eq!(b.color, sl.color);
    }

    // 12 §5.4: %.2d and the colour from the unclamped value.
    #[test]
    fn boss_timer_text() {
        assert_eq!(fmt_02d(7), "07");
        assert_eq!(fmt_02d(99), "99");
        assert_eq!(fmt_02d(-1), "-01");
        assert_eq!(timer_colour(150), 0x00A0_D0FF);
        assert_eq!(timer_colour(10), 0x00A0_80FF);
        assert_eq!(timer_colour(5), 0x00E0_80C0);
        assert_eq!(timer_colour(-3), 0x00FF_4040);
    }
}
