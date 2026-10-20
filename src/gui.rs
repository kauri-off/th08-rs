//! Spell manager g_Gui (12 §5.8): player/boss spell-name plates, cut-ins, streak strips and the bonus line.

use crate::anmmgr::AnmManager;
use crate::anmvm::{self, AnmVm};
use crate::enemy::EnemyManager;
use crate::game::Ctx;
use crate::gfx::PLAYFIELD;
use crate::player::PlayerRef;
use crate::task::{self, Chain, Ret, Task};
use anyhow::Result;
use std::cell::RefCell;
use std::rc::Rc;

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

thread_local! {
    static QUEUE: RefCell<Vec<Req>> = const { RefCell::new(Vec::new()) };
}

pub fn request(r: Req) {
    QUEUE.with_borrow_mut(|q| q.push(r));
}

// g_Gui 0x4EA670.
pub struct Gui {
    player: PlayerRef,
    enemies: Rc<RefCell<EnemyManager>>,
    // VMs at +0x120 + k·0x2A4, roles in the 12 §5.8 table; 3 and 5 never get a script.
    vms: [AnmVm; 14],
    raster: crate::text::Raster,
    // Pending Text_RenderToVm requests: (VM, text, colour, x inside the strip).
    texts: Vec<(usize, String, [u8; 3], f32)>,
    // Spell flag bit 10: name-only show (native 22), no bonus line and no VM 13 interrupts.
    name_only: bool,
    card: i32,
    shot: u8,
    // Spell +0xFC bonus and spell flag bit 2 (still capturable), copied from the running spell (12 §5.8 draw step 4).
    bonus: i32,
    capturable: bool,
    // Captures / attempts of the card for this shot type (CATK story +0x158 / +0x124, practice +0x1F4 / +0x1C0).
    history: (u32, u32),
}

impl Gui {
    pub fn register(
        ctx: &mut Ctx,
        player: PlayerRef,
        enemies: Rc<RefCell<EnemyManager>>,
    ) -> Result<Rc<RefCell<Gui>>> {
        QUEUE.with_borrow_mut(Vec::clear);
        let g = Rc::new(RefCell::new(Gui {
            player,
            enemies,
            vms: Default::default(),
            raster: crate::text::Raster::default(),
            texts: Vec::new(),
            name_only: false,
            card: -1,
            shot: ctx.game.shot,
            bonus: 0,
            capturable: false,
            history: (0, 0),
        }));
        let t: task::TaskRef<Ctx> = g.clone();
        task::add(ctx, Chain::Calc, CALC_PRIO, 0, t.clone())?;
        task::add(ctx, Chain::Draw, DRAW_PRIO, 0, t)?;
        Ok(g)
    }

    // Gui_LoadFaces 0x414590 part (12 §5.8): face_cdbg, VMs 0–7 hidden, the load-time scripts of VMs 6 and 10–13.
    fn load(&mut self, ctx: &mut Ctx) -> Result<()> {
        ctx.anm
            .load(&mut ctx.gfx, &ctx.archive, CDBG_SLOT, "face_cdbg.anm")?;
        let anm = &mut ctx.anm;
        for vm in &mut self.vms[..8] {
            *vm = AnmVm::default();
            vm.flags &= !anmvm::F_VISIBLE;
        }
        anm.set_script(&mut self.vms[6], TEXT_SLOT, 4);
        self.vms[6].flags &= !anmvm::F_VISIBLE;
        for (k, s) in [(10, 1), (11, 0), (12, 4), (13, 2)] {
            anm.set_script(&mut self.vms[k], ASCII_SLOT, s);
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

    fn handle(&mut self, anm: &mut AnmManager, r: Req) {
        let v = &mut self.vms;
        match r {
            // Gui_ShowPlayerSpellName 0x415D60 (12 §5.8); face −1 = dissolve spell.
            Req::PlayerShow { face, name, last } => {
                // SPEC-GAP: 12 §5.8 skips only VM 0 for face −1 (dissolve spell); the port also leaves VMs 2/4 untouched.
                if face >= 0 {
                    anm.set_script(&mut v[0], FACE_SLOT + face as usize, 0);
                    v[0].set_sprite(anm, 0);
                    anm.set_script(&mut v[2], CDBG_SLOT, 0);
                    v[2].set_sprite(anm, last as i32);
                    anm.set_script(&mut v[4], CDBG_SLOT, 2);
                    v[4].set_sprite(anm, last as i32);
                }
                // Name in COLORREF 0x00F0F0FF, left-aligned in the strip (0, 90); 0x17CE8CC := 2 is not needed (full redraw).
                anm.set_script(&mut v[6], TEXT_SLOT, 4);
                self.texts.push((6, name.to_string(), [255, 240, 240], 0.0));
                v[10].pending_interrupt = 1;
                crate::sound::play(0xE);
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
                    anm.set_script(&mut v[1], SPELL_FACE_SLOT, 0);
                    v[1].set_sprite(anm, face);
                }
                anm.set_script(&mut v[2], CDBG_SLOT, 1);
                v[2].set_sprite(anm, 0);
                anm.set_script(&mut v[4], CDBG_SLOT, 3);
                v[4].set_sprite(anm, 0);
                if (0xCD..=0xDD).contains(&card) {
                    anm.set_script(&mut v[7], TEXT_SLOT, 6);
                    anm.set_script(&mut v[8], TEXT_SLOT, 7);
                    anm.set_script(&mut v[9], TEXT_SLOT, 8);
                } else {
                    anm.set_script(&mut v[7], TEXT_SLOT, 5);
                }
                // Right-aligned (0x4664A0): x = strip.x + strip.w − len·size/2, size 15, colour 0x00FFF0F0.
                let x = 320.0 - name.len() as f32 * 15.0 / 2.0;
                self.texts
                    .push((7, crate::text::decode_sjis(&name), [240, 240, 255], x));
                v[11].pending_interrupt = 1;
                if !name_only {
                    v[13].pending_interrupt = 1;
                    self.card = card;
                }
                self.name_only = name_only;
                crate::sound::play(0xE);
            }
            // Spell_HideName 0x416160 (native 23).
            Req::BossHide => {
                v[7].pending_interrupt = 1;
                v[11].pending_interrupt = 2;
                if !self.name_only {
                    v[13].pending_interrupt = 2;
                }
            }
            Req::Attempt { .. } | Req::Captured { .. } => {}
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
            let t = ctx.gfx.texture(sp.tex);
            let (tx, ty) = (
                (sp.uv[0] * t.width as f32).round() as u32,
                (sp.uv[1] * t.height as f32).round() as u32,
            );
            let w = sp.w as u32;
            let rgba = self.raster.render_at(&s, colour, w as usize, 15, x);
            ctx.gfx.write_texture(sp.tex, tx, ty, w, 15, &rgba);
        }
    }

    // Draw2DSnap (mode 0) / Draw2DRot (mode 2) (05 §9).
    fn snap(anm: &mut AnmManager, gfx: &mut crate::gfx::Gfx, vm: &mut AnmVm) {
        anm.draw_mode(gfx, vm, 0);
    }

    fn rot(anm: &mut AnmManager, gfx: &mut crate::gfx::Gfx, vm: &mut AnmVm) {
        anm.draw_mode(gfx, vm, 2);
    }

    fn shown(vm: &AnmVm) -> bool {
        vm.script.is_some() && vm.visible()
    }

    // Bonus line: 8 digits of the bonus, captures and attempts (3 digits each), leading zeros suppressed.
    // 12 §5.8 draw step 4: VM 12 stamps sprite 0x88 + d from VM 13 pos + (−40, +1), advance 7.
    fn draw_bonus(&mut self, anm: &mut AnmManager, gfx: &mut crate::gfx::Gfx) {
        let (captures, attempts) = self.history;
        let base = self.vms[13].pos;
        let mut x = base[0] - 40.0;
        let y = base[1] + 1.0;
        let vm = &mut self.vms[12];
        let mut stamp = |anm: &mut AnmManager, gfx: &mut crate::gfx::Gfx, x: f32, d: u32| {
            vm.set_sprite(anm, 0x88 + d as i32);
            vm.pos = [x, y, 0.0];
            anm.draw_mode(gfx, vm, 0);
        };
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
                stamp(anm, gfx, x, d);
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
                stamp(anm, gfx, x + 7.0 * k as f32, c / div % 10);
            }
        }
        x += 21.0 + 13.0;
        let a = attempts.min(999);
        for (k, div) in [100, 10, 1].into_iter().enumerate() {
            if a >= div || div == 1 || (div == 10 && hundreds) {
                stamp(anm, gfx, x + 7.0 * k as f32, a / div % 10);
            }
        }
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
            ctx.anm.unload(&mut ctx.gfx, CDBG_SLOT);
        }
    }

    // Spell_Calc 0x416B90 (VM part, 12 §5.8).
    fn calc(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        for r in QUEUE.with_borrow_mut(std::mem::take) {
            Self::record(&mut ctx.game, &r);
            self.handle(&mut ctx.anm, r);
        }
        if self.card >= 0 {
            // Read from score.dat every frame for the bonus line.
            self.history = ctx.game.score.card_history(
                self.card as usize,
                self.shot as usize,
                ctx.game.spell_practice(),
            );
        }
        let frozen = {
            let e = self.enemies.borrow();
            if let Some(s) = &e.g.spell {
                (self.bonus, self.capturable) = (s.bonus, s.capture);
            }
            self.player.borrow().freeze_bullets || crate::enemy::timer_pause() != 0
        };
        // VMs run only while bullets are not frozen (GameFlags bit 10) and the freeze byte 0x160F534 is 0 (12 §5.8).
        if !frozen {
            // SPEC-GAP: 07 §11.1 does not order the capture-ring step against the VM runs; done first.
            self.enemies.borrow_mut().capture_ring_step(&mut ctx.anm);
            for k in CALC_ORDER {
                let vm = &mut self.vms[k];
                if vm.script.is_some() && vm.run(&mut ctx.anm) {
                    vm.script = None;
                }
            }
        }
        Ret::Next
    }

    // Spell_Draw 0x4178C0 (12 §5.8 draw steps 1–4).
    fn draw(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        self.flush_texts(ctx);
        let (anm, gfx) = (&mut ctx.anm, &mut ctx.gfx);
        gfx.set_viewport(PLAYFIELD);
        // Tint 0xFFF01010 only while GameFlags bit 10, off at the end (06 §10.2).
        if self.player.borrow().freeze_bullets {
            (anm.tint, anm.tint_on) = (0xFFF01010, true);
        }
        let v = &mut self.vms;
        // Boss strips (VM 2/4) are drawn only while the player cut-in VM 0 is visible: original quirk (12 §5.8 step 1).
        if Self::shown(&v[0]) {
            Self::snap(anm, gfx, &mut v[0]);
            Self::snap(anm, gfx, &mut v[2]);
            Self::rot(anm, gfx, &mut v[4]);
        }
        if Self::shown(&v[1]) {
            let p = v[1].pos;
            let o = v[1].pos_offset;
            // Boss cut-in drawn at pos + pos_offset, restored afterwards (12 §5.8 step 2).
            v[1].pos = [p[0] + o[0], p[1] + o[1], p[2] + o[2]];
            Self::snap(anm, gfx, &mut v[1]);
            v[1].pos = p;
        }
        if Self::shown(&v[6]) {
            let p = v[6].pos;
            v[10].pos = [p[0] - 32.0, p[1], p[2]];
            Self::snap(anm, gfx, &mut v[10]);
            Self::rot(anm, gfx, &mut v[6]);
        }
        if Self::shown(&v[7]) {
            // Neutral tint +0x2620 = 0x80808080 from Spell_Start (12 §5.8 step 4).
            (anm.tint, anm.tint_on) = (0x8080_8080, true);
            v[11].pos = v[7].pos;
            Self::snap(anm, gfx, &mut v[11]);
            for k in [7, 8, 9] {
                if v[k].script.is_some() {
                    Self::rot(anm, gfx, &mut v[k]);
                }
            }
            Self::snap(anm, gfx, &mut v[13]);
            if !self.name_only {
                self.draw_bonus(anm, gfx);
            }
            anm.tint_off();
        }
        anm.tint_off();
        Ret::Next
    }
}
