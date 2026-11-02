//! Spell manager g_Gui (12 §5.8): player/boss spell-name plates, cut-ins, streak strips and the bonus line.

use crate::anmmgr::{AnmManager, Tint};
use crate::anmvm::{self, AnmVm};
use crate::drawlist::{PLAYFIELD, Recorder, TextCmd};
use crate::enemy::EnemyManager;
use crate::game::Ctx;
use crate::player::PlayerRef;
use crate::task::{self, Chain, Ret, Task};
use anyhow::Result;
use std::cell::RefCell;
use std::rc::Rc;
use th08_formats::sjis::{self, SjisString};

// Gui_Calc = Spell_Calc 0x416B90 at calc prio 12, Gui_Draw = Spell_Draw 0x4178C0 at draw prio 15 (12 §5.8).
pub const CALC_PRIO: u16 = 12;
pub const DRAW_PRIO: u16 = 15;
// face_cdbg.anm slot 15; player faces 16 (human) / 17 (youkai); spell face file slot 18 (12 §5.8).
pub const CDBG_SLOT: usize = 15;
const FACE_SLOT: usize = 16;
const SPELL_FACE_SLOT: usize = 18;
const TEXT_SLOT: usize = crate::msg::TEXT_SLOT;
const ASCII_SLOT: usize = crate::ascii::ANM_SLOT;
// VM run order of Spell_Calc (12 §5.8); VM 12 (digit stamp) is never run.
const CALC_ORDER: [usize; 13] = [0, 2, 4, 6, 10, 1, 3, 5, 7, 8, 9, 11, 13];

// Calls into g_Gui from Bomb_Start, Player_UpdateBomb, Spell_Start / Spell_End and ECL natives 22/23, queued for Gui_Calc.
#[derive(Clone, Debug)]
pub enum Req {
    PlayerShow {
        face: i32,
        name: &'static str,
        last: bool,
    },
    PlayerHide,
    BossShow {
        face: i32,
        name: Vec<u8>,
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

// g_Gui 0x4EA670.
pub struct Gui {
    player: PlayerRef,
    enemies: Rc<RefCell<EnemyManager>>,
    // VMs at +0x120 + k·0x2A4, roles in the 12 §5.8 table; 3 and 5 never get a script.
    vms: [AnmVm; 14],
    // Pending Text_RenderToVm requests: (VM, Shift-JIS text, colour, x inside the strip).
    texts: Vec<(usize, SjisString, [u8; 3], f32)>,
    // Spell flag bit 10: name-only show (native 22), no bonus line and no VM 13 interrupts.
    name_only: bool,
    card: i32,
    shot: u8,
    // Spell +0xFC bonus and spell flag bit 2 (still capturable), read live from the card or what it left (12 §5.8 step 4).
    bonus: i32,
    capturable: bool,
    // Captures / attempts of the card for this shot type (CATK story +0x158 / +0x124, practice +0x1F4 / +0x1C0).
    history: (u32, u32),
    // +0x2620 bonus-line tint, 0x80808080 at Spell_Start; alpha +0x2623 fades 0x80 ↔ 0x20 (12 §5.8, 07 §11.1 step 1.2).
    tint: u32,
    // Port-only draw-phase scratch: the global tint VMs 0–6 were drawn under when the VM-7 block's tint-off followed
    // (06 §10.2); None = the emit-time tint is the right one.
    pre_tint: Option<Tint>,
}

impl Gui {
    pub fn register(
        ctx: &mut Ctx,
        player: PlayerRef,
        enemies: Rc<RefCell<EnemyManager>>,
    ) -> Result<Rc<RefCell<Gui>>> {
        ctx.gui_queue.clear();
        let g = Rc::new(RefCell::new(Gui {
            player,
            enemies,
            vms: Default::default(),
            texts: Vec::new(),
            name_only: false,
            card: -1,
            shot: ctx.game.shot,
            bonus: 0,
            capturable: false,
            history: (0, 0),
            tint: 0x8080_8080,
            pre_tint: None,
        }));
        let t: task::TaskRef<Ctx> = g.clone();
        task::add(ctx, Chain::Calc, CALC_PRIO, 0, t.clone())?;
        task::add(ctx, Chain::Draw, DRAW_PRIO, 0, t)?;
        Ok(g)
    }

    // Gui_LoadFaces 0x414590 part (12 §5.8): face_cdbg, VMs 0–7 hidden, the load-time scripts of VMs 6 and 10–13.
    fn load(&mut self, ctx: &mut Ctx) -> Result<()> {
        ctx.anm
            .load(&mut ctx.rec, &ctx.archive, CDBG_SLOT, "face_cdbg.anm")?;
        let anm = &mut ctx.anm;
        for vm in &mut self.vms[..8] {
            *vm = AnmVm::default();
            vm.flags &= !anmvm::F_VISIBLE;
        }
        anm.set_script(&mut ctx.rng, &mut self.vms[6], TEXT_SLOT, 4);
        self.vms[6].flags &= !anmvm::F_VISIBLE;
        for (k, s) in [(10, 1), (11, 0), (12, 4), (13, 2)] {
            anm.set_script(&mut ctx.rng, &mut self.vms[k], ASCII_SLOT, s);
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
                let d = g.difficulty.clamp(0, 4) as u8;
                g.score
                    .card_attempt(*card as usize, shot, d, practice, name, owner)
            }
            Req::Captured {
                card,
                bonus,
                comment,
            } if *card >= 0 => {
                // HSCR +0x88 + id: captured during this game (15 §2.3).
                if let Some(c) = g.rec_cards.get_mut(*card as usize) {
                    *c = 1;
                }
                g.score.card_capture(
                    *card as usize,
                    shot,
                    practice,
                    (*bonus).max(0) as u32,
                    comment,
                )
            }
            _ => {}
        }
    }

    fn handle(
        &mut self,
        anm: &mut AnmManager,
        rng: &mut crate::rng::Rng,
        r: Req,
        sound: &mut crate::sound::Queue,
        panel: &mut crate::hud::Panel,
    ) {
        let v = &mut self.vms;
        match r {
            // Gui_ShowPlayerSpellName 0x415D60 (12 §5.8); face −1 = dissolve spell.
            Req::PlayerShow { face, name, last } => {
                // Face −1 (dissolve spell) skips only VM 0; the strips 2/4 always restart (12 §5.8).
                if face >= 0 {
                    anm.set_script(rng, &mut v[0], FACE_SLOT + face as usize, 0);
                    v[0].set_sprite(anm, 0);
                }
                anm.set_script(rng, &mut v[2], CDBG_SLOT, 0);
                v[2].set_sprite(anm, last as i32);
                anm.set_script(rng, &mut v[4], CDBG_SLOT, 2);
                v[4].set_sprite(anm, last as i32);
                // Name in COLORREF 0x00F0F0FF, left-aligned in the strip (0, 90); 0x17CE8CC := 2 (12 §5.3).
                panel.set_repaint(2);
                anm.set_script(rng, &mut v[6], TEXT_SLOT, 4);
                self.texts
                    .push((6, sjis::encode(name), [255, 240, 240], 0.0));
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
                    // File pointer g_Gui+0x262C = portrait k = 2, swapped by Msg_Start(0) in 6B / Ex (12 §5.8, 10 §4).
                    let slot = self
                        .enemies
                        .borrow()
                        .msg
                        .as_ref()
                        .map_or(SPELL_FACE_SLOT, |m| m.borrow().face_slot[2]);
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
                let x = (320.0 - name.len() as f32 * 15.0 / 2.0).trunc();
                self.texts
                    .push((7, SjisString(name.to_vec()), [240, 240, 255], x));
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
            Req::Attempt { .. } => self.tint = 0x8080_8080,
            Req::Captured { .. } => {}
        }
    }

    // Text_RenderToVm 0x4663B0 into the VM's text.anm strip (size 15, 10 §6).
    fn flush_texts(&mut self, ctx: &mut Ctx) {
        for (k, s, colour, x) in std::mem::take(&mut self.texts) {
            let Some(sp) = ctx
                .anm
                .sprite(TEXT_SLOT, self.vms[k].sprite_index.max(0) as usize)
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
                size: 15,
                text: s,
                color: colour,
                x,
                outline: 0,
            });
        }
    }

    fn shown(vm: &AnmVm) -> bool {
        vm.script.is_some() && vm.visible()
    }

    // Bonus line: 8 digits of the bonus, captures and attempts (3 digits each), leading zeros suppressed.
    // Live +0xFC: the card's, or after the card ended the value time orbs keep changing (07 §11.1).
    fn read_bonus(&mut self) {
        let en = self.enemies.borrow();
        if let Some(s) = &en.g.spell {
            (self.bonus, self.capturable) = (s.bonus, s.capture);
        } else if let Some(l) = &en.g.spell_left {
            // Also after Spell_End: a same-frame timeout + op 123 left bit 2 clear and +0xFC 0 (12 §5.8 step 4).
            (self.bonus, self.capturable) = (l.bonus, l.capture);
        }
    }

    // 12 §5.8 draw step 4: VM 12 stamps sprite 0x88 + d from VM 13 pos + (−40, +1), advance 7; (x, y, digit) each.
    fn bonus_stamps(&self) -> Vec<(f32, f32, u32)> {
        let (captures, attempts) = self.history;
        let base = self.vms[13].pos;
        let mut x = base[0] - 40.0;
        let y = base[1] + 1.0;
        let mut out = Vec::new();
        let bonus = if self.capturable {
            self.bonus.max(0) as u32
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
            x += 7.0;
            div /= 10;
        }
        x += 32.0;
        // Captures after x += 32, attempts after x += 13; the attempts' tens digit is forced when captures have hundreds.
        let c = captures.min(999);
        let hundreds = c >= 100;
        for (k, div) in [100, 10, 1].into_iter().enumerate() {
            if c >= div || div == 1 {
                out.push((x + 7.0 * k as f32, y, c / div % 10));
            }
        }
        // Two advances between the three slots, none after the units; then x += 13 (12 §5.8).
        x += 14.0 + 13.0;
        let a = attempts.min(999);
        for (k, div) in [100, 10, 1].into_iter().enumerate() {
            if a >= div || div == 1 || (div == 10 && hundreds) {
                out.push((x + 7.0 * k as f32, y, a / div % 10));
            }
        }
        out
    }

    // VM 12 with the sprite and position of one stamp.
    fn stamp(vm: &mut AnmVm, anm: &AnmManager, (x, y, d): (f32, f32, u32)) {
        vm.set_sprite(anm, 0x88 + d as i32);
        vm.pos = [x, y, 0.0];
    }
}

impl Task<Ctx> for Gui {
    fn on_register(&mut self, ctx: &mut Ctx, chain: Chain, _: u8) -> Result<()> {
        if chain == Chain::Calc {
            self.load(ctx)?;
        }
        Ok(())
    }

    fn on_remove(&mut self, ctx: &mut Ctx, chain: Chain, _: u8) {
        if chain == Chain::Calc {
            ctx.anm.unload(&mut ctx.rec, CDBG_SLOT);
        }
    }

    // Spell_Calc 0x416B90 (VM part, 12 §5.8).
    fn calc(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        for r in std::mem::take(&mut ctx.gui_queue) {
            Self::record(&mut ctx.game, &r);
            self.handle(
                &mut ctx.anm,
                &mut ctx.rng,
                r,
                &mut ctx.sound,
                &mut ctx.panel,
            );
        }
        if self.card >= 0 {
            // Read from score.dat every frame for the bonus line.
            self.history = ctx.game.score.card_history(
                self.card as usize,
                self.shot as usize,
                ctx.game.spell_practice(),
            );
        }
        let frozen = self.player.borrow().freeze_bullets(ctx.game.flags) || ctx.timer_pause != 0;
        // Nothing runs while bullets are frozen (GameFlags bit 10) or the freeze byte 0x160F534 is set (07 §11.1).
        if !frozen {
            // Steps 1 (card active) and 2 (capture ring) run before the VMs (07 §11.1).
            let mut e = self.enemies.borrow_mut();
            if e.g.spell.is_some() {
                // Step 1.2: fade the bonus line while the player is in the top band x ≥ 64, y < 64.
                let p = self.player.borrow().pos;
                let a = self.tint >> 24;
                let a = if p[0] < 64.0 || p[1] >= 64.0 {
                    if a < 0x80 { a + 4 } else { a }
                } else if a > 0x20 {
                    a - 4
                } else {
                    a
                };
                self.tint = self.tint & 0x00FF_FFFF | a << 24;
            }
            let ended = e.spell_step(&ctx.anm, &mut crate::sys!(ctx));
            e.capture_ring_step(&mut ctx.anm, &mut crate::sys!(ctx));
            drop(e);
            if ended {
                // Spell_HideName from step 1.1 lands before this frame's VM run.
                for r in std::mem::take(&mut ctx.gui_queue) {
                    self.handle(
                        &mut ctx.anm,
                        &mut ctx.rng,
                        r,
                        &mut ctx.sound,
                        &mut ctx.panel,
                    );
                }
            }
            for k in CALC_ORDER {
                let vm = &mut self.vms[k];
                if vm.script.is_some() && vm.run(&mut ctx.anm, &mut ctx.rng) {
                    vm.script = None;
                }
            }
            // Step 3: Timer_Decrement(+0x108) at game speed after the VMs, also while no card is active (07 §11.1).
            let (speed, sr) = (ctx.anm.speed, ctx.anm.slowdown_restore);
            self.enemies.borrow_mut().g.spell_remaining.dec(speed, sr);
        }
        self.read_bonus();
        Ret::Next
    }

    // Spell_Draw 0x4178C0 (12 §5.8 draw steps 1–4). Modes 0 / 2 write no VM state (05 §9), so the step keeps only the
    // bonus read, the text renders, the VM 10 / 11 / 12 positions and the tint-off; the emit draws.
    fn draw_step(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        // The bonus line reads the live +0xFC (08 §8.5: time orbs collected after Spell_Calc show the same frame).
        self.read_bonus();
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
                Self::stamp(&mut self.vms[12], &ctx.anm, last);
            }
            // Spell_Draw turns the tint off only inside its VM-7 block (06 §10.2); the emit applies +0x2620 itself.
            self.pre_tint = ctx.anm.tint_state();
            ctx.anm.tint_off();
        } else {
            self.pre_tint = None;
        }
        Ret::Next
    }

    fn emit(&self, ctx: &Ctx, rec: &mut Recorder, _: u8) {
        let anm = &ctx.anm;
        rec.set_viewport(PLAYFIELD);
        // No 0xFFF01010 freeze tint here: the only tint of Spell_Draw is +0x2620 of step 4 (12 §5.8, RE-Q68).
        let v = &self.vms;
        // VMs 0–6 are drawn before the VM-7 block, under the global tint still on then.
        let pre = self.pre_tint;
        let draw =
            |rec: &mut Recorder, vm: &AnmVm, mode| anm.emit_tinted(rec, vm, mode, vm.pos[2], pre);
        // Boss strips (VM 2/4) are drawn only while the player cut-in VM 0 is visible: original quirk (12 §5.8 step 1).
        if Self::shown(&v[0]) {
            draw(rec, &v[0], 0);
            draw(rec, &v[2], 0);
            draw(rec, &v[4], 2);
        }
        if Self::shown(&v[1]) {
            // Boss cut-in drawn at pos + pos_offset, restored afterwards (12 §5.8 step 2).
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
            // Tint +0x2620 (12 §5.8 step 4), turned off by the step; the alpha-0 skip tests the untinted colour.
            let t = Some(Tint(self.tint));
            let draw =
                |rec: &mut Recorder, vm: &AnmVm, mode| anm.emit_tinted(rec, vm, mode, vm.pos[2], t);
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
