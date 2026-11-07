//! Main-menu list screens (calc, draw step): practice stage (12 §2.5), spell practice (16 §6), replay (12 §2.8, 15 §3.8).

use crate::anmvm::{self, AnmVm};
use crate::ascii::{Text, int, percent_glyphs, short, uidx};
use crate::builtin::Builtin;
use crate::drawlist::{Align, TextCmd, TextSite};
use crate::game::Ctx;
use crate::gamemgr::{F_PRACTICE, F_REPLAY, F_SPELL_PRACTICE};
use crate::input::{DOWN, LEFT, RIGHT, UP};
use crate::menu::{CANCEL, CONFIRM, MainMenu, Pending};
use crate::scoredat::Catk;
use crate::sound;
use crate::textref::{FmtArg, SpellField, Text as Txt, TextRef};
use crate::types::{D3dColor, lo8};
use crate::world::World;
use th08_formats::sjis::SjisString;

// resulttext.anm ANM slot (00 §3.2 loader thread); text.anm slot 0 (info strips).
pub const RT_SLOT: usize = 22;
const TEXT_SLOT: usize = 0;
// Anchor A = raw pos of title01 VM 141 (12 §2.5, 16 §6.4); unselected row colour.
const VM_ANCHOR: usize = 141;
const GREY: u32 = 0xFFA0_A0A0;

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
// Screen-16 row names, table 0x4C8274 (16 §6.4).
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
// Stage names 0x4C8250 (12 §2.5, §2.8), 7 chars; the 9th only in the replay detail.
const STAGES: [&str; 9] = [
    "Stage1 ", "Stage2 ", "Stage3 ", "Stage4A", "Stage4B", "Stage5 ", "Stage6A", "Stage6B",
    "StageEX",
];
// Replay list character / difficulty columns, tables 0x4C86C0 / 0x4C86F0 (12 §2.8).
pub(crate) const CHARS: [&str; 12] = [
    "Rm & Yk", "Ms & Al", "Sk & Rr", "Ym & Yy", "Reimu  ", "Yukari ", "Marisa ", "Alice  ",
    "Sakuya ", "Remilia", "Youmu  ", "Yuyuko ",
];
const DIFFS: [&str; 5] = ["Easy    ", "Normal  ", "Hard    ", "Lunatic ", "Extra   "];
// Easy / Normal / Hard card lists 0x4C6120 / 0x4C61C8 / 0x4C6290 (15 §2.8); the
// Lunatic list is every other id < 191.
const EASY: [u8; 42] = [
    2, 6, 13, 17, 21, 25, 32, 39, 43, 47, 54, 58, 62, 66, 70, 77, 81, 85, 89, 93, 100, 104, 108,
    112, 119, 123, 127, 131, 135, 139, 143, 147, 151, 155, 159, 163, 167, 171, 175, 179, 183, 187,
];
const NORMAL: [u8; 49] = [
    3, 7, 10, 14, 18, 22, 26, 29, 33, 36, 40, 44, 48, 51, 55, 59, 63, 67, 71, 74, 78, 82, 86, 90,
    94, 97, 101, 105, 109, 113, 116, 120, 124, 128, 132, 136, 140, 144, 148, 152, 156, 160, 164,
    168, 172, 176, 180, 184, 188,
];
const HARD: [u8; 50] = [
    0, 4, 8, 11, 15, 19, 23, 27, 30, 34, 37, 41, 45, 49, 52, 56, 60, 64, 68, 72, 75, 79, 83, 87,
    91, 95, 98, 102, 106, 110, 114, 117, 121, 125, 129, 133, 137, 141, 145, 149, 153, 157, 161,
    165, 169, 173, 177, 181, 185, 189,
];
// "Last" list 0x4C6B90, 43 cards (test 0x414540, 15 §2.8).
const LAST: [u8; 43] = [
    10, 11, 12, 29, 30, 31, 51, 52, 53, 74, 75, 76, 97, 98, 99, 116, 117, 118, 143, 144, 145, 146,
    171, 172, 173, 174, 175, 176, 177, 178, 179, 180, 181, 182, 183, 184, 185, 186, 187, 188, 189,
    190, 204,
];

// Spell_DifficultyOf 0x4144D0 (15 §2.8): list index 0..4, 5 for Last Words (ids ≥ 205).
pub(crate) fn difficulty_of(id: usize) -> usize {
    let has = |l: &[u8]| l.iter().any(|&v| v as usize == id);
    if has(&EASY) {
        0
    } else if has(&NORMAL) {
        1
    } else if has(&HARD) {
        2
    } else if id < 191 {
        3
    } else if id < 205 {
        4
    } else {
        5
    }
}

// Replay list paging (15 §3.8), only called with more than 15 files: ±15 rows, −15 < 0 → + count,
// +15 ≥ count → − count.
fn rpy_page(cur: i32, n: i32, left: bool) -> i32 {
    if left {
        let c = cur - 15;
        if c < 0 { c + n } else { c }
    } else {
        let c = cur + 15;
        if c >= n { c - n } else { c }
    }
}

// Replay sub-state 2 (15 §3.8): Menu_CursorUpDown(9) (wraps), then keep moving the same way while the stage has
// no record.
fn rpy_stage_step(have: &[bool; 10], cur: usize, d: i32) -> usize {
    if d == 0 || !have[..9].contains(&true) {
        return cur;
    }
    // Menu_CursorUpDown(9) wraps 0 ↔ 8; the skip loop wraps up −1 → 9 (have[9] = header +0x44), down 9 → 0.
    let mut c = int(cur) + d;
    if c < 0 {
        c = 8;
    } else if c >= 9 {
        c = 0;
    }
    while !have[uidx(c)] {
        c += d;
        if c < 0 {
            c = 9;
        } else if c > 8 && d > 0 {
            c = 0;
        }
    }
    uidx(c)
}

// Screen-17 confirm (16 §6.2): difficulty = Spell_DifficultyOf(id); a Last Word (5) plays on Normal (1) at the stage of
// the id table. Returns (difficulty, stage for Last Words).
fn spell_start(id: usize) -> (i32, Option<usize>) {
    match difficulty_of(id) {
        5 => (
            1,
            Some(match id {
                205 => 0,
                206 => 1,
                207 => 2,
                208 | 212 => 5,
                209 => 6,
                210 => 7,
                211 | 213 => 8,
                215 => 4,
                _ => 3,
            }),
        ),
        d => (int(d), None),
    }
}

fn is_last(id: usize) -> bool {
    LAST.iter().any(|&v| v as usize == id)
}

// LastWord_ComputeUnlocks 0x46CBBB (15 §2.8 table, slot k = card 205 + k): sets FLSP slots to 0xCD + k, never clears them.
// "Shots" count all 12 CLRD entries; captures/attempts use the all-shots index 12.
pub fn compute_last_word_unlocks(g: &mut crate::gamemgr::Game) {
    let c = |id: usize| g.score.catk.get(id).cloned().unwrap_or_default();
    let prac = |id: usize| c(id).counters[PC][12] != 0;
    let captured = |id: usize| c(id).counters[SC][12] != 0 || prac(id);
    let attempted = |id: usize| seen(&c(id), 12);
    let final_b = (0..12)
        .filter(|&t| g.clear_nc[t].iter().take(4).any(|m| m & 0x4000 != 0))
        .count();
    let prac_n = (0..222).filter(|&i| prac(i)).count();
    let last_n = LAST.iter().filter(|&&i| prac(i as usize)).count();
    let extra = (0..12).filter(|&t| g.clear_nc[t][4] & 0x100 != 0).count();
    let unlocked = [
        final_b >= 2,
        final_b >= 3,
        prac_n >= 50,
        final_b >= 4,
        captured(137),
        last_n >= 15,
        captured(195) && captured(145) && attempted(204),
        (208..=210).all(attempted),
        [205, 206, 207, 211].into_iter().all(attempted),
        NORMAL.iter().all(|&i| {
            let k = c(i as usize);
            k.counters[SC][6] != 0 || k.counters[PC][6] != 0
        }),
        g.clear_nc[4][2..4].iter().any(|m| m & 0x4000 != 0),
        prac_n >= 120,
        final_b >= 6,
        extra >= 3,
        last_n >= 30,
        g.clear_any[12][3] & 0xC000 != 0,
        (205..=220).all(attempted),
    ];
    for (k, u) in unlocked.into_iter().enumerate() {
        if u {
            g.score.flsp[k] = 0xCD + k as u8;
        }
    }
}

// CATK counter rows (15 §2.5): story attempts / captures, practice bonus / attempts / captures.
const SA: usize = 1;
const SC: usize = 2;
const PB: usize = 3;
const PA: usize = 4;
const PC: usize = 5;

// State of the list screens.
#[derive(Default, Clone)]
pub struct Lists {
    // Scratch VMs from menu+0xE7B8 (screen 16: E7B8 / EA5C / ED00; screen 17: 15 row VMs) and the screen they belong to.
    pub(crate) scratch: Vec<AnmVm>,
    scratch_for: i32,
    // Screen-17 header VM menu+0x10F54 and the 7 info VMs menu+0x11F2C (16 §6.3, §6.5).
    pub(crate) header: AnmVm,
    pub(crate) info: Vec<AnmVm>,
    // ascii.anm VM for Ascii_DrawGaugePercent.
    pub(crate) glyph: AnmVm,
    // Eased pie values +0xC2A0..+0xC2AC (16 §6.4).
    pub(crate) pies: [f32; 4],
    // Spell-practice stage row (g_StageIdx on screens 16/17).
    pub sp_row: i32,
    // Screen 16 in its input sub-state (16 §6.1).
    sp_input: bool,
    // Screen-17 cursor (index in the row's card list), page = card / 15.
    pub card: i32,
    pub(crate) page: i32,
    prev_card: i32,
    // Info refresh counter +0xC29C (16 §6.3).
    info_counter: u32,
    // Replay screen sub-state +0x0C (1 list, 2 stage, 3 speed), list row +0xC28C, displayed / chosen stage row
    // +0xC290, loaded file +0xC280. The cursor +0x00 is shared by all three sub-states (15 §3.8).
    pub(crate) rpy_sub: u8,
    rpy_file: i32,
    // Zeroed with the menu object (memset at MainMenu_Register); the replay screen entry keeps it (15 §3.8).
    rpy_stage: usize,
    pub(crate) rpy_loaded: Option<usize>,
    // Sub-state counter +0x10 of the list (sub-state 1): zeroed in call 8 and by a press, += 1 per call (12 §2).
    rpy_t: u32,
}

fn catk(w: &World, id: usize) -> Catk {
    w.game.score.catk.get(id).cloned().unwrap_or_default()
}

// Story or practice attempts of shot index `s` ≠ 0.
fn seen(c: &Catk, s: usize) -> bool {
    c.counters[SA][s] != 0 || c.counters[PA][s] != 0
}

// Card_IsSelectable 0x46FD5F: attempted (ids < 205) or the Last Word's FLSP byte == id (0xCD + k, 15 §2.8).
fn selectable(w: &World, id: usize) -> bool {
    if id < 205 {
        return seen(&catk(w, id), 12);
    }
    w.game
        .score
        .flsp
        .get(id - 205)
        .is_some_and(|&v| v as usize == id - 205 + 0xCD)
}

fn spell_ref(id: usize, field: SpellField) -> TextRef {
    TextRef::Spell {
        card: id as u16,
        field,
    }
}

// Shift-JIS bytes of a NUL-terminated score.dat / replay field.
fn sjis(b: &[u8]) -> SjisString {
    let n = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    SjisString(b[..n].to_vec())
}

// Three full-width digits with leading zeros (info line 0, 16 §6.3).
fn fullwidth_digits(n: usize) -> String {
    format!("{n:03}")
        .chars()
        .map(|c| char::from_u32(c as u32 - '0' as u32 + '０' as u32).unwrap_or(c))
        .collect()
}

// AnmVm_Init + sprite, top-left anchor (unless centred), white.
fn scratch(w: &World, slot: usize, sprite: i32, centred: bool) -> AnmVm {
    let mut vm = AnmVm::default();
    vm.init();
    vm.anm_slot = short(slot);
    vm.set_sprite(&w.anm, sprite);
    if !centred {
        vm.flags |= anmvm::F_ANCHOR_X | anmvm::F_ANCHOR_Y;
    }
    vm.color = [255; 4];
    vm
}

// The four percent readouts of 16 §6.4 step 6 in draw order: (pie value index, centre, colour).
pub(crate) const PERCENTS: [(usize, [f32; 2], [u8; 4]); 4] = [
    (0, [514.0, 404.0], [255; 4]),
    (1, [514.0, 412.0], 0x80C0_C080_u32.to_le_bytes()),
    (2, [580.0, 451.0], [255; 4]),
    (3, [580.0, 443.0], 0x80C0_C080_u32.to_le_bytes()),
];

// Pie value → percent ×100 for the glyph readout (16 §6.4 step 6).
pub(crate) fn pct(v: f32) -> i32 {
    (v * 10000.0) as i32
}

// Step half of a mode-0 draw at (x, y): only the position is written (mode 0 prepares nothing, 05 §9).
fn place(vm: &mut AnmVm, x: f32, y: f32) {
    vm.pos = [x, y, 0.0];
}

fn set_alpha(vm: &mut AnmVm, a: u8) {
    vm.color[anmvm::A] = a;
}

fn set_rgb(vm: &mut AnmVm, argb: u32) {
    vm.color[anmvm::R] = (argb >> 16) as u8;
    vm.color[anmvm::G] = (argb >> 8) as u8;
    vm.color[anmvm::B] = argb as u8;
}

// VM colour as D3DCOLOR 0xAARRGGBB (05 §2 byte order B, G, R, A).
fn vm_argb(vm: &AnmVm) -> u32 {
    D3dColor::from_bgra(vm.color).0
}

fn text(x: f32, y: f32, s: impl Into<String>, argb: u32, hl: bool) -> Text {
    Text::new(x, y, s).argb(argb).highlight(hl)
}

// Text_RenderToVm (white, outline 0, font 15, 16 §6.3 / §6.5) into the VM's sprite strip, then visible (10 §6).
#[expect(
    clippy::cast_sign_loss,
    reason = "texture row of v0 ≥ 0, rounded as before"
)]
pub(crate) fn render_strip(ctx: &mut Ctx, vm: &mut AnmVm, s: Txt) {
    let slot = uidx(i32::from(vm.anm_slot));
    let Some(sp) = ctx
        .w
        .anm
        .sprite(slot, uidx(i32::from(vm.sprite_index).max(0)))
        .copied()
    else {
        return;
    };
    // 10 §6: x = u0 = 0, W = texture-row width (512), 15 rows from v0; rows 15–16 of a 17-high sprite stay untouched.
    let t = ctx.rec.texture(sp.tex);
    let (w, ty) = (t.width, (sp.uv[1] * t.height as f32).round() as u32);
    ctx.rec.text(TextCmd {
        dst: sp.tex,
        y: ty,
        w,
        size: 15,
        text: s.raw,
        key: s.key,
        color: [255, 255, 255],
        x: 0.0,
        edge: sp.uv[0] * t.width as f32 + sp.w,
        outline: 0,
        align: Align::Left,
        site: TextSite::MenuList,
    });
    vm.flags |= anmvm::F_VISIBLE;
}

impl MainMenu {
    fn anchor(&self) -> [f32; 2] {
        let p = self.vms[VM_ANCHOR].pos;
        [p[0], p[1]]
    }

    fn glyph_vm(&mut self, ctx: &mut Ctx) {
        if self.lists.glyph.script.is_none() {
            ctx.w.anm.set_script(
                &mut ctx.w.rng,
                &mut self.lists.glyph,
                crate::ascii::ANM_SLOT,
                4,
            );
        }
    }

    // ---- screen 11 (12 §2.5) ----

    // Any-clear mask CLRD[shot].any[difficulty]: 0 → 1 (stage 1 only); the 6B bit adds 4A and 4B (12 §2.5).
    fn practice_mask(w: &World) -> u16 {
        let g = &w.game;
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

    // Entry (12 §2.5): range interrupt 18, portraits entries 1–3, +0x6C = 0, practice flag set (the +0x6C forward
    // reaches it with bit 0 cleared by MainMenu_OnRegister, 12 §2.2), cursor = g_StageIdx (0 if unavailable).
    fn practice_enter(&mut self, ctx: &mut Ctx) {
        {
            self.back = 0;
            ctx.w.game.flags |= F_PRACTICE;
            self.interrupt_all(18);
            self.char_vms_list(i32::from(ctx.w.game.shot));
            self.cursor = int(ctx.w.game.stage).clamp(0, 7);
            let m = Self::practice_mask(&ctx.w);
            self.stages = std::array::from_fn(|i| ctx.core.unlock || m & (1 << i) != 0);
            if !self.stages[uidx(self.cursor.clamp(0, 7))] {
                self.cursor = 0;
            }
        }
    }

    // Input from +0x1442C == 8 (12 §2).
    fn practice_run(&mut self, ctx: &mut Ctx) {
        if self.frames < 8 {
            return;
        }
        let stages = self.stages;
        self.move_cursor(ctx, 8, UP, DOWN, |_, c| stages[uidx(c)]);
        if ctx.w.input.pressed(CONFIRM) {
            ctx.w.sound.play(sound::OK);
            let g = &mut ctx.w.game;
            g.difficulty = i32::from(g.cfg.difficulty.min(3));
            g.stage = uidx(self.cursor);
            g.flags = g.flags & !F_REPLAY | F_PRACTICE;
            g.fresh_start = true;
            ctx.w.bgm.stop_bgm();
            g.request(crate::types::SceneId::Game);
            self.remove = true;
        } else if ctx.w.input.pressed(CANCEL) {
            ctx.w.sound.play(sound::CANCEL);
            // 11 → 9 returns 2: screen 9 runs in this frame (12 §2).
            self.set_screen(crate::menu::CHAR_PRACTICE);
            self.again = true;
        }
    }

    // MainMenu_PracticeStageDraw 0x46F429 (12 §2.5): header at A − (0, 96), rows `"%s %9d0 (%3d)"` from A − (0, 80),
    // PSCR hiscore / plays; cursor row white + highlight, others 0xFFA0A0A0 or 0xFF404040 (locked).
    // Draw step only: the rows are ASCII queue pushes, the draw records nothing itself.
    pub(crate) fn practice_stage_draw_step(&mut self, ctx: &mut Ctx) {
        let [ax, ay] = self.anchor();
        ctx.w
            .ascii
            .push(text(ax, ay - 96.0, "Stage    HI-Score", 0xFFFF_FFFF, false));
        let m = Self::practice_mask(&ctx.w);
        let g = &ctx.w.game;
        let (shot, d) = (g.shot as usize % 12, g.cfg.difficulty.min(4) as usize);
        let p = &g.score.pscr[shot];
        for (r, stage) in STAGES.iter().enumerate().take(8) {
            let cur = int(r) == self.cursor;
            let c = if cur {
                0xFFFF_FFFF
            } else if m & (1 << r) != 0 {
                GREY
            } else {
                0xFF40_4040
            };
            let s = format!("{} {:9}0 ({:3})", stage, p.hiscore[r][d], p.plays[r][d]);
            ctx.w
                .ascii
                .push(text(ax, ay - 80.0 + 16.0 * r as f32, s, c, cur));
        }
    }

    // ---- screen 16 (16 §6.1, §6.4) ----

    // Screen 16 offers shots whose team has a Final A clear (Clear_TeamFinalA); solo shots always pass (16 §6.1).
    pub(crate) fn sp_shot_ok(w: &World, unlock: bool, s: i32) -> bool {
        s >= 4 || crate::menu::team_final_a(&w.game, unlock, uidx(s))
    }

    pub(crate) fn spell_char(&mut self, ctx: &mut Ctx) {
        let n = if crate::menu::extra_all(&ctx.w.game, ctx.core.unlock) {
            12
        } else {
            4
        };
        if self.frames == 0 {
            self.spell_char_enter(ctx, n);
        }
        // Back from a spell-practice game: 16 → 17 at once, stage row 9 if the last card was a Last Word,
        // return 2 so screen 17 runs in this frame (16 §6.1).
        if self.back != 0 {
            if ctx.w.game.card >= 205 {
                ctx.w.game.stage = 9;
            }
            self.lists.sp_row = int(ctx.w.game.stage.min(9));
            // +0x00 := 0 before Menu_SetScreen(17) (16 §6.1).
            self.lists.card = 0;
            self.set_screen(crate::menu::SPELL_CARD);
            self.again = true;
            return;
        }
        self.spell_char_run(ctx, n);
    }

    // Screen 16 entry (16 §6.1, §6.4): select00.png, flags |= 0x4001, interrupt 18, cursor = first allowed shot,
    // stage row = g_StageIdx, scratch VMs E7B8 / EA5C / ED00 with their strips, pie values zeroed.
    fn spell_char_enter(&mut self, ctx: &mut Ctx, n: i32) {
        {
            self.lists.sp_row = int(ctx.w.game.stage.min(9));
            self.lists.sp_input = false;
            self.set_surface(ctx, "select00.png");
            ctx.w.game.flags |= F_PRACTICE | F_SPELL_PRACTICE;
            self.interrupt_all(18);
            let mut c = i32::from(ctx.w.game.shot).min(n - 1);
            for _ in 0..n {
                if Self::sp_shot_ok(&ctx.w, ctx.core.unlock, c) {
                    break;
                }
                c = (c + 1) % n;
            }
            self.cursor = c;
            self.prev_cursor = -1;
            self.lists.scratch_for = crate::menu::SPELL_PRACTICE;
            self.lists.scratch = vec![
                scratch(&ctx.w, RT_SLOT, 2, false),
                scratch(&ctx.w, RT_SLOT, 3, false),
                scratch(&ctx.w, crate::menu::ANM_SLOT, 144, false),
            ];
            // QUIRK (16 §6.4): the anchor bits go to the EA5C VM twice, so the pie frame stays centred.
            self.lists.scratch[2].flags &= !(anmvm::F_ANCHOR_X | anmvm::F_ANCHOR_Y);
            render_strip(ctx, &mut self.lists.scratch[0], Builtin::SpHeader.txt());
            render_strip(ctx, &mut self.lists.scratch[1], Builtin::SpPercent.txt());
            self.lists.pies = [0.0; 4];
            self.glyph_vm(ctx);
        }
        self.spell_char_shot(ctx, true);
    }

    // Portraits redone on entry and on every shot change; VMs 138/139 get pending 27 after the range interrupt (16 §6.1).
    // A shot change also zeroes +0x1442C, so the pie easing (16 §6.4 step 5) restarts.
    fn spell_char_shot(&mut self, ctx: &mut Ctx, entry: bool) {
        if self.cursor != self.prev_cursor {
            if !entry {
                self.frames = 0;
            }
            self.prev_cursor = self.cursor;
            ctx.w.game.shot = lo8(self.cursor);
            self.char_vms_list(self.cursor);
        }
        if entry {
            self.vms[138].pending_interrupt = 27;
            self.vms[139].pending_interrupt = 27;
        }
    }

    // Screen 16 controls (16 §6.1): Left/Right shot, Up/Down 10 stage rows, confirm → 17, Bomb → top (cursor 2) after 20 frames.
    // Input (sub-state 1) from +0x1442C == 8; a later shot change zeroes the counter but stays in sub-state 1 (16 §6.1).
    fn spell_char_run(&mut self, ctx: &mut Ctx, n: i32) {
        self.spell_char_shot(ctx, false);
        if self.frames >= 8 {
            self.lists.sp_input = true;
        }
        if !self.lists.sp_input || self.pending.is_some() {
            return;
        }
        self.move_cursor(ctx, n, LEFT, RIGHT, |ctx, c| {
            Self::sp_shot_ok(&ctx.w, ctx.core.unlock, c)
        });
        let inp = ctx.w.input;
        let d = if inp.pressed_or_repeat(UP) {
            -1
        } else {
            i32::from(inp.pressed_or_repeat(DOWN))
        };
        if d != 0 {
            self.lists.sp_row = (self.lists.sp_row + d).rem_euclid(10);
            ctx.w.sound.play(sound::SELECT);
        }
        if inp.pressed(CONFIRM) {
            ctx.w.sound.play(sound::OK);
            ctx.w.game.stage = uidx(self.lists.sp_row);
            // +0x00 := 0 before Menu_SetScreen(17); 16 → 17 returns 2: screen 17 runs in this frame (16 §6.1).
            self.lists.card = 0;
            self.set_screen(crate::menu::SPELL_CARD);
            self.again = true;
        } else if inp.pressed(CANCEL) {
            ctx.w.sound.play(sound::CANCEL);
            // Bomb also stores the row into g_StageIdx (16 §6.1).
            ctx.w.game.stage = uidx(self.lists.sp_row);
            self.interrupt_all(6);
            // flags &= ~1 only with the switch to the top menu at P+20, after cursor := 2 (16 §6.1).
            self.pending = Some(Pending {
                frames: 20,
                screen: crate::menu::TOP,
                cursor: 2,
                clear_flags: F_PRACTICE,
            });
        }
    }

    // MainMenu_SpellPracticeCharDraw 0x46F5CB (16 §6.4 steps 1–6).
    // Step: scratch VM positions, the ASCII rows, the pie easing (+0x1442C-driven, read by later frames) and the
    // glyph VM's writes; `spell_char_emit` records the VMs, pies and glyphs in the original order.
    pub(crate) fn spell_char_draw_step(&mut self, ctx: &mut Ctx) {
        let own = self.spell_char_own();
        let [ax, ay] = self.anchor();
        // The heading x (A.x − 32) is also the row pen x (16 §6.4 step 1).
        let x0 = ax - 32.0;
        if own {
            place(&mut self.lists.scratch[0], ax - 32.0, ay - 152.0);
        }
        let (tot, sc_shot, mut y) = self.spell_char_rows(ctx, x0, ay - 136.0);
        y += 5.0;
        let marker = if tot[0] == 222 {
            "@"
        } else if tot[1] == 222 {
            "*"
        } else {
            " "
        };
        ctx.w
            .ascii
            .push(text(x0, y, format!("{marker}Total"), 0xFFD0_6060, false));
        ctx.w.ascii.push(
            text(
                x0 + 182.0,
                y,
                format!("{:3}({:3})/{:3}/{:3}", tot[0], tot[1], tot[2], 222),
                0xFFD0_6060,
                false,
            )
            .scale_xy(0.75, 1.0),
        );
        if own {
            place(&mut self.lists.scratch[1], 400.0, 376.0);
            place(&mut self.lists.scratch[2], 552.0, 426.0);
        }
        self.spell_char_pies(ctx, tot, sc_shot);
    }

    // The 10 stage rows of screen 16 from y (16 §6.4 steps 2–3): per row pc (captured with the cursor's shot),
    // pca (any shot), seen and count. Returns the PC / PCA / seen totals, SC for the shot, and the pen y after the rows.
    fn spell_char_rows(&self, ctx: &mut Ctx, x0: f32, y0: f32) -> ([u32; 3], u32, f32) {
        let s = uidx(self.cursor.clamp(0, 11));
        let mut y = y0;
        let mut tot = [0u32; 3];
        let (mut sc_shot, mut sc_all) = (0u32, 0u32);
        for (r, &(a, b)) in ROW_IDS.iter().enumerate() {
            let count = (b - a) as u32;
            let (mut pc, mut pca, mut seen_all) = (0u32, 0u32, 0u32);
            for id in a..b {
                let c = catk(&ctx.w, id);
                pc += u32::from(c.counters[PC][s] != 0);
                pca += u32::from(c.counters[PC][12] != 0);
                let sn = if r > 8 {
                    selectable(&ctx.w, id)
                } else {
                    seen(&c, 12)
                };
                seen_all += u32::from(sn);
                if r <= 8 {
                    sc_shot += u32::from(c.counters[SC][s] != 0);
                    sc_all += u32::from(c.counters[SC][12] != 0);
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
            let cur = int(r) == self.lists.sp_row;
            let c = if cur { 0xFFFF_FFFF } else { GREY };
            ctx.w
                .ascii
                .push(text(x0, y, format!("{marker}{}", SP_ROWS[r]), c, cur));
            ctx.w.ascii.push(
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
        let _ = sc_all;
        (tot, sc_shot, y)
    }

    // Pie values v1..v4 eased into `lists.pies`, then the glyph VM's percent writes (16 §6.4 steps 5–6).
    fn spell_char_pies(&mut self, ctx: &mut Ctx, tot: [u32; 3], sc_shot: u32) {
        // Pie values v1..v4 (16 §6.4 step 5), eased over the first 50 screen frames.
        let target = [
            tot[0] as f32 / 222.0,
            sc_shot as f32 / 205.0,
            tot[1] as f32 / 222.0,
            (0..205)
                .filter(|&id| catk(&ctx.w, id).counters[SC][12] != 0)
                .count() as f32
                / 205.0,
        ];
        let f = self.frames as f32;
        for (k, &t) in target.iter().enumerate() {
            let v = if self.frames < 50 {
                let prev = self.lists.pies[k];
                prev + (t - prev) * f / 50.0
            } else {
                t
            };
            self.lists.pies[k] = v;
        }
        self.glyph_vm(ctx);
        let anm = &ctx.w.anm;
        for (k, z, c) in PERCENTS {
            percent_glyphs(
                anm,
                &mut self.lists.glyph,
                pct(self.lists.pies[k]),
                [z[0], z[1], 0.01],
                c,
                |_| {},
            );
        }
    }

    // The draw follows the screen variable, so the press frame already draws this screen (VMs not created yet).
    pub(crate) fn spell_char_own(&self) -> bool {
        self.lists.scratch_for == crate::menu::SPELL_PRACTICE && self.lists.scratch.len() >= 3
    }

    // ---- screen 17 (16 §6.2, §6.3, §6.5) ----

    pub(crate) fn card_ids(&self) -> (usize, usize) {
        ROW_IDS[uidx(self.lists.sp_row.clamp(0, 9))]
    }

    // Displayed by card id: the CATK bytes are only the vanilla text.
    fn row_name(w: &World, id: usize) -> Txt {
        let c = catk(w, id);
        if seen(&c, 12) {
            Txt::new(spell_ref(id, SpellField::Name), sjis(&c.name))
        } else if id >= 205 && selectable(w, id) {
            Builtin::SpWelcome.txt()
        } else {
            Builtin::Unknown8.txt()
        }
    }

    // The 15 row VMs (resulttext sprites 2..16) of the current page with their names (16 §6.5).
    fn card_rows(&mut self, ctx: &mut Ctx) {
        let (a, b) = self.card_ids();
        self.lists.scratch_for = crate::menu::SPELL_CARD;
        self.lists.scratch = (0..15)
            .map(|j| scratch(&ctx.w, RT_SLOT, j + 2, false))
            .collect();
        for j in 0..15 {
            let i = uidx(self.lists.page) * 15 + j;
            if a + i < b {
                let s = Self::row_name(&ctx.w, a + i);
                render_strip(ctx, &mut self.lists.scratch[j], s);
            }
        }
        self.card_colours();
    }

    fn card_colours(&mut self) {
        let cur = self.lists.card - self.lists.page * 15;
        for (j, vm) in self.lists.scratch.iter_mut().enumerate() {
            set_rgb(
                vm,
                if int(j) == cur {
                    0xFFFF_FFFF
                } else {
                    0xFF60_6060
                },
            );
        }
    }

    // Info panel line k (16 §6.3, 0x46D7F9).
    fn info_line(&self, w: &World, k: usize) -> Txt {
        let (a, _) = self.card_ids();
        let id = a + uidx(self.lists.card.max(0));
        let c = catk(w, id);
        let s = uidx(self.cursor.clamp(0, 11));
        let att = seen(&c, 12);
        let lw = id >= 205;
        // Name / owner bytes from score.dat, or the unknown placeholder.
        let field = |b: &[u8], f: SpellField| {
            FmtArg::Text(if att {
                Txt::new(spell_ref(id, f), sjis(b))
            } else {
                Builtin::Unknown8.txt()
            })
        };
        match k {
            0 => Builtin::SpNoLine.fmt(vec![
                FmtArg::Text(Txt::dynamic_str(&fullwidth_digits(id + 1))),
                field(&c.name, SpellField::Name),
            ]),
            1 => Builtin::SpOwnerLine.fmt(vec![
                field(&c.owner, SpellField::Owner),
                FmtArg::Text(Builtin::SpDiff(difficulty_of(id) as u8).txt()),
                FmtArg::Text(if is_last(id) {
                    Builtin::SpLast.txt()
                } else {
                    Txt::dynamic_str(" ")
                }),
            ]),
            2 => Builtin::SpScoreLine.fmt(vec![FmtArg::Text(Builtin::Shot(s as u8).txt())]),
            3 => {
                let t = if !att {
                    "　　---/---(---/---)[--------]　　---/---(---/---)[--------]".into()
                } else if lw {
                    let f = |s: usize| {
                        format!(
                            "{:3}/{:3}(---/---)[{:08}]",
                            c.counters[PC][s], c.counters[PA][s], c.counters[PB][s]
                        )
                    };
                    format!("　　{}　　{}", f(s), f(12))
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
                    format!("　　{}　　{}", f(s), f(12))
                };
                Txt::dynamic_str(&t)
            }
            4 => Builtin::SpComment.txt(),
            _ => {
                if id >= 204 && !att && !selectable(w, id) {
                    Builtin::SpHint {
                        card: id as u8,
                        line: (k - 5) as u8,
                    }
                    .txt()
                } else if c.counters[PC][12] != 0 {
                    Txt::new(
                        spell_ref(id, SpellField::Comment((k - 5) as u8)),
                        sjis(&c.comment[k - 5]),
                    )
                } else {
                    Builtin::Unknown9.txt()
                }
            }
        }
    }

    fn render_info(&mut self, ctx: &mut Ctx, k: usize) {
        let s = self.info_line(&ctx.w, k);
        render_strip(ctx, &mut self.lists.info[k], s);
    }

    // Screen 17, MainMenu_SpellPracticeCard 0x46BBC0 (16 §6.2); input from +0x1442C == 8 (12 §2).
    pub(crate) fn spell_card(&mut self, ctx: &mut Ctx) {
        let (a, b) = self.card_ids();
        let count = int(b - a);
        if self.frames == 0 {
            self.spell_card_setup(ctx);
        }
        if self.frames < 8 || self.pending.is_some() {
            return;
        }
        self.spell_card_input(ctx, a, count);
    }

    // Screen 17 entry (16 §6.2): range interrupt 26, LastWord_ComputeUnlocks, cursor on the last played id (0x164D0B8),
    // all 7 info lines rendered and shown at once; +0x6C = 0 (12 §2.2).
    fn spell_card_setup(&mut self, ctx: &mut Ctx) {
        let (a, b) = self.card_ids();
        let count = int(b - a);
        {
            self.back = 0;
            for vm in &mut self.vms {
                vm.pending_interrupt = 26;
            }
            compute_last_word_unlocks(&mut ctx.w.game);
            let id = i32::from(ctx.w.game.card) - int(a);
            self.lists.card = if (0..count).contains(&id) { id } else { 0 };
            self.lists.page = self.lists.card / 15;
            self.lists.prev_card = self.lists.card;
            self.card_rows(ctx);
            self.lists.header = scratch(&ctx.w, RT_SLOT, 17, false);
            render_strip(ctx, &mut self.lists.header, Builtin::SpListHeader.txt());
            self.lists.info = (0..7)
                .map(|k| scratch(&ctx.w, TEXT_SLOT, 21 + k, false))
                .collect();
            for k in 0..7 {
                self.render_info(ctx, k);
            }
            self.lists.info_counter = 0;
        }
    }

    fn spell_card_input(&mut self, ctx: &mut Ctx, a: usize, count: i32) {
        let inp = ctx.w.input;
        let mut c = self.lists.card;
        if inp.pressed_or_repeat(UP) {
            c = (c - 1).rem_euclid(count);
            ctx.w.sound.play(sound::SELECT);
        } else if inp.pressed_or_repeat(DOWN) {
            c = (c + 1).rem_euclid(count);
            ctx.w.sound.play(sound::SELECT);
        // Page keys only when count > 15 (16 §6.2); Left also flushes the sound queue (Sound_Update, 13 §1.3).
        } else if count > 15 && inp.pressed_or_repeat(LEFT) {
            c -= 15;
            if c < 0 {
                c = count - 1;
            }
            ctx.w.sound.play(sound::SELECT);
            ctx.sound_update();
        } else if count > 15 && inp.pressed_or_repeat(RIGHT) {
            if count % 15 < count - c {
                c = (c + 15).min(count - 1);
            } else {
                c %= 15;
            }
            ctx.w.sound.play(sound::SELECT);
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
        // SpellPractice_DrawInfo (16 §6.3): staggered re-render, all lines shown at counter 3, so the panel is blank for
        // 18 frames after a move.
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
        let id = a + uidx(self.lists.card);
        if inp.pressed(CONFIRM) {
            // Locked card: SE 0x29 (16 §6.2).
            let ok = seen(&catk(&ctx.w, id), 12) || selectable(&ctx.w, id);
            if !ok {
                ctx.w.sound.play(sound::INVALID);
                return;
            }
            ctx.w.sound.play(sound::OK);
            let g = &mut ctx.w.game;
            g.flags |= F_SPELL_PRACTICE;
            g.flags &= !F_REPLAY;
            g.card = short(id);
            let (d, st) = spell_start(id);
            g.difficulty = d;
            // Other cards keep g_StageIdx = the row set by the screen-16 confirm (16 §6.1).
            g.stage = st.unwrap_or(uidx(self.lists.sp_row));
            g.fresh_start = true;
            ctx.w.bgm.stop_bgm();
            g.request(crate::types::SceneId::Game);
            self.remove = true;
        } else if inp.pressed(CANCEL) {
            ctx.w.sound.play(sound::CANCEL);
            ctx.w.game.card = short(id);
            // +0x00 := g_StageIdx before Menu_SetScreen(16) (16 §6.2); 17 → 16 returns 2: screen 16 runs in this frame.
            self.lists.sp_row = int(ctx.w.game.stage.min(9));
            self.set_screen(crate::menu::SPELL_PRACTICE);
            self.again = true;
        }
    }

    // MainMenu_SpellPracticeCardDraw 0x46FFC0 (16 §6.5): header VM at (334, 78), rows from y 94, then the info VMs
    // at x 64 (lines 5/6 at x 96), y 344 + 16k (+8 from line 4).
    // Step: VM positions and the ASCII rows; `spell_card_emit` records header, rows, info lines.
    pub(crate) fn spell_card_draw_step(&mut self, ctx: &mut Ctx) {
        let (a, b) = self.card_ids();
        if !self.spell_card_own() {
            return;
        }
        place(&mut self.lists.header, 334.0, 78.0);
        let s = uidx(self.cursor.clamp(0, 11));
        for j in 0..15 {
            let i = uidx(self.lists.page) * 15 + j;
            if a + i >= b {
                break;
            }
            let id = a + i;
            let y = 94.0 + 16.0 * j as f32;
            let c = catk(&ctx.w, id);
            let col = vm_argb(&self.lists.scratch[j]);
            let marker = if c.counters[PC][s] != 0 {
                "@"
            } else if c.counters[PC][12] != 0 {
                "*"
            } else {
                " "
            };
            ctx.w.ascii.push(text(
                16.0,
                y,
                format!("{marker}No.{:03}", id + 1),
                col,
                false,
            ));
            ctx.w.ascii.push(
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
            place(&mut self.lists.scratch[j], 118.0, y);
        }
        for (k, vm) in self.lists.info.iter_mut().enumerate() {
            let x = if k >= 5 { 96.0 } else { 64.0 };
            let y = 344.0 + 16.0 * k as f32 + if k >= 4 { 8.0 } else { 0.0 };
            place(vm, x, y);
        }
    }

    pub(crate) fn spell_card_own(&self) -> bool {
        self.lists.scratch_for == crate::menu::SPELL_CARD && self.lists.scratch.len() >= 15
    }

    // ---- screen 7 (12 §2.8, 15 §3.8) ----

    // Listing (15 §3.8): slots th8_01..15, then up to 45 th8_ud????.rpy files; unreadable files are skipped.
    fn load_replays(&mut self, ver: &crate::scoredat::VerList, host: &mut dyn crate::host::Host) {
        let mut names: Vec<String> = (1..=15).map(|i| format!("replay/th8_{i:02}.rpy")).collect();
        let mut ud: Vec<String> = host
            .list("replay/th8_ud")
            .into_iter()
            .filter(|n| n.ends_with(".rpy"))
            .collect();
        ud.sort();
        names.extend(ud.into_iter().take(45));
        self.replays = names
            .into_iter()
            .filter_map(|n| {
                let r = crate::replay::parse(&host.read(&n)?, ver.get()).ok()?;
                Some((n, r))
            })
            .collect();
    }

    // The list-row VM 80 + (row mod 15) (12 §2.8).
    fn rpy_row_vm(&self) -> usize {
        80 + uidx(self.lists.rpy_file.max(0) % 15)
    }

    // Speed plates VMs 108–110: 21 = not selected, 20 = selected (12 §2.8). Spell replays: VM 110 gets no interrupt,
    // only its alpha byte := 0, here and not every frame, so it never shows.
    fn rpy_speed_vms(&mut self, spell: bool) {
        for k in 0..3 {
            if spell && k == 2 {
                set_alpha(&mut self.vms[110], 0);
            } else {
                self.vms[108 + k].pending_interrupt = 21;
            }
        }
        self.vms[108 + uidx(self.cursor.clamp(0, 2))].pending_interrupt = 20;
    }

    // Choosing a file: 15 to all, then 17 to the chosen row's VM (12 §2.8).
    fn rpy_choose(&mut self) {
        self.interrupt_all(15);
        let v = self.rpy_row_vm();
        self.vms[v].pending_interrupt = 17;
    }

    // Screen 7, MainMenu_Replay 0x46E136 (12 §2.8, 15 §3.8). Returns false for the handler's return 0.
    pub(crate) fn replay(&mut self, ctx: &mut Ctx) -> bool {
        let r = self.replay_run(ctx);
        self.lists.rpy_t += 1;
        r
    }

    fn replay_run(&mut self, ctx: &mut Ctx) -> bool {
        if self.frames == 0 {
            self.set_surface(ctx, "select00.png");
            self.interrupt_all(14);
            self.desc_cur = None;
            self.load_replays(&ctx.w.game.ver_list, &mut *ctx.host);
            self.lists.rpy_sub = 1;
            self.lists.rpy_loaded = None;
            if self.prev_screen == crate::menu::TOP {
                self.cursor = 0;
            }
            self.cursor = self.cursor.clamp(0, (int(self.replays.len()) - 1).max(0));
        }
        // Sub-state 0 in calls 0–7; sub-state 1 starts in call 8 with +0x10 := 0, so the list moves from call 9 and
        // takes confirm / cancel from call 18 (+0x10 > 9) (15 §3.8).
        if self.frames == 8 {
            self.lists.rpy_t = 0;
        }
        if self.frames < 9 || self.pending.is_some() {
            return true;
        }
        let n = int(self.replays.len());
        let inp = ctx.w.input;
        match self.lists.rpy_sub {
            1 => self.replay_files(ctx, n, &inp),
            2 => self.replay_stages(ctx, &inp),
            _ => self.replay_speed(ctx, &inp),
        }
    }

    // Sub-state 1, file list (15 §3.8); false = scene −1 (broken file).
    fn replay_files(&mut self, ctx: &mut Ctx, n: i32, inp: &crate::input::Input) -> bool {
        if n > 0 {
            self.move_cursor(ctx, n, UP, DOWN, |_, _| true);
        }
        // Left / Right page by 15 only with more than 15 files, wrapping by the count; SE 12 (15 §3.8).
        if n > 15 && (inp.pressed_or_repeat(LEFT) || inp.pressed_or_repeat(RIGHT)) {
            self.cursor = rpy_page(self.cursor, n, inp.pressed_or_repeat(LEFT));
            ctx.w.sound.play(sound::SELECT);
        }
        // +0xC28C := cursor every sub-state-1 frame, after the paging (15 §3.8).
        self.lists.rpy_file = self.cursor;
        // The +0x10 > 9 gate is armed once per visit: only the list cancel zeroes +0x10 again; the file
        // confirm and the cancels 2 → 1 / 3 → 2 leave it counting (15 §3.8).
        if self.lists.rpy_t <= 9 {
            return true;
        }
        if inp.pressed(CONFIRM) && n > 0 {
            ctx.w.sound.play(sound::OK);
            let i = uidx(self.cursor);
            // Stage cursor = first stage with a record; +0xC290 keeps its old value until sub-state 2. None:
            // error log line, scene −1, return 0 (15 §3.8).
            let st = &self.replays[i].1.stages;
            let Some(first) = (0..9).find(|&s| st[s].is_some()) else {
                log::error!("リプレイデータが異常");
                ctx.w.game.request(crate::types::SceneId::Quit);
                return false;
            };
            self.lists.rpy_loaded = Some(i);
            // Card-name scratch VM menu+0xE7B8 (12 §2.8 step 3): header +0x7E into resulttext sprite 11, set up
            // for every file, drawn only for spell replays.
            // Shown by the header's card id +0x7C; −1 outside spell practice.
            let r = &self.replays[i].1;
            let name = match u16::try_from(r.spell_id()) {
                Ok(card) => Txt::new(
                    TextRef::Spell {
                        card,
                        field: SpellField::Name,
                    },
                    sjis(&r.card_name()),
                ),
                Err(_) => Txt::dynamic(sjis(&r.card_name())),
            };
            self.notice = scratch(&ctx.w, RT_SLOT, 11, false);
            render_strip(ctx, &mut self.notice, name);
            self.cursor = int(first);
            self.lists.rpy_sub = 2;
            self.rpy_choose();
        } else if inp.pressed(CANCEL) {
            ctx.w.sound.play(sound::CANCEL);
            self.lists.rpy_t = 0;
            self.interrupt_all(16);
            // Replay leave: top menu (cursor 4) after 30 frames (12 §2).
            self.pending = Some(Pending {
                frames: 30,
                screen: crate::menu::TOP,
                cursor: 4,
                clear_flags: 0,
            });
        }
        true
    }

    // Sub-state 2, stage choice of the loaded file (15 §3.8).
    fn replay_stages(&mut self, ctx: &mut Ctx, inp: &crate::input::Input) -> bool {
        let Some(i) = self.lists.rpy_loaded else {
            return true;
        };
        // Index 9 = header +0x44, read by the up-direction skip loop's −1 → 9 wrap (15 §3.8).
        let r = &self.replays[i].1;
        let have: [bool; 10] =
            std::array::from_fn(|s| r.stages.get(s).map_or(r.fps0 != 0, Option::is_some));
        let d = if inp.pressed_or_repeat(UP) {
            -1
        } else {
            i32::from(inp.pressed_or_repeat(DOWN))
        };
        if d != 0 {
            ctx.w.sound.play(sound::SELECT);
        }
        // +0xC290 is written only here, so the row shown on the file-confirm frame lags by one (15 §3.8).
        self.cursor = int(rpy_stage_step(&have, uidx(self.cursor.max(0)), d));
        self.lists.rpy_stage = uidx(self.cursor);
        if inp.pressed(CONFIRM) {
            ctx.w.sound.play(sound::OK);
            let spell = r.spell_id() >= 0;
            self.lists.rpy_sub = 3;
            self.cursor = 0;
            self.interrupt_all(19);
            let v = self.rpy_row_vm();
            self.vms[v].pending_interrupt = 17;
            self.rpy_speed_vms(spell);
        } else if inp.pressed(CANCEL) {
            ctx.w.sound.play(sound::CANCEL);
            // The cancels zero +0x1442C, which only sub-state 0 reads, so the entry block never reruns (15 §3.8).
            self.lists.rpy_sub = 1;
            self.cursor = self.lists.rpy_file;
            self.lists.rpy_loaded = None;
            self.interrupt_all(14);
        }
        true
    }

    // Sub-state 3, playback speed (15 §3.8 Start).
    fn replay_speed(&mut self, ctx: &mut Ctx, inp: &crate::input::Input) -> bool {
        let Some(i) = self.lists.rpy_loaded else {
            return true;
        };
        let spell = self.replays[i].1.spell_id() >= 0;
        let m = if spell { 2 } else { 3 };
        let old = self.cursor;
        if inp.pressed_or_repeat(UP) {
            self.cursor = (old - 1).rem_euclid(m);
        } else if inp.pressed_or_repeat(DOWN) {
            self.cursor = (old + 1).rem_euclid(m);
        }
        if self.cursor != old {
            ctx.w.sound.play(sound::SELECT);
            self.rpy_speed_vms(spell);
        }
        if inp.pressed(CONFIRM) {
            ctx.w.sound.play(sound::OK);
            ctx.w.game.replay_speed = lo8(self.cursor);
            // The loaded replay stays listed: this frame's draw still shows its detail rows.
            let r = self.replays[i].1.clone();
            let st = Some(self.lists.rpy_stage);
            if crate::replay::start_playback(&mut ctx.w.game, r, st, None) {
                ctx.w.bgm.stop_bgm();
            }
            // Return 0: nothing of the menu is drawn in the confirm frame (15 §3.8 Start).
            self.remove = true;
        } else if inp.pressed(CANCEL) {
            ctx.w.sound.play(sound::CANCEL);
            self.lists.rpy_sub = 2;
            self.cursor = int(self.lists.rpy_stage);
            self.rpy_choose();
        }
        true
    }

    // MainMenu_ReplayDraw 0x46EF8D (12 §2.8 draw steps 1–3); text anchors are the VMs' raw positions.
    // Step: ASCII rows and the card-name VM position; `replay_emit` records that VM.
    pub(crate) fn replay_draw_step(&mut self, ctx: &mut Ctx) {
        let at = |m: &Self, k: usize| (m.vms[k].pos[0], m.vms[k].pos[1]);
        let (x, y) = at(self, 79);
        ctx.w.ascii.push(text(
            x,
            y,
            "No.   Name       Date  Player   Rank",
            0xFFFF_FFFF,
            false,
        ));
        let n = self.replays.len();
        // Page and highlight follow +0xC28C, not the shared cursor (12 §2.8 draw step 2).
        let page = uidx(self.lists.rpy_file.max(0) / 15 * 15);
        for j in 0..15 {
            let i = page + j;
            if i >= n {
                break;
            }
            let (name, r) = &self.replays[i];
            let label = match name
                .trim_start_matches("replay/th8_")
                .trim_end_matches(".rpy")
            {
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
            let cur = int(i) == self.lists.rpy_file;
            let (x, y) = at(self, 80 + j);
            ctx.w.ascii.push(text(
                x,
                y,
                head + &tail,
                if cur { 0xFFFF_FFFF } else { 0xFF80_8080 },
                cur,
            ));
        }
        if let (2 | 3, Some(i)) = (self.lists.rpy_sub, self.lists.rpy_loaded) {
            let r = &self.replays[i].1;
            let (x, y) = at(self, 78);
            ctx.w.ascii.push(text(
                x,
                y,
                format!("       {:2.3}%", r.slowdown()),
                0xFFFF_FFFF,
                false,
            ));
            let (x, y) = at(self, 95);
            ctx.w
                .ascii
                .push(text(x, y, "Stage    LastScore", 0xFFFF_FFFF, false));
            if r.spell_id() >= 0 {
                let (x, y) = at(self, 97);
                ctx.w.ascii.push(text(
                    x,
                    y,
                    format!("No.{:03} {:9}0", r.spell_id() + 1, r.score()),
                    0xFFFF_FFFF,
                    false,
                ));
                // The card name under the row: scratch VM at VM 97's pos + (0, 16), top-left, white (12 §2.8).
                place(&mut self.notice, x, y + 16.0);
            } else {
                for (s, name) in STAGES.iter().enumerate() {
                    let line = match &r.stages[s] {
                        Some(st) => format!("{} {:9}0", name, st.score),
                        None => format!("{name} ----------"),
                    };
                    let chosen = s == self.lists.rpy_stage;
                    let (c, hl) = match (self.lists.rpy_sub, chosen) {
                        (2, true) => (0xFFFF_FFFF, true),
                        (2, false) => (0xFF80_8080, false),
                        (_, true) => (0x60FF_FFFF, false),
                        _ => (0x6080_8080, false),
                    };
                    let (x, y) = at(self, 96 + s);
                    ctx.w.ascii.push(text(x, y, line, c, hl));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Text commands carry Shift-JIS; every port literal must round-trip so the raster sees the same characters.
    fn round_trips(s: &str) {
        assert_eq!(th08_formats::sjis::encode(s).decode(), s);
    }

    #[test]
    fn literals_are_sjis() {
        round_trips("Ｎｏ．０１２　　");
        // The info-line formats rebuild the bytes of the original concatenation (16 §6.3).
        let name = Txt::dynamic(SjisString(vec![0x83, 0x41, b'X']));
        let t = Builtin::SpNoLine.fmt(vec![
            FmtArg::Text(Txt::dynamic_str(&fullwidth_digits(12))),
            FmtArg::Text(name.clone()),
        ]);
        let mut want = th08_formats::sjis::encode("Ｎｏ．０１２　　");
        want.push_bytes(&name.raw);
        assert_eq!(t.raw, want);
        let t = Builtin::SpOwnerLine.fmt(vec![
            FmtArg::Text(name.clone()),
            FmtArg::Text(Builtin::SpDiff(5).txt()),
            FmtArg::Text(Builtin::SpLast.txt()),
        ]);
        let mut want = th08_formats::sjis::encode("使用者  ");
        want.push_bytes(&name.raw);
        want.push_str("　　極意（ラストワード） Last");
        assert_eq!(t.raw, want);
        round_trips("取得数/挑戦数[最高点]　　全主人公合計");
    }

    // 16 §6.2: difficulty = Spell_DifficultyOf(id), not the card's last attempt; Last Words → Normal + table stage.
    #[test]
    fn spell_start_difficulty() {
        assert_eq!(spell_start(EASY[0] as usize), (0, None));
        assert_eq!(spell_start(NORMAL[0] as usize), (1, None));
        assert_eq!(spell_start(HARD[0] as usize), (2, None));
        assert_eq!(spell_start(1), (3, None));
        assert_eq!(spell_start(191), (4, None));
        assert_eq!(spell_start(204), (4, None));
        let lw: Vec<_> = (205..222).map(spell_start).collect();
        assert!(lw.iter().all(|&(d, _)| d == 1));
        let st: Vec<usize> = lw.iter().map(|&(_, s)| s.unwrap()).collect();
        assert_eq!(st, [0, 1, 2, 5, 6, 7, 8, 5, 8, 3, 4, 3, 3, 3, 3, 3, 3]);
    }

    // 12 §2.8: spell replays send VM 110 no interrupt, only alpha 0; normal replays send it 21.
    #[test]
    fn speed_plates_spell_replay() {
        let mut m = crate::menu::MainMenu::new();
        m.vms = (0..142).map(|_| AnmVm::default()).collect();
        m.vms[110].color[anmvm::A] = 96;
        m.cursor = 1;
        m.rpy_speed_vms(true);
        let ints: Vec<_> = (108..111).map(|k| m.vms[k].pending_interrupt).collect();
        assert_eq!(ints, [21, 20, 0]);
        assert_eq!(m.vms[110].color[anmvm::A], 0);
        m.rpy_speed_vms(false);
        assert_eq!(m.vms[110].pending_interrupt, 21);
    }

    // 15 §3.8: Menu_CursorUpDown(9) wraps, then skips stages without a record in the same direction.
    #[test]
    fn replay_stage_cursor() {
        let mut have = [false; 10];
        have[1] = true;
        have[4] = true;
        have[8] = true;
        assert_eq!(rpy_stage_step(&have, 1, 0), 1);
        assert_eq!(rpy_stage_step(&have, 1, 1), 4);
        assert_eq!(rpy_stage_step(&have, 4, 1), 8);
        assert_eq!(rpy_stage_step(&have, 8, 1), 1);
        assert_eq!(rpy_stage_step(&have, 1, -1), 8);
        assert_eq!(rpy_stage_step(&[false; 10], 0, 1), 0);
        // Up from the first record wraps −1 → 9 and tests header +0x44: 0 → on to 8, ≠ 0 → stops on row 9.
        assert_eq!(rpy_stage_step(&have, 1, -1), 8);
        have[0] = false;
        have[9] = true;
        assert_eq!(rpy_stage_step(&have, 1, -1), 9);
        assert_eq!(rpy_stage_step(&have, 9, 1), 1);
        assert_eq!(rpy_stage_step(&have, 8, 1), 1);
    }

    // 15 §3.8: ±15 rows, wrapping by the file count.
    #[test]
    fn replay_paging() {
        assert_eq!(rpy_page(3, 20, true), 8);
        assert_eq!(rpy_page(16, 20, true), 1);
        assert_eq!(rpy_page(3, 20, false), 18);
        assert_eq!(rpy_page(7, 20, false), 2);
        assert_eq!(rpy_page(0, 16, true), 1);
    }
}
