// Main-menu list screens: practice stage select (12 §2.5), spell practice 16/17 (16 §6), replay (12 §2.8, 15 §3.8).
use crate::anmvm::{self, AnmVm};
use crate::ascii::Text;
use crate::game::Ctx;
use crate::gamemgr::{self, F_PRACTICE, F_REPLAY, F_SPELL_PRACTICE};
use crate::gfx::{Blend, Vertex};
use crate::input::{DOWN, LEFT, RIGHT, UP};
use crate::menu::{CANCEL, CONFIRM, MainMenu, Pending};
use crate::scoredat::Catk;
use crate::sound;

pub const RT_SLOT: usize = 21;
const TEXT_SLOT: usize = 0;
const VM_ANCHOR: usize = 141;
const GREY: u32 = 0xFFA0A0A0;

// Card id ranges per spell-practice row (table 0x4C6C40, 16 §6.2): stages 1, 2, 3, 4A, 4B, 5, 6A, 6B, Extra, Last Word.
const ROW_IDS: [(usize, usize); 10] = [
    (0, 13),
    (13, 32),
    (32, 54),
    (54, 77),
    (77, 100),
    (100, 119),
    (119, 147),
    (147, 191),
    (191, 205),
    (205, 222),
];
const SP_ROWS: [&str; 10] = [
    "Stage1   ",
    "Stage2   ",
    "Stage3   ",
    "Stage4A  ",
    "Stage4B  ",
    "Stage5   ",
    "Stage6A  ",
    "Stage6B  ",
    "Extra    ",
    "Last Word",
];
const STAGES: [&str; 9] = [
    "Stage1 ", "Stage2 ", "Stage3 ", "Stage4A", "Stage4B", "Stage5 ", "Stage6A", "Stage6B", "StageEX",
];
const CHARS: [&str; 12] = [
    "Rm & Yk", "Ms & Al", "Sk & Rr", "Ym & Yy", "Reimu  ", "Yukari ", "Marisa ", "Alice  ", "Sakuya ",
    "Remilia", "Youmu  ", "Yuyuko ",
];
const DIFFS: [&str; 5] = ["Easy    ", "Normal  ", "Hard    ", "Lunatic ", "Extra   "];
// SPEC-GAP: the shot names (info line 2) and the difficulty names (table 0x4C8628) are not in the spec; only
// 「三日月（ノーマル）」 is visible in mopup1_sp17_d.png.
const SHOT_NAMES: [&str; 12] = [
    "霊夢＆紫",
    "魔理沙＆アリス",
    "咲夜＆レミリア",
    "妖夢＆幽々子",
    "霊夢",
    "紫",
    "魔理沙",
    "アリス",
    "咲夜",
    "レミリア",
    "妖夢",
    "幽々子",
];
const DIFF_NAMES: [&str; 6] = [
    "（イージー）",
    "三日月（ノーマル）",
    "（ハード）",
    "（ルナティック）",
    "（エキストラ）",
    "（ラストワード）",
];
const UNKNOWN8: &str = "？？？？？？？？";
const UNKNOWN9: &str = "？？？？？？？？？";

// CATK counters (15 §2.5).
const SA: usize = 1;
const SC: usize = 2;
const PB: usize = 3;
const PA: usize = 4;
const PC: usize = 5;

#[derive(Default)]
pub struct Lists {
    scratch: Vec<AnmVm>,
    scratch_for: i32,
    header: AnmVm,
    info: Vec<AnmVm>,
    glyph: AnmVm,
    pies: [f32; 4],
    pub sp_row: i32,
    card: i32,
    page: i32,
    prev_card: i32,
    info_counter: u32,
    rpy_sub: u8,
    rpy_stage: usize,
    rpy_speed: i32,
    rpy_loaded: Option<usize>,
}

fn catk(ctx: &Ctx, id: usize) -> Catk {
    ctx.game.score.catk.get(id).cloned().unwrap_or_default()
}

fn seen(c: &Catk, s: usize) -> bool {
    c.counters[SA][s] != 0 || c.counters[PA][s] != 0
}

// Card_IsSelectable 0x46FD5F: a Last Word whose FLSP slot is set.
fn selectable(ctx: &Ctx, id: usize) -> bool {
    id >= 205 && ctx.game.score.flsp.get(id - 205).is_some_and(|&v| v != 0)
}

fn sjis(b: &[u8]) -> String {
    let n = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    crate::text::decode_sjis(&b[..n])
}

fn fullwidth_digits(n: usize) -> String {
    format!("{n:03}")
        .chars()
        .map(|c| char::from_u32(c as u32 - '0' as u32 + '０' as u32).unwrap_or(c))
        .collect()
}

// AnmVm_Init + sprite, top-left anchor (unless centred), white.
fn scratch(ctx: &Ctx, slot: usize, sprite: i32, centred: bool) -> AnmVm {
    let mut vm = AnmVm::default();
    vm.init();
    vm.anm_slot = slot as _;
    vm.set_sprite(&ctx.anm, sprite);
    if !centred {
        vm.flags |= anmvm::F_ANCHOR_X | anmvm::F_ANCHOR_Y;
    }
    vm.color = [255; 4];
    vm
}

fn draw_at(ctx: &mut Ctx, vm: &mut AnmVm, x: f32, y: f32) {
    vm.pos = [x, y, 0.0];
    ctx.anm.draw_mode(&mut ctx.gfx, vm, 0);
}

fn set_alpha(vm: &mut AnmVm, a: u8) {
    vm.color[anmvm::A] = a;
}

fn set_rgb(vm: &mut AnmVm, argb: u32) {
    vm.color[anmvm::R] = (argb >> 16) as u8;
    vm.color[anmvm::G] = (argb >> 8) as u8;
    vm.color[anmvm::B] = argb as u8;
}

fn vm_argb(vm: &AnmVm) -> u32 {
    let c = vm.color;
    (c[anmvm::A] as u32) << 24
        | (c[anmvm::R] as u32) << 16
        | (c[anmvm::G] as u32) << 8
        | c[anmvm::B] as u32
}

fn text(x: f32, y: f32, s: impl Into<String>, argb: u32, hl: bool) -> Text {
    Text::new(x, y, s).argb(argb).highlight(hl)
}

// Menu_DrawPie 0x46FDC2 → AnmManager_DrawFan: 64-vertex fan from 12 o'clock, clockwise over v·360° (16 §6.4).
fn pie(ctx: &mut Ctx, c: [f32; 2], r: f32, z: f32, argb: u32, v: f32) {
    let col = [(argb >> 16) as u8, (argb >> 8) as u8, argb as u8, (argb >> 24) as u8];
    let p = |x: f32, y: f32| Vertex::screen(x + 0.5, y + 0.5, z, [0.0, 0.0], col);
    let mut verts = vec![p(c[0], c[1])];
    let step = 0.101342 * v;
    for k in 0..63 {
        let a = -std::f32::consts::FRAC_PI_2 + k as f32 * step;
        verts.push(p(c[0] + r * a.cos(), c[1] + r * a.sin()));
    }
    ctx.gfx.fan(Blend::Alpha, &verts);
}

impl MainMenu {
    fn anchor(&self) -> [f32; 2] {
        let p = self.vms[VM_ANCHOR].pos;
        [p[0], p[1]]
    }

    // Text_RenderToVm (white, black outline, font 15) into the VM's sprite strip.
    fn render_vm(&mut self, ctx: &mut Ctx, slot: usize, sprite: i32, s: &str) {
        let Some(sp) = ctx.anm.sprite(slot, sprite.max(0) as usize).copied() else {
            return;
        };
        let t = ctx.gfx.texture(sp.tex);
        let (tx, ty) = (
            (sp.uv[0] * t.width as f32).round() as u32,
            (sp.uv[1] * t.height as f32).round() as u32,
        );
        let (w, h) = (sp.w as usize, sp.h as usize);
        let mut rgba = self.raster.render(s, [255, 255, 255], w, 15);
        rgba.resize(w * h * 4, 0);
        ctx.gfx
            .write_texture(sp.tex, tx, ty, w as u32, h as u32, &rgba);
    }

    fn glyph_vm(&mut self, ctx: &mut Ctx) {
        if self.lists.glyph.script.is_none() {
            ctx.anm
                .set_script(&mut self.lists.glyph, crate::ascii::ANM_SLOT, 4);
        }
    }

    // ---- screen 11 (12 §2.5) ----

    fn practice_mask(ctx: &Ctx) -> u16 {
        let g = &ctx.game;
        let mut m = g.clear_any[g.shot as usize % 12][g.cfg.difficulty.min(3) as usize];
        if m == 0 {
            m = 1;
        }
        if m & 0x80 != 0 {
            m |= 0x18;
        }
        m
    }

    pub(crate) fn practice_stage(&mut self, ctx: &mut Ctx) {
        if self.frames == 0 {
            self.practice_enter(ctx);
        }
        self.practice_run(ctx);
    }

    // Run from the character screen on the confirm frame (mopup1 log: the list slides in from the press frame).
    pub(crate) fn practice_enter(&mut self, ctx: &mut Ctx) {
        {
            self.interrupt_all(18);
            // SPEC-GAP (mopup1 log): the portraits ignore the range interrupt, the description disappears at once,
            // and the cursor starts on the last stage index.
            for v in 111..=130 {
                self.vms[v].pending_interrupt = 0;
                if v >= 119 {
                    set_alpha(&mut self.vms[v], 0);
                }
            }
            self.cursor = (ctx.game.stage as i32).clamp(0, 7);
            let m = Self::practice_mask(ctx);
            self.stages =
                std::array::from_fn(|i| crate::menu::unlock_all() || m & (1 << i) != 0);
            if !self.stages[self.cursor.clamp(0, 7) as usize] {
                self.cursor = 0;
            }
        }
    }

    fn practice_run(&mut self, ctx: &mut Ctx) {
        if self.frames < 8 {
            return;
        }
        let stages = self.stages;
        self.move_cursor(ctx, 8, UP, DOWN, |c| stages[c as usize]);
        if ctx.input.pressed(CONFIRM) {
            sound::play(sound::OK);
            let g = &mut ctx.game;
            g.difficulty = g.cfg.difficulty.min(3) as i32;
            g.stage = self.cursor as usize;
            g.flags = g.flags & !F_REPLAY | F_PRACTICE;
            g.fresh_start = true;
            ctx.audio.stop_bgm();
            g.request(gamemgr::SCENE_GAME);
        } else if ctx.input.pressed(CANCEL) {
            sound::play(sound::CANCEL);
            self.set_screen(crate::menu::CHAR_PRACTICE);
        }
    }

    // MainMenu_PracticeStageDraw 0x46F429.
    pub(crate) fn practice_stage_draw(&mut self, ctx: &mut Ctx) {
        let [ax, ay] = self.anchor();
        ctx.ascii
            .push(text(ax, ay - 96.0, "Stage    HI-Score", 0xFFFFFFFF, false));
        let m = Self::practice_mask(ctx);
        let g = &ctx.game;
        let (shot, d) = (g.shot as usize % 12, g.cfg.difficulty.min(4) as usize);
        let p = &g.score.pscr[shot];
        for r in 0..8 {
            let cur = r as i32 == self.cursor;
            let c = if cur {
                0xFFFFFFFF
            } else if m & (1 << r) != 0 {
                GREY
            } else {
                0xFF404040
            };
            let s = format!(
                "{} {:9}0 ({:3})",
                STAGES[r], p.hiscore[r][d], p.plays[r][d]
            );
            ctx.ascii
                .push(text(ax, ay - 80.0 + 16.0 * r as f32, s, c, cur));
        }
    }

    // ---- screen 16 (16 §6.1, §6.4) ----

    fn sp_shot_ok(ctx: &Ctx, s: i32) -> bool {
        s >= 4 || crate::menu::team_final_a(&ctx.game, s as usize)
    }

    pub(crate) fn spell_char(&mut self, ctx: &mut Ctx) {
        let n = if crate::menu::extra_all(&ctx.game) {
            12
        } else {
            4
        };
        if self.frames == 0 && self.lists.scratch_for != crate::menu::SPELL_PRACTICE {
            self.spell_char_enter(ctx, n);
        }
        self.spell_char_run(ctx, n);
    }

    fn spell_char_enter(&mut self, ctx: &mut Ctx, n: i32) {
        {
            self.set_surface(ctx, "select00.png");
            ctx.game.flags |= F_PRACTICE | F_SPELL_PRACTICE;
            // SPEC-GAP: VM 141 restarts its slide from (640, 320) on every entry (mopup1 log, also back from 17).
            self.vms[VM_ANCHOR].pos = [640.0, 320.0, 0.0];
            self.interrupt_all(18);
            let mut c = (ctx.game.shot as i32).min(n - 1);
            for _ in 0..n {
                if Self::sp_shot_ok(ctx, c) {
                    break;
                }
                c = (c + 1) % n;
            }
            self.cursor = c;
            self.prev_cursor = -1;
            self.lists.scratch_for = crate::menu::SPELL_PRACTICE;
            self.lists.scratch = vec![
                scratch(ctx, RT_SLOT, 2, false),
                scratch(ctx, RT_SLOT, 3, false),
                scratch(ctx, crate::menu::ANM_SLOT, 144, false),
            ];
            // QUIRK (16 §6.4): the anchor bits go to the EA5C VM twice, so the pie frame stays centred.
            self.lists.scratch[2].flags &= !(anmvm::F_ANCHOR_X | anmvm::F_ANCHOR_Y);
            self.render_vm(
                ctx,
                RT_SLOT,
                2,
                "ステージ    取得/挑戦可能/総数（()内は全キャラ合計）",
            );
            self.render_vm(ctx, RT_SLOT, 3, "取得率　（キャラ別／総合）");
            self.lists.pies = [0.0; 4];
            self.glyph_vm(ctx);
        }
        self.spell_char_shot(ctx, true);
    }

    fn spell_char_shot(&mut self, ctx: &mut Ctx, entry: bool) {
        if self.cursor != self.prev_cursor {
            self.prev_cursor = self.cursor;
            ctx.game.shot = self.cursor as u8;
            // Portraits only: the description VMs keep the range interrupt (18 on entry, none later).
            self.char_vms(self.cursor);
            for vm in &mut self.vms[119..=130] {
                vm.pending_interrupt = if entry { 18 } else { 0 };
            }
        }
        if entry {
            // SPEC-GAP: heading VMs (title01 scripts 138/139) per the mopup1 log: 138 fades in at once, 139 stays hidden.
            self.vms[138].pending_interrupt = 27;
            self.vms[139].pending_interrupt = 26;
        }
    }

    fn spell_char_run(&mut self, ctx: &mut Ctx, n: i32) {
        self.spell_char_shot(ctx, false);
        if self.frames < 8 || self.pending.is_some() {
            return;
        }
        self.move_cursor(ctx, n, LEFT, RIGHT, |c| Self::sp_shot_ok(ctx, c));
        let inp = ctx.input;
        let d = if inp.pressed_or_repeat(UP) {
            -1
        } else if inp.pressed_or_repeat(DOWN) {
            1
        } else {
            0
        };
        if d != 0 {
            self.lists.sp_row = (self.lists.sp_row + d).rem_euclid(10);
            sound::play(sound::SELECT);
        }
        if inp.pressed(CONFIRM) {
            sound::play(sound::OK);
            ctx.game.stage = self.lists.sp_row as usize;
            self.set_screen(crate::menu::SPELL_CARD);
            // The card list is built (and drawn) on the press frame (mopup1 log).
            self.spell_card_setup(ctx);
        } else if inp.pressed(CANCEL) {
            sound::play(sound::CANCEL);
            self.interrupt_all(6);
            ctx.game.flags &= !F_PRACTICE;
            self.pending = Some(Pending {
                frames: 20,
                screen: crate::menu::TOP,
                cursor: 2,
            });
        }
    }

    // MainMenu_SpellPracticeCharDraw 0x46F5CB.
    pub(crate) fn spell_char_draw(&mut self, ctx: &mut Ctx) {
        // The draw follows the screen variable, so the press frame already draws this screen (VMs not created yet).
        let own = self.lists.scratch_for == crate::menu::SPELL_PRACTICE && self.lists.scratch.len() >= 3;
        let [ax, ay] = self.anchor();
        // SPEC-GAP: the rows sit 32 px left of VM 141 in mopup1 (x 224 at rest), like VM E7B8.
        let x0 = ax - 32.0;
        if own {
            let mut vm = std::mem::take(&mut self.lists.scratch[0]);
            draw_at(ctx, &mut vm, ax - 32.0, ay - 152.0);
            self.lists.scratch[0] = vm;
        }
        let s = self.cursor.clamp(0, 11) as usize;
        let mut y = ay - 136.0;
        let mut tot = [0u32; 3];
        let (mut sc_shot, mut sc_all) = (0u32, 0u32);
        for (r, &(a, b)) in ROW_IDS.iter().enumerate() {
            let count = (b - a) as u32;
            let (mut pc, mut pca, mut seen_all) = (0u32, 0u32, 0u32);
            for id in a..b {
                let c = catk(ctx, id);
                pc += (c.counters[PC][s] != 0) as u32;
                pca += (c.counters[PC][12] != 0) as u32;
                let sn = if r > 8 {
                    selectable(ctx, id)
                } else {
                    seen(&c, 12)
                };
                seen_all += sn as u32;
                if r <= 8 {
                    sc_shot += (c.counters[SC][s] != 0) as u32;
                    sc_all += (c.counters[SC][12] != 0) as u32;
                }
            }
            tot[0] += pc;
            tot[1] += pca;
            tot[2] += seen_all;
            let marker = if pc == count {
                "@"
            } else if pca == count {
                "*"
            } else {
                " "
            };
            let cur = r as i32 == self.lists.sp_row;
            let c = if cur { 0xFFFFFFFF } else { GREY };
            ctx.ascii
                .push(text(x0, y, format!("{marker}{}", SP_ROWS[r]), c, cur));
            ctx.ascii.push(
                text(
                    x0 + 182.0,
                    y,
                    format!("{pc:3}({pca:3})/{seen_all:3}/{count:3}"),
                    c,
                    cur,
                )
                .scale_xy(0.75, 1.0),
            );
            y += 16.0;
        }
        y += 5.0;
        let marker = if tot[0] == 222 {
            "@"
        } else if tot[1] == 222 {
            "*"
        } else {
            " "
        };
        ctx.ascii
            .push(text(x0, y, format!("{marker}Total"), 0xFFD06060, false));
        ctx.ascii.push(
            text(
                x0 + 182.0,
                y,
                format!("{:3}({:3})/{:3}/{:3}", tot[0], tot[1], tot[2], 222),
                0xFFD06060,
                false,
            )
            .scale_xy(0.75, 1.0),
        );
        if own {
            let mut vm = std::mem::take(&mut self.lists.scratch[1]);
            draw_at(ctx, &mut vm, 400.0, 376.0);
            self.lists.scratch[1] = vm;
            let mut vm = std::mem::take(&mut self.lists.scratch[2]);
            draw_at(ctx, &mut vm, 552.0, 426.0);
            self.lists.scratch[2] = vm;
        }
        let _ = sc_all;
        let target = [
            tot[0] as f32 / 222.0,
            sc_shot as f32 / 205.0,
            tot[1] as f32 / 222.0,
            (0..205)
                .filter(|&id| catk(ctx, id).counters[SC][12] != 0)
                .count() as f32
                / 205.0,
        ];
        let f = self.frames as f32;
        for k in 0..4 {
            let v = if self.frames < 50 {
                let prev = self.lists.pies[k];
                prev + (target[k] - prev) * f / 50.0
            } else {
                target[k]
            };
            self.lists.pies[k] = v;
        }
        let v = self.lists.pies;
        self.glyph_vm(ctx);
        let mut glyph = std::mem::take(&mut self.lists.glyph);
        let white = [255; 4];
        let green = 0x80C0C080u32.to_le_bytes();
        pie(ctx, [530.0, 420.0], 24.0, 0.02, 0x60C0C0F0, v[0]);
        pie(ctx, [530.0, 420.0], 12.0, 0.01, 0x80C0C0F0, v[1]);
        let pct = |v: f32| (v * 10000.0) as i32;
        crate::ascii::draw_percent(
            &mut ctx.anm,
            &mut ctx.gfx,
            &mut glyph,
            pct(v[0]),
            514.0,
            404.0,
            0.01,
            white,
        );
        crate::ascii::draw_percent(
            &mut ctx.anm,
            &mut ctx.gfx,
            &mut glyph,
            pct(v[1]),
            514.0,
            412.0,
            0.01,
            green,
        );
        pie(ctx, [562.0, 438.0], 16.0, 0.02, 0x6080C0C0, v[2]);
        pie(ctx, [562.0, 438.0], 8.0, 0.01, 0x8080C0C0, v[3]);
        crate::ascii::draw_percent(
            &mut ctx.anm,
            &mut ctx.gfx,
            &mut glyph,
            pct(v[2]),
            580.0,
            451.0,
            0.01,
            white,
        );
        crate::ascii::draw_percent(
            &mut ctx.anm,
            &mut ctx.gfx,
            &mut glyph,
            pct(v[3]),
            580.0,
            443.0,
            0.01,
            green,
        );
        self.lists.glyph = glyph;
    }

    // ---- screen 17 (16 §6.2, §6.3, §6.5) ----

    fn card_ids(&self) -> (usize, usize) {
        ROW_IDS[self.lists.sp_row.clamp(0, 9) as usize]
    }

    fn row_name(ctx: &Ctx, id: usize) -> String {
        let c = catk(ctx, id);
        if seen(&c, 12) {
            sjis(&c.name)
        } else if id >= 205 && selectable(ctx, id) {
            "挑戦者歓迎中！".into()
        } else {
            UNKNOWN8.into()
        }
    }

    fn card_rows(&mut self, ctx: &mut Ctx) {
        let (a, b) = self.card_ids();
        self.lists.scratch_for = crate::menu::SPELL_CARD;
        self.lists.scratch = (0..15)
            .map(|j| scratch(ctx, RT_SLOT, j + 2, false))
            .collect();
        for j in 0..15 {
            let i = self.lists.page as usize * 15 + j;
            if a + i < b {
                let s = Self::row_name(ctx, a + i);
                self.render_vm(ctx, RT_SLOT, j as i32 + 2, &s);
            }
        }
        self.card_colours();
    }

    fn card_colours(&mut self) {
        let cur = self.lists.card - self.lists.page * 15;
        for (j, vm) in self.lists.scratch.iter_mut().enumerate() {
            set_rgb(
                vm,
                if j as i32 == cur {
                    0xFFFFFFFF
                } else {
                    0xFF606060
                },
            );
        }
    }

    fn info_line(&self, ctx: &Ctx, k: usize) -> String {
        let (a, _) = self.card_ids();
        let id = a + self.lists.card.max(0) as usize;
        let c = catk(ctx, id);
        let s = self.cursor.clamp(0, 11) as usize;
        let att = seen(&c, 12);
        let lw = id >= 205;
        match k {
            0 => format!(
                "Ｎｏ．{}　　{}",
                fullwidth_digits(id + 1),
                if att { sjis(&c.name) } else { UNKNOWN8.into() }
            ),
            1 => {
                // SPEC-GAP: Spell_DifficultyOf needs the per-difficulty card lists (not in the spec); the CATK
                // difficulty of the last attempt is used, and the "Last" marker (list 0x4C6B90) is not shown.
                let d = if lw { 5 } else { c.difficulty.min(4) as usize };
                if att {
                    format!("使用者  {}　　{} ", sjis(&c.owner), DIFF_NAMES[d])
                } else {
                    format!("使用者  {UNKNOWN8}")
                }
            }
            2 => format!("取得数/挑戦数[最高点]　{}　全主人公合計", SHOT_NAMES[s]),
            3 => {
                if !att {
                    "---/---(---/---)[--------]".into()
                } else if lw {
                    let f = |s: usize| {
                        format!(
                            "{:3}/{:3}(---/---)[{:08}]",
                            c.counters[PC][s], c.counters[PA][s], c.counters[PB][s]
                        )
                    };
                    format!("{}    {}", f(s), f(12))
                } else {
                    let f = |s: usize| {
                        format!(
                            "{:3}/{:3}({:3}/{:3})[{:08}]",
                            c.counters[PC][s],
                            c.counters[PA][s],
                            c.counters[SC][s],
                            c.counters[SA][s],
                            c.counters[PB][s]
                        )
                    };
                    format!("{}    {}", f(s), f(12))
                }
            }
            4 => "カードの補足".into(),
            // SPEC-GAP: the unlock-hint texts of table 0x4C82C8 are not in the spec; locked cards show the
            // unknown line.
            _ => {
                if c.counters[PC][12] != 0 {
                    sjis(&c.comment[k - 5])
                } else {
                    UNKNOWN9.into()
                }
            }
        }
    }

    fn render_info(&mut self, ctx: &mut Ctx, k: usize) {
        let s = self.info_line(ctx, k);
        self.render_vm(ctx, TEXT_SLOT, 21 + k as i32, &s);
    }

    pub(crate) fn spell_card(&mut self, ctx: &mut Ctx) {
        let (a, b) = self.card_ids();
        let count = (b - a) as i32;
        if self.frames == 0 && self.lists.scratch_for != crate::menu::SPELL_CARD {
            self.spell_card_setup(ctx);
        }
        if self.frames < 8 || self.pending.is_some() {
            return;
        }
        self.spell_card_input(ctx, a, count);
    }

    fn spell_card_setup(&mut self, ctx: &mut Ctx) {
        let (a, b) = self.card_ids();
        let count = (b - a) as i32;
        {
            // SPEC-GAP: interrupt 26 (portraits of the chosen team dim, heading 140 in) as seen in the mopup1 log.
            for v in 136..=140 {
                self.vms[v].pending_interrupt = 26;
            }
            let row = crate::menu::CHAR_VMS[self.cursor.clamp(0, 11) as usize];
            for &v in &row[1..] {
                if (111..=118).contains(&v) {
                    self.vms[v as usize].pending_interrupt = 26;
                }
            }
            // SPEC-GAP: LastWord_ComputeUnlocks (15 §2.8) needs the card lists; FLSP is used as loaded.
            let id = ctx.game.card as i32 - a as i32;
            self.lists.card = if (0..count).contains(&id) { id } else { 0 };
            self.lists.page = self.lists.card / 15;
            self.lists.prev_card = self.lists.card;
            self.card_rows(ctx);
            self.lists.header = scratch(ctx, RT_SLOT, 17, false);
            self.render_vm(
                ctx,
                RT_SLOT,
                17,
                "取得数/挑戦数（()内は本編での集計）",
            );
            self.lists.info = (0..7)
                .map(|k| scratch(ctx, TEXT_SLOT, 21 + k, false))
                .collect();
            for k in 0..7 {
                self.render_info(ctx, k);
            }
            self.lists.info_counter = 0;
        }
    }

    fn spell_card_input(&mut self, ctx: &mut Ctx, a: usize, count: i32) {
        let inp = ctx.input;
        let mut c = self.lists.card;
        if inp.pressed_or_repeat(UP) {
            c = (c - 1).rem_euclid(count);
            sound::play(sound::SELECT);
        } else if inp.pressed_or_repeat(DOWN) {
            c = (c + 1).rem_euclid(count);
            sound::play(sound::SELECT);
        } else if count > 15 && inp.pressed_or_repeat(LEFT) {
            c -= 15;
            if c < 0 {
                c = count - 1;
            }
            sound::play(sound::SELECT);
        } else if count > 15 && inp.pressed_or_repeat(RIGHT) {
            if count % 15 < count - c {
                c = (c + 15).min(count - 1);
            } else {
                c %= 15;
            }
            sound::play(sound::SELECT);
        }
        if c != self.lists.card {
            self.lists.card = c;
            if c / 15 != self.lists.page {
                self.lists.page = c / 15;
                self.card_rows(ctx);
            }
            self.card_colours();
            for vm in &mut self.lists.info {
                set_alpha(vm, 0);
            }
            self.lists.info_counter = 21;
        }
        // SpellPractice_DrawInfo: staggered re-render, all lines shown at counter 3.
        if self.lists.info_counter != 0 {
            match self.lists.info_counter {
                11 => self.render_info(ctx, 0),
                9 => self.render_info(ctx, 1),
                7 => self.render_info(ctx, 3),
                5 => self.render_info(ctx, 5),
                3 => {
                    self.render_info(ctx, 6);
                    for vm in &mut self.lists.info {
                        set_alpha(vm, 255);
                    }
                }
                _ => {}
            }
            self.lists.info_counter -= 1;
        }
        let id = a + self.lists.card as usize;
        if inp.pressed(CONFIRM) {
            let ok = seen(&catk(ctx, id), 12) || selectable(ctx, id);
            if !ok {
                sound::play(sound::INVALID);
                return;
            }
            sound::play(sound::OK);
            let last_d = catk(ctx, id).difficulty as i32;
            let g = &mut ctx.game;
            g.flags |= F_SPELL_PRACTICE;
            g.flags &= !F_REPLAY;
            g.card = id as i16;
            if id >= 205 {
                g.difficulty = 1;
                g.stage = match id {
                    205 => 0,
                    206 => 1,
                    207 => 2,
                    208 | 212 => 5,
                    209 => 6,
                    210 => 7,
                    211 | 213 => 8,
                    215 => 4,
                    _ => 3,
                };
            } else {
                // SPEC-GAP: Spell_DifficultyOf (see info line 1).
                g.difficulty = if self.lists.sp_row == 8 {
                    4
                } else {
                    last_d.min(3)
                };
                g.stage = self.lists.sp_row as usize;
            }
            g.fresh_start = true;
            ctx.audio.stop_bgm();
            g.request(gamemgr::SCENE_GAME);
        } else if inp.pressed(CANCEL) {
            sound::play(sound::CANCEL);
            ctx.game.card = id as i16;
            self.set_screen(crate::menu::SPELL_PRACTICE);
            // Screen 16 is rebuilt (and drawn) on the press frame (mopup1 log).
            let n = if crate::menu::extra_all(&ctx.game) {
                12
            } else {
                4
            };
            self.spell_char_enter(ctx, n);
            self.frames = 1;
        }
    }

    // MainMenu_SpellPracticeCardDraw 0x46FFC0.
    pub(crate) fn spell_card_draw(&mut self, ctx: &mut Ctx) {
        let own = self.lists.scratch_for == crate::menu::SPELL_CARD && self.lists.scratch.len() >= 15;
        let (a, b) = self.card_ids();
        if !own {
            return;
        }
        let mut header = std::mem::take(&mut self.lists.header);
        draw_at(ctx, &mut header, 334.0, 78.0);
        self.lists.header = header;
        let s = self.cursor.clamp(0, 11) as usize;
        for j in 0..15 {
            let i = self.lists.page as usize * 15 + j;
            if a + i >= b {
                break;
            }
            let id = a + i;
            let y = 94.0 + 16.0 * j as f32;
            let c = catk(ctx, id);
            let col = vm_argb(&self.lists.scratch[j]);
            let marker = if c.counters[PC][s] != 0 {
                "@"
            } else if c.counters[PC][12] != 0 {
                "*"
            } else {
                " "
            };
            ctx.ascii
                .push(text(16.0, y, format!("{marker}No.{:03}", id + 1), col, false));
            ctx.ascii.push(
                text(
                    430.0,
                    y,
                    format!(
                        "{:3}/{:3}({:3}/{:3})",
                        c.counters[PC][s], c.counters[PA][s], c.counters[SC][s], c.counters[SA][s]
                    ),
                    col,
                    false,
                )
                .scale_xy(0.8, 1.0),
            );
            let mut vm = std::mem::take(&mut self.lists.scratch[j]);
            draw_at(ctx, &mut vm, 118.0, y);
            self.lists.scratch[j] = vm;
        }
        let mut info = std::mem::take(&mut self.lists.info);
        for (k, vm) in info.iter_mut().enumerate() {
            let x = if k >= 5 { 96.0 } else { 64.0 };
            let y = 344.0 + 16.0 * k as f32 + if k >= 4 { 8.0 } else { 0.0 };
            draw_at(ctx, vm, x, y);
        }
        self.lists.info = info;
    }

    // ---- screen 7 (12 §2.8, 15 §3.8) ----

    fn load_replays(&mut self) {
        let mut names: Vec<String> = (1..=15).map(|i| format!("replay/th8_{i:02}.rpy")).collect();
        let mut ud: Vec<String> = crate::scoredat::storage_list("replay/th8_ud")
            .into_iter()
            .filter(|n| n.ends_with(".rpy"))
            .collect();
        ud.sort();
        names.extend(ud.into_iter().take(45));
        self.replays = names
            .into_iter()
            .filter_map(|n| {
                let r = crate::replay::Replay::parse(&crate::scoredat::storage_read(&n)?).ok()?;
                Some((n, r))
            })
            .collect();
    }

    fn rpy_row_vm(&self) -> usize {
        80 + (self.cursor.max(0) % 15) as usize
    }

    fn rpy_speed_vms(&mut self) {
        for k in 0..3 {
            self.vms[108 + k].pending_interrupt = 21;
        }
        self.vms[108 + self.lists.rpy_speed as usize].pending_interrupt = 20;
    }

    fn rpy_choose(&mut self) {
        self.interrupt_all(15);
        let v = self.rpy_row_vm();
        self.vms[v].pending_interrupt = 17;
    }

    pub(crate) fn replay(&mut self, ctx: &mut Ctx) {
        if self.frames == 0 {
            self.set_surface(ctx, "select00.png");
            self.interrupt_all(14);
            self.load_replays();
            self.lists.rpy_sub = 1;
            self.lists.rpy_loaded = None;
            if self.prev_screen == crate::menu::TOP {
                self.cursor = 0;
            }
            self.cursor = self.cursor.clamp(0, (self.replays.len() as i32 - 1).max(0));
        }
        if self.frames < 8 || self.pending.is_some() {
            return;
        }
        let n = self.replays.len() as i32;
        let inp = ctx.input;
        match self.lists.rpy_sub {
            1 => {
                if n > 0 {
                    self.move_cursor(ctx, n, UP, DOWN, |_| true);
                    if inp.pressed_or_repeat(LEFT) || inp.pressed_or_repeat(RIGHT) {
                        let d = if inp.pressed_or_repeat(LEFT) { -15 } else { 15 };
                        self.cursor = (self.cursor + d).clamp(0, n - 1);
                        sound::play(sound::SELECT);
                    }
                }
                if inp.pressed(CONFIRM) && n > 0 {
                    sound::play(sound::OK);
                    let i = self.cursor as usize;
                    self.lists.rpy_loaded = Some(i);
                    // The stage row starts at 0 and snaps to a recorded stage on the next frame (mopup1 log).
                    self.lists.rpy_stage = 0;
                    self.lists.rpy_sub = 2;
                    self.rpy_choose();
                } else if inp.pressed(CANCEL) {
                    sound::play(sound::CANCEL);
                    self.interrupt_all(16);
                    self.pending = Some(Pending {
                        frames: 30,
                        screen: crate::menu::TOP,
                        cursor: 4,
                    });
                }
            }
            2 => {
                let Some(i) = self.lists.rpy_loaded else {
                    return;
                };
                let have: Vec<usize> = (0..9)
                    .filter(|&s| self.replays[i].1.stages[s].is_some())
                    .collect();
                if !have.contains(&self.lists.rpy_stage) {
                    self.lists.rpy_stage = have.first().copied().unwrap_or(0);
                }
                let k = have
                    .iter()
                    .position(|&s| s == self.lists.rpy_stage)
                    .unwrap_or(0);
                let m = have.len().max(1);
                let d = if inp.pressed_or_repeat(UP) {
                    m - 1
                } else if inp.pressed_or_repeat(DOWN) {
                    1
                } else {
                    0
                };
                if d != 0 {
                    sound::play(sound::SELECT);
                    if let Some(&s) = have.get((k + d) % m) {
                        self.lists.rpy_stage = s;
                    }
                }
                if inp.pressed(CONFIRM) {
                    sound::play(sound::OK);
                    self.lists.rpy_sub = 3;
                    self.lists.rpy_speed = 0;
                    self.interrupt_all(19);
                    let v = self.rpy_row_vm();
                    self.vms[v].pending_interrupt = 17;
                    self.rpy_speed_vms();
                } else if inp.pressed(CANCEL) {
                    sound::play(sound::CANCEL);
                    self.lists.rpy_sub = 1;
                    self.lists.rpy_loaded = None;
                    self.interrupt_all(14);
                }
            }
            _ => {
                let Some(i) = self.lists.rpy_loaded else {
                    return;
                };
                let spell = self.replays[i].1.spell_id() >= 0;
                let m = if spell { 2 } else { 3 };
                let old = self.lists.rpy_speed;
                if inp.pressed_or_repeat(UP) {
                    self.lists.rpy_speed = (old - 1).rem_euclid(m);
                } else if inp.pressed_or_repeat(DOWN) {
                    self.lists.rpy_speed = (old + 1).rem_euclid(m);
                }
                if self.lists.rpy_speed != old {
                    sound::play(sound::SELECT);
                    self.rpy_speed_vms();
                }
                if spell {
                    set_alpha(&mut self.vms[110], 0);
                }
                if inp.pressed(CONFIRM) {
                    sound::play(sound::OK);
                    ctx.game.replay_speed = self.lists.rpy_speed as u8;
                    let (_, r) = self.replays.swap_remove(i);
                    let st = Some(self.lists.rpy_stage);
                    if crate::replay::start_playback(&mut ctx.game, r, st, None) {
                        ctx.audio.stop_bgm();
                    }
                } else if inp.pressed(CANCEL) {
                    sound::play(sound::CANCEL);
                    self.lists.rpy_sub = 2;
                    self.rpy_choose();
                }
            }
        }
    }

    // MainMenu_ReplayDraw 0x46EF8D.
    pub(crate) fn replay_draw(&mut self, ctx: &mut Ctx) {
        let at = |m: &Self, k: usize| (m.vms[k].pos[0], m.vms[k].pos[1]);
        let (x, y) = at(self, 79);
        ctx.ascii.push(text(
            x,
            y,
            "No.   Name       Date  Player   Rank",
            0xFFFFFFFF,
            false,
        ));
        let n = self.replays.len();
        let page = (self.cursor.max(0) / 15 * 15) as usize;
        for j in 0..15 {
            let i = page + j;
            if i >= n {
                break;
            }
            let (name, r) = &self.replays[i];
            let label = match name.trim_start_matches("replay/th8_").trim_end_matches(".rpy") {
                s if s.starts_with("ud") => "User ".to_string(),
                s => format!("No.{s:>2}"),
            };
            let head = format!(
                "{label} {:>8}  {:>6} {:>7}  ",
                r.name(),
                r.date(),
                CHARS[r.shot() as usize % 12]
            );
            let tail = if r.spell_id() >= 0 {
                format!("Spell No.{:3}", r.spell_id() + 1)
            } else {
                format!("{:>8}", DIFFS[r.difficulty().min(4) as usize])
            };
            let cur = i as i32 == self.cursor;
            let (x, y) = at(self, 80 + j);
            ctx.ascii.push(text(
                x,
                y,
                head + &tail,
                if cur { 0xFFFFFFFF } else { 0xFF808080 },
                cur,
            ));
        }
        if let (2 | 3, Some(i)) = (self.lists.rpy_sub, self.lists.rpy_loaded) {
            let r = &self.replays[i].1;
            let (x, y) = at(self, 78);
            ctx.ascii.push(text(
                x,
                y,
                format!("       {:2.3}%", r.slowdown()),
                0xFFFFFFFF,
                false,
            ));
            let (x, y) = at(self, 95);
            ctx.ascii
                .push(text(x, y, "Stage    LastScore", 0xFFFFFFFF, false));
            if r.spell_id() >= 0 {
                // SPEC-GAP: the card-name scratch VM (resulttext sprite 11) under the spell row is not drawn.
                let (x, y) = at(self, 97);
                ctx.ascii.push(text(
                    x,
                    y,
                    format!("No.{:03} {:9}0", r.spell_id() + 1, r.score()),
                    0xFFFFFFFF,
                    false,
                ));
            } else {
                for s in 0..9 {
                    let line = match &r.stages[s] {
                        Some(st) => format!("{} {:9}0", STAGES[s], st.score),
                        None => format!("{} ----------", STAGES[s]),
                    };
                    let chosen = s == self.lists.rpy_stage;
                    let (c, hl) = match (self.lists.rpy_sub, chosen) {
                        (2, true) => (0xFFFFFFFF, true),
                        (2, false) => (0xFF808080, false),
                        (_, true) => (0x60FFFFFF, false),
                        _ => (0x60808080, false),
                    };
                    let (x, y) = at(self, 96 + s);
                    ctx.ascii.push(text(x, y, line, c, hl));
                }
            }
        }
    }
}
