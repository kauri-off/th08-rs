// Spell manager g_Gui (12 §5.8): player/boss spell-name plates, cut-ins, streak strips and the bonus line.
use crate::anmmgr::AnmManager;
use crate::anmvm::{self, AnmVm};
use crate::enemy::EnemyManager;
use crate::game::Ctx;
use crate::gfx::PLAYFIELD;
use crate::player::PlayerRef;
use crate::task::{self, Chain, Ret, Task};
use anyhow::Result;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub const CALC_PRIO: u16 = 12;
pub const DRAW_PRIO: u16 = 15;
pub const CDBG_SLOT: usize = 15;
const FACE_SLOT: usize = 16;
const SPELL_FACE_SLOT: usize = 18;
const TEXT_SLOT: usize = crate::msg::TEXT_SLOT;
const ASCII_SLOT: usize = crate::ascii::ANM_SLOT;
const CALC_ORDER: [usize; 13] = [0, 2, 4, 6, 10, 1, 3, 5, 7, 8, 9, 11, 13];

pub enum Req {
    PlayerShow { face: i32, name: &'static str, last: bool },
    PlayerHide,
    BossShow { face: i32, name: Vec<u8>, card: i32, name_only: bool },
    BossHide,
    Captured(i32),
}

thread_local! {
    static QUEUE: RefCell<Vec<Req>> = const { RefCell::new(Vec::new()) };
}

pub fn request(r: Req) {
    QUEUE.with_borrow_mut(|q| q.push(r));
}

pub struct Gui {
    player: PlayerRef,
    enemies: Rc<RefCell<EnemyManager>>,
    vms: [AnmVm; 14],
    raster: crate::text::Raster,
    texts: Vec<(usize, String, [u8; 3], f32)>,
    name_only: bool,
    card: i32,
    shot: u8,
    bonus: i32,
    capturable: bool,
    // SPEC-GAP: score.dat (CATK) is not loaded; captures/attempts are counted for this session only.
    history: HashMap<(i32, u8), (u32, u32)>,
}

impl Gui {
    pub fn register(ctx: &mut Ctx, player: PlayerRef, enemies: Rc<RefCell<EnemyManager>>) -> Result<Rc<RefCell<Gui>>> {
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
            history: HISTORY.with_borrow(Clone::clone),
        }));
        let t: task::TaskRef<Ctx> = g.clone();
        task::add(ctx, Chain::Calc, CALC_PRIO, 0, t.clone())?;
        task::add(ctx, Chain::Draw, DRAW_PRIO, 0, t)?;
        Ok(g)
    }

    // Gui_LoadFaces part: face_cdbg, VMs 0–7 hidden, the load-time scripts of VMs 6 and 10–13.
    fn load(&mut self, ctx: &mut Ctx) -> Result<()> {
        ctx.anm.load(&mut ctx.gfx, &ctx.archive, CDBG_SLOT, "face_cdbg.anm")?;
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

    fn handle(&mut self, anm: &mut AnmManager, r: Req) {
        let v = &mut self.vms;
        match r {
            // Gui_ShowPlayerSpellName 0x415D60.
            Req::PlayerShow { face, name, last } => {
                if face >= 0 {
                    anm.set_script(&mut v[0], FACE_SLOT + face as usize, 0);
                    v[0].set_sprite(anm, 0);
                    anm.set_script(&mut v[2], CDBG_SLOT, 0);
                    v[2].set_sprite(anm, last as i32);
                    anm.set_script(&mut v[4], CDBG_SLOT, 2);
                    v[4].set_sprite(anm, last as i32);
                }
                anm.set_script(&mut v[6], TEXT_SLOT, 4);
                self.texts.push((6, name.to_string(), [255, 240, 240], 0.0));
                v[10].pending_interrupt = 1;
                crate::sound::play(0xE);
            }
            // 0x416130.
            Req::PlayerHide => {
                v[6].pending_interrupt = 1;
                v[10].pending_interrupt = 2;
            }
            // Gui_ShowBossSpellName 0x415F00 (Spell_Start; native 22 with `name_only`).
            Req::BossShow { face, name, card, name_only } => {
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
                let x = 320.0 - name.len() as f32 * 15.0 / 2.0;
                self.texts.push((7, crate::text::decode_sjis(&name), [240, 240, 255], x));
                v[11].pending_interrupt = 1;
                if !name_only {
                    v[13].pending_interrupt = 1;
                    self.card = card;
                    self.history.entry((card, self.shot)).or_default().1 += 1;
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
            Req::Captured(card) => self.history.entry((card, self.shot)).or_default().0 += 1,
        }
    }

    // Text_RenderToVm into the VM's text.anm strip (size 15).
    fn flush_texts(&mut self, ctx: &mut Ctx) {
        for (k, s, colour, x) in std::mem::take(&mut self.texts) {
            let Some(sp) = ctx.anm.sprite(TEXT_SLOT, self.vms[k].sprite_index.max(0) as usize).copied() else { continue };
            let t = ctx.gfx.texture(sp.tex);
            let (tx, ty) = ((sp.uv[0] * t.width as f32).round() as u32, (sp.uv[1] * t.height as f32).round() as u32);
            let w = sp.w as u32;
            let rgba = self.raster.render_at(&s, colour, w as usize, 15, x);
            ctx.gfx.write_texture(sp.tex, tx, ty, w, 15, &rgba);
        }
    }

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
    fn draw_bonus(&mut self, anm: &mut AnmManager, gfx: &mut crate::gfx::Gfx) {
        let (captures, attempts) = self.history.get(&(self.card, self.shot)).copied().unwrap_or_default();
        let base = self.vms[13].pos;
        let mut x = base[0] - 40.0;
        let y = base[1] + 1.0;
        let vm = &mut self.vms[12];
        let mut stamp = |anm: &mut AnmManager, gfx: &mut crate::gfx::Gfx, x: f32, d: u32| {
            vm.set_sprite(anm, 0x88 + d as i32);
            vm.pos = [x, y, 0.0];
            anm.draw_mode(gfx, vm, 0);
        };
        let bonus = if self.capturable { self.bonus.max(0) as u32 } else { 0 };
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

thread_local! {
    static HISTORY: RefCell<HashMap<(i32, u8), (u32, u32)>> = RefCell::new(HashMap::new());
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
            HISTORY.with_borrow_mut(|h| *h = std::mem::take(&mut self.history));
        }
    }

    // Spell_Calc 0x416B90 (VM part).
    fn calc(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        for r in QUEUE.with_borrow_mut(std::mem::take) {
            self.handle(&mut ctx.anm, r);
        }
        let frozen = {
            let e = self.enemies.borrow();
            if let Some(s) = &e.g.spell {
                (self.bonus, self.capturable) = (s.bonus, s.capture);
            }
            self.player.borrow().freeze_bullets || e.g.timer_pause != 0
        };
        if !frozen {
            for k in CALC_ORDER {
                let vm = &mut self.vms[k];
                if vm.script.is_some() && vm.run(&mut ctx.anm) {
                    vm.script = None;
                }
            }
        }
        Ret::Next
    }

    // Spell_Draw 0x4178C0.
    fn draw(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        self.flush_texts(ctx);
        let (anm, gfx) = (&mut ctx.anm, &mut ctx.gfx);
        gfx.set_viewport(PLAYFIELD);
        if self.player.borrow().freeze_bullets {
            (anm.tint, anm.tint_on) = (0xFFF01010, true);
        }
        let v = &mut self.vms;
        if Self::shown(&v[0]) {
            Self::snap(anm, gfx, &mut v[0]);
            Self::snap(anm, gfx, &mut v[2]);
            Self::rot(anm, gfx, &mut v[4]);
        }
        if Self::shown(&v[1]) {
            let p = v[1].pos;
            let o = v[1].pos_offset;
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
