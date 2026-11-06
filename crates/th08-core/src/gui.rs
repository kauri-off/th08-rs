//! Spell manager g_Gui (12 §5.8): player/boss spell-name plates, cut-ins, streak strips and the bonus line.

use crate::anmmgr::{AnmManager, Tint};
use crate::anmvm::{self, AnmVm, table_index};
use crate::builtin::Builtin;
use crate::drawlist::{Align, PLAYFIELD, Recorder, TextCmd, TextSite};
use crate::enemy::EnemyManager;
use crate::game::{Ctx, View};
use crate::task::{self, Chain, Ret, Task};
use crate::textref::Text;
use crate::types::lo8;
use anyhow::Result;

// Gui_Calc = Spell_Calc 0x416B90 at calc prio 12, Gui_Draw = Spell_Draw 0x4178C0 at draw prio 15.
pub const CALC_PRIO: u16 = 12;
pub const DRAW_PRIO: u16 = 15;
// face_cdbg.anm slot 15; player faces 16 (human) / 17 (youkai); spell face file slot 18.
pub const CDBG_SLOT: usize = 15;
const FACE_SLOT: usize = 16;
const SPELL_FACE_SLOT: usize = 18;
const TEXT_SLOT: usize = crate::msg::TEXT_SLOT;
const ASCII_SLOT: usize = crate::ascii::ANM_SLOT;
// VM run order of Spell_Calc; VM 12 (digit stamp) is never run.
const CALC_ORDER: [usize; 13] = [0, 2, 4, 6, 10, 1, 3, 5, 7, 8, 9, 11, 13];
// Bonus line (draw step 4): first stamp at VM 13 pos + (−40, +1), 7 px per digit, digit sprites 0x88 + d.
const STAMP_DX: f32 = -40.0;
const STAMP_DY: f32 = 1.0;
const STAMP_ADVANCE: f32 = 7.0;
const DIGIT_SPRITE: i32 = 0x88;
// Text strips (10 §6): glyph size 15, boss-name strip width 320; player name 0x00F0F0FF, boss name 0x00FFF0F0.
const TEXT_SIZE: u32 = 15;
const NAME_STRIP_W: f32 = 320.0;
const PLAYER_NAME_RGB: [u8; 3] = [255, 240, 240];
const BOSS_NAME_RGB: [u8; 3] = [240, 240, 255];
// Spell_Start bonus-line tint +0x2620 (07 §11.1); its alpha fades between 0x20 and 0x80 (step 1.2).
const TINT_START: u32 = 0x8080_8080;
const TINT_A_MIN: u32 = 0x20;
const TINT_A_MAX: u32 = 0x80;

// Calls into g_Gui from Bomb_Start, Player_UpdateBomb, Spell_Start / Spell_End and ECL natives 22/23, queued for Gui_Calc.
#[derive(Clone, Debug)]
pub enum Req {
    PlayerShow {
        face: i32,
        name: Builtin,
        last: bool,
    },
    PlayerHide,
    BossShow {
        face: i32,
        // Card name bytes keyed by card id, or the native-22 literal (07 §11.1).
        name: Text,
        card: i32,
        name_only: bool,
    },
    BossHide,
    Attempt {
        card: i32,
        name: Vec<u8>,
        owner: Vec<u8>,
    },
    Captured {
        card: i32,
        bonus: i32,
        comment: [Vec<u8>; 2],
    },
}

// Port queue of g_Gui calls (`Ctx::gui_queue`, `Sys::gui`), drained by Gui_Calc in arrival order.
pub type Queue = Vec<Req>;

// g_Gui 0x4EA670; `Ctx.gui`, its chain nodes are `GuiTask`.
#[derive(Clone)]
pub struct Gui {
    // VMs at +0x120 + k·0x2A4, roles in the §5.8 table; 3 and 5 never get a script.
    vms: [AnmVm; 14],
    // Pending Text_RenderToVm requests: (VM, text, colour, x inside the strip, placement).
    texts: Vec<(usize, Text, [u8; 3], f32, Align)>,
    // Spell flag bit 10: name-only show (native 22), no bonus line and no VM 13 interrupts.
    name_only: bool,
    card: i32,
    shot: u8,
    // Spell +0xFC bonus and spell flag bit 2 (still capturable), read live from the card or what it left (step 4).
    bonus: i32,
    capturable: bool,
    // Captures / attempts of the card for this shot type (CATK story +0x158 / +0x124, practice +0x1F4 / +0x1C0).
    history: (u32, u32),
    // +0x2620 bonus-line tint, 0x80808080 at Spell_Start; alpha +0x2623 fades 0x80 ↔ 0x20 (07 §11.1 step 1.2).
    tint: u32,
    // Port-only draw-phase scratch: the global tint VMs 0–6 were drawn under when the VM-7 block's tint-off followed
    // (06 §10.2); None = the emit-time tint is the right one.
    pre_tint: Option<Tint>,
}

impl Gui {
    pub fn register(ctx: &mut Ctx) -> Result<crate::chain::Node> {
        ctx.w.gui_queue.clear();
        ctx.w.gui = Some(Box::new(Gui::new(ctx.w.game.shot)));
        let t = crate::chain::Node::Gui;
        task::add(ctx, Chain::Calc, CALC_PRIO, 0, t)?;
        task::add(ctx, Chain::Draw, DRAW_PRIO, 0, t)?;
        Ok(t)
    }

    // g_Gui as Gui_Register leaves it: no card, bonus-line tint 0x80808080.
    pub fn new(shot: u8) -> Gui {
        Gui {
            vms: Default::default(),
            texts: Vec::new(),
            name_only: false,
            card: -1,
            shot,
            bonus: 0,
            capturable: false,
            history: (0, 0),
            tint: TINT_START,
            pre_tint: None,
        }
    }

    // Clone test state (worldclone.rs): card, bonus, tint and per VM (script index, interrupt, alpha).
    #[cfg(test)]
    pub fn trace(&self) -> Vec<u64> {
        let mut t = vec![
            i64::from(self.card).cast_unsigned(),
            i64::from(self.bonus).cast_unsigned(),
            u64::from(self.capturable),
            u64::from(self.name_only),
            u64::from(self.tint),
        ];
        t.extend(self.vms.iter().map(|v| {
            u64::from(v.script_index.cast_unsigned()) << 32
                | u64::from(v.pending_interrupt) << 8
                | u64::from(v.color[anmvm::A])
        }));
        t
    }

    // Gui_LoadFaces 0x414590 part: face_cdbg, VMs 0–7 hidden, the load-time scripts of VMs 6 and 10–13.
    fn load(&mut self, ctx: &mut Ctx) -> Result<()> {
        ctx.w
            .anm
            .load(&mut ctx.rec, &ctx.archive, CDBG_SLOT, "face_cdbg.anm")?;
        let anm = &mut ctx.w.anm;
        for vm in &mut self.vms[..8] {
            *vm = AnmVm::default();
            vm.flags &= !anmvm::F_VISIBLE;
        }
        anm.set_script(&mut ctx.w.rng, &mut self.vms[6], TEXT_SLOT, 4);
        self.vms[6].flags &= !anmvm::F_VISIBLE;
        for (k, s) in [(10, 1), (11, 0), (12, 4), (13, 2)] {
            anm.set_script(&mut ctx.w.rng, &mut self.vms[k], ASCII_SLOT, s);
        }
        Ok(())
    }

    // CATK updates (15 §2.5): attempts in Spell_Start, captures in Spell_End; never in replay playback.
    fn record(g: &mut crate::gamemgr::Game, r: &Req) {
        if g.replay() {
            return;
        }
        let (shot, practice) = (g.shot as usize, g.spell_practice());
        match r {
            Req::Attempt { card, name, owner } if *card >= 0 => {
                let d = lo8(g.difficulty.clamp(0, 4));
                g.score
                    .card_attempt(table_index(*card), shot, d, practice, name, owner);
            }
            Req::Captured {
                card,
                bonus,
                comment,
            } if *card >= 0 => {
                // HSCR +0x88 + id: captured during this game (15 §2.3).
                if let Some(c) = g.rec_cards.get_mut(table_index(*card)) {
                    *c = 1;
                }
                g.score.card_capture(
                    table_index(*card),
                    shot,
                    practice,
                    (*bonus).max(0).cast_unsigned(),
                    comment,
                );
            }
            _ => {}
        }
    }

    pub(crate) fn handle(
        &mut self,
        anm: &mut AnmManager,
        rng: &mut crate::rng::Rng,
        r: Req,
        sound: &mut crate::sound::Queue,
        panel: &mut crate::hud::Panel,
        // The HUD's dialogue: Msg_Start(0) may have swapped the portrait file pointers (10 §4).
        msg: Option<&crate::msg::Dialogue>,
    ) {
        let v = &mut self.vms;
        match r {
            // Gui_ShowPlayerSpellName 0x415D60; face −1 = dissolve spell.
            Req::PlayerShow { face, name, last } => {
                // Face −1 (dissolve spell) skips only VM 0; the strips 2/4 always restart.
                if face >= 0 {
                    anm.set_script(rng, &mut v[0], FACE_SLOT + table_index(face), 0);
                    v[0].set_sprite(anm, 0);
                }
                anm.set_script(rng, &mut v[2], CDBG_SLOT, 0);
                v[2].set_sprite(anm, i32::from(last));
                anm.set_script(rng, &mut v[4], CDBG_SLOT, 2);
                v[4].set_sprite(anm, i32::from(last));
                // Name in COLORREF 0x00F0F0FF, left-aligned in the strip (0, 90); 0x17CE8CC := 2 (12 §5.3).
                panel.set_repaint(2);
                anm.set_script(rng, &mut v[6], TEXT_SLOT, 4);
                self.texts
                    .push((6, name.txt(), PLAYER_NAME_RGB, 0.0, Align::Left));
                v[10].pending_interrupt = 1;
                sound.play(0xE);
            }
            // Player hide 0x416130 (Player_UpdateBomb, bomb end).
            Req::PlayerHide => {
                v[6].pending_interrupt = 1;
                v[10].pending_interrupt = 2;
            }
            // Gui_ShowBossSpellName 0x415F00 (Spell_Start; native 22 with `name_only`).
            Req::BossShow {
                face,
                name,
                card,
                name_only,
            } => {
                if face >= 0 {
                    // File pointer g_Gui+0x262C = portrait k = 2, swapped by Msg_Start(0) in 6B / Ex (10 §4).
                    let slot = msg.map_or(SPELL_FACE_SLOT, |m| m.face_slot[2]);
                    anm.set_script(rng, &mut v[1], slot, 0);
                    v[1].set_sprite(anm, face);
                }
                anm.set_script(rng, &mut v[2], CDBG_SLOT, 1);
                v[2].set_sprite(anm, 0);
                anm.set_script(rng, &mut v[4], CDBG_SLOT, 3);
                v[4].set_sprite(anm, 0);
                if (0xCD..=0xDD).contains(&card) {
                    anm.set_script(rng, &mut v[7], TEXT_SLOT, 6);
                    anm.set_script(rng, &mut v[8], TEXT_SLOT, 7);
                    anm.set_script(rng, &mut v[9], TEXT_SLOT, 8);
                } else {
                    anm.set_script(rng, &mut v[7], TEXT_SLOT, 5);
                }
                // Text_RenderToVmRight 0x4664A0 (10 §6): x = trunc(u0 + sw − len·15/2), u0 0, sw 320, len in
                // Shift-JIS bytes; colour 0x00FFF0F0.
                let x = (NAME_STRIP_W - name.raw.len() as f32 * 15.0 / 2.0).trunc();
                self.texts
                    .push((7, name, BOSS_NAME_RGB, x, Align::Right(NAME_STRIP_W)));
                v[11].pending_interrupt = 1;
                if !name_only {
                    v[13].pending_interrupt = 1;
                    self.card = card;
                }
                self.name_only = name_only;
                // 0x17CE8CC := 2 (12 §5.3).
                panel.set_repaint(2);
                sound.play(0xE);
            }
            // Spell_HideName 0x416160 (native 23).
            Req::BossHide => {
                v[7].pending_interrupt = 1;
                v[11].pending_interrupt = 2;
                if !self.name_only {
                    v[13].pending_interrupt = 2;
                }
            }
            // Spell_Start resets the bonus-line tint (07 §11.1).
            Req::Attempt { .. } => self.tint = TINT_START,
            Req::Captured { .. } => {}
        }
    }

    // Text_RenderToVm 0x4663B0 into the VM's text.anm strip (size 15, 10 §6).
    #[expect(
        clippy::cast_sign_loss,
        reason = "ftol of the strip's v0 in pixels (10 §6)"
    )]
    fn flush_texts(&mut self, ctx: &mut Ctx) {
        for (k, s, colour, x, align) in std::mem::take(&mut self.texts) {
            let Some(sp) = ctx
                .w
                .anm
                .sprite(
                    TEXT_SLOT,
                    usize::from(self.vms[k].sprite_index.max(0).cast_unsigned()),
                )
                .copied()
            else {
                continue;
            };
            // 10 §6: W = texture-row width (512), the whole 15-row strip at (0, v0) is replaced.
            let t = ctx.rec.texture(sp.tex);
            let (w, ty) = (t.width, (sp.uv[1] * t.height as f32).round() as u32);
            ctx.rec.text(TextCmd {
                dst: sp.tex,
                y: ty,
                w,
                size: TEXT_SIZE,
                text: s.raw,
                key: s.key,
                color: colour,
                x,
                edge: sp.uv[0] * t.width as f32 + sp.w,
                outline: 0,
                align,
                site: TextSite::Spell,
            });
        }
    }

    fn shown(vm: &AnmVm) -> bool {
        vm.script.is_some() && vm.visible()
    }

    // Bonus line: 8 digits of the bonus, captures and attempts (3 digits each), leading zeros suppressed.
    // Live +0xFC: the card's, or after the card ended the value time orbs keep changing (07 §11.1).
    pub(crate) fn read_bonus(&mut self, en: &EnemyManager) {
        if let Some(s) = &en.g.spell {
            (self.bonus, self.capturable) = (s.bonus, s.capture);
        } else if let Some(l) = &en.g.spell_left {
            // Also after Spell_End: a same-frame timeout + op 123 left bit 2 clear and +0xFC 0 (step 4).
            (self.bonus, self.capturable) = (l.bonus, l.capture);
        }
    }

    // Draw step 4: VM 12 stamps sprite 0x88 + d from VM 13 pos + (−40, +1), advance 7; (x, y, digit) each.
    fn bonus_stamps(&self) -> Vec<(f32, f32, u32)> {
        let (captures, attempts) = self.history;
        let base = self.vms[13].pos;
        let mut x = base[0] + STAMP_DX;
        let y = base[1] + STAMP_DY;
        let mut out = Vec::new();
        let bonus = if self.capturable {
            self.bonus.max(0).cast_unsigned()
        } else {
            0
        };
        let mut on = false;
        let mut div = 10_000_000;
        while div > 0 {
            let d = bonus / div % 10;
            on |= d != 0 || div == 1;
            if on {
                out.push((x, y, d));
            }
            x += STAMP_ADVANCE;
            div /= 10;
        }
        x += 32.0;
        // Captures after x += 32, attempts after x += 13; the attempts' tens digit is forced when captures have hundreds.
        let c = captures.min(999);
        let hundreds = c >= 100;
        for (k, div) in [100, 10, 1].into_iter().enumerate() {
            if c >= div || div == 1 {
                out.push((x + STAMP_ADVANCE * k as f32, y, c / div % 10));
            }
        }
        // Two advances between the three slots, none after the units; then x += 13.
        x += 14.0 + 13.0;
        let a = attempts.min(999);
        for (k, div) in [100, 10, 1].into_iter().enumerate() {
            if a >= div || div == 1 || (div == 10 && hundreds) {
                out.push((x + STAMP_ADVANCE * k as f32, y, a / div % 10));
            }
        }
        out
    }

    // VM 12 with the sprite and position of one stamp.
    fn stamp(vm: &mut AnmVm, anm: &AnmManager, (x, y, d): (f32, f32, u32)) {
        vm.set_sprite(anm, DIGIT_SPRITE + d.cast_signed());
        vm.pos = [x, y, 0.0];
    }
}

// The spell manager lent out of `ctx.w.gui` for a step that also takes the `Ctx` (nothing it calls reads `ctx.w.gui`).
// None = no spell manager (Gui::register sets it before adding the nodes): the node does nothing.
fn with_gui<R>(ctx: &mut Ctx, f: impl FnOnce(&mut Gui, &mut Ctx) -> R) -> Option<R> {
    let mut g = ctx.w.gui.take()?;
    let r = f(&mut g, ctx);
    ctx.w.gui = Some(g);
    Some(r)
}

// Chain nodes of `ctx.w.gui` (calc prio 12, draw prio 15): stateless forwarder.
pub struct GuiTask;

impl Task<Ctx> for GuiTask {
    fn on_register(&mut self, ctx: &mut Ctx, chain: Chain, tag: u8) -> Result<()> {
        with_gui(ctx, |g, ctx| g.on_register(ctx, chain, tag)).unwrap_or(Ok(()))
    }

    fn on_remove(&mut self, ctx: &mut Ctx, chain: Chain, tag: u8) {
        with_gui(ctx, |g, ctx| g.on_remove(ctx, chain, tag));
    }

    fn calc(&mut self, ctx: &mut Ctx, tag: u8) -> Ret {
        with_gui(ctx, |g, ctx| g.calc(ctx, tag)).unwrap_or(Ret::Next)
    }

    fn draw_step(&mut self, ctx: &mut Ctx, tag: u8) -> Ret {
        with_gui(ctx, |g, ctx| g.draw_step(ctx, tag)).unwrap_or(Ret::Next)
    }
}

impl GuiTask {
    pub(crate) fn emit(&self, ctx: &View, rec: &mut Recorder, tag: u8) {
        if let Some(g) = ctx.w.gui.as_deref() {
            g.emit(ctx, rec, tag);
        }
    }
}

impl Gui {
    fn on_register(&mut self, ctx: &mut Ctx, chain: Chain, _: u8) -> Result<()> {
        if chain == Chain::Calc {
            self.load(ctx)?;
        }
        Ok(())
    }

    fn on_remove(&mut self, ctx: &mut Ctx, chain: Chain, _: u8) {
        if chain == Chain::Calc {
            ctx.w.anm.unload(&mut ctx.rec, CDBG_SLOT);
        }
    }

    // Spell_Calc 0x416B90 (VM part).
    fn calc(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        for r in std::mem::take(&mut ctx.w.gui_queue) {
            Self::record(&mut ctx.w.game, &r);
            self.handle(
                &mut ctx.w.anm,
                &mut ctx.w.rng,
                r,
                &mut ctx.w.sound,
                &mut ctx.w.panel,
                ctx.w.msg.as_deref(),
            );
        }
        if self.card >= 0 {
            // Read from score.dat every frame for the bonus line.
            self.history = ctx.w.game.score.card_history(
                table_index(self.card),
                self.shot as usize,
                ctx.w.game.spell_practice(),
            );
        }
        let frozen = crate::pl!(ctx.w).freeze_bullets(ctx.w.game.flags) || ctx.w.timer_pause != 0;
        // Nothing runs while bullets are frozen (GameFlags bit 10) or the freeze byte 0x160F534 is set (07 §11.1).
        if !frozen {
            // Steps 1 (card active) and 2 (capture ring) run before the VMs (07 §11.1).
            let e = crate::em!(ctx.w);
            if e.g.spell.is_some() {
                // Step 1.2: fade the bonus line while the player is in the top band x ≥ 64, y < 64.
                let p = crate::pl!(ctx.w).pos;
                let a = self.tint >> 24;
                let a = if p[0] < 64.0 || p[1] >= 64.0 {
                    if a < TINT_A_MAX { a + 4 } else { a }
                } else if a > TINT_A_MIN {
                    a - 4
                } else {
                    a
                };
                self.tint = self.tint & 0x00FF_FFFF | a << 24;
            }
            let ended = e.spell_step(&ctx.w.anm, &mut crate::sys!(ctx.w));
            e.capture_ring_step(&mut ctx.w.anm, &mut crate::sys!(ctx.w));
            if ended {
                // Spell_HideName from step 1.1 lands before this frame's VM run.
                for r in std::mem::take(&mut ctx.w.gui_queue) {
                    self.handle(
                        &mut ctx.w.anm,
                        &mut ctx.w.rng,
                        r,
                        &mut ctx.w.sound,
                        &mut ctx.w.panel,
                        ctx.w.msg.as_deref(),
                    );
                }
            }
            for k in CALC_ORDER {
                let vm = &mut self.vms[k];
                if vm.script.is_some() && vm.run(&mut ctx.w.anm, &mut ctx.w.rng) {
                    vm.script = None;
                }
            }
            // Step 3: Timer_Decrement(+0x108) at game speed after the VMs, also while no card is active (07 §11.1).
            let (speed, sr) = (ctx.w.anm.speed, ctx.w.anm.slowdown_restore);
            crate::em!(ctx.w).g.spell_remaining.dec(speed, sr);
        }
        // No enemy manager = no card: the bonus keeps its last value.
        if let Some(e) = ctx.w.enemies.as_deref() {
            self.read_bonus(e);
        }
        Ret::Next
    }

    // Spell_Draw 0x4178C0 (draw steps 1–4). Modes 0 / 2 write no VM state (05 §9), so the step keeps only the
    // bonus read, the text renders, the VM 10 / 11 / 12 positions and the tint-off; the emit draws.
    fn draw_step(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        // The bonus line reads the live +0xFC (08 §8.5: time orbs collected after Spell_Calc show the same frame).
        // No enemy manager = no card: the bonus keeps its last value.
        if let Some(e) = ctx.w.enemies.as_deref() {
            self.read_bonus(e);
        }
        self.flush_texts(ctx);
        if Self::shown(&self.vms[6]) {
            let p = self.vms[6].pos;
            self.vms[10].pos = [p[0] - 32.0, p[1], p[2]];
        }
        if Self::shown(&self.vms[7]) {
            self.vms[11].pos = self.vms[7].pos;
            // VM 12 keeps the sprite / position of its last stamp.
            if !self.name_only
                && let Some(&last) = self.bonus_stamps().last()
            {
                Self::stamp(&mut self.vms[12], &ctx.w.anm, last);
            }
            // Spell_Draw turns the tint off only inside its VM-7 block (06 §10.2); the emit applies +0x2620 itself.
            self.pre_tint = ctx.w.anm.tint_state();
            ctx.w.anm.tint_off();
        } else {
            self.pre_tint = None;
        }
        Ret::Next
    }

    fn emit(&self, ctx: &View, rec: &mut Recorder, _: u8) {
        let anm = &ctx.w.anm;
        rec.set_viewport(PLAYFIELD);
        // No 0xFFF01010 freeze tint here: the only tint of Spell_Draw is +0x2620 of step 4.
        let v = &self.vms;
        // VMs 0–6 are drawn before the VM-7 block, under the global tint still on then.
        let pre = self.pre_tint;
        let draw = |rec: &mut Recorder, vm: &AnmVm, mode| {
            crate::emit::anm::emit_tinted(anm, rec, vm, mode, vm.pos[2], pre);
        };
        // Boss strips (VM 2/4) are drawn only while the player cut-in VM 0 is visible: original quirk (step 1).
        if Self::shown(&v[0]) {
            draw(rec, &v[0], 0);
            draw(rec, &v[2], 0);
            draw(rec, &v[4], 2);
        }
        if Self::shown(&v[1]) {
            // Boss cut-in drawn at pos + pos_offset, restored afterwards (step 2).
            let mut c = v[1].clone();
            let (p, o) = (c.pos, c.pos_offset);
            c.pos = [p[0] + o[0], p[1] + o[1], p[2] + o[2]];
            draw(rec, &c, 0);
        }
        if Self::shown(&v[6]) {
            draw(rec, &v[10], 0);
            draw(rec, &v[6], 2);
        }
        if Self::shown(&v[7]) {
            // Tint +0x2620 (step 4), turned off by the step; the alpha-0 skip tests the untinted colour.
            let t = Some(Tint(self.tint));
            let draw = |rec: &mut Recorder, vm: &AnmVm, mode| {
                crate::emit::anm::emit_tinted(anm, rec, vm, mode, vm.pos[2], t);
            };
            draw(rec, &v[11], 0);
            for k in [7, 8, 9] {
                if v[k].script.is_some() {
                    draw(rec, &v[k], 2);
                }
            }
            draw(rec, &v[13], 0);
            if !self.name_only {
                let mut vm = v[12].clone();
                for st in self.bonus_stamps() {
                    Self::stamp(&mut vm, anm, st);
                    draw(rec, &vm, 0);
                }
            }
        }
    }
}
