//! Main-menu list screens: practice stage select (12 §2.5), spell practice 16/17 (16 §6), replay (12 §2.8, 15 §3.8).

use crate::anmvm::{self, AnmVm};
use crate::ascii::Text;
use crate::game::Ctx;
use crate::gamemgr::{self, F_PRACTICE, F_REPLAY, F_SPELL_PRACTICE};
use crate::gfx::{Blend, Vertex};
use crate::input::{DOWN, LEFT, RIGHT, UP};
use crate::menu::{CANCEL, CONFIRM, MainMenu, Pending};
use crate::scoredat::Catk;
use crate::sound;

// resulttext.anm ANM slot (00 §3.2 loader thread); text.anm slot 0 (info strips, 16 §6.3).
pub const RT_SLOT: usize = 22;
const TEXT_SLOT: usize = 0;
// Anchor A = raw pos of title01 VM 141 (12 §2.5, 16 §6.4); unselected row colour.
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
// Shot names 0x4C7F4C, padded to 8 full-width cells (specs/data/spell_tables.md §5).
pub(crate) const SHOT_NAMES: [&str; 13] = [
    "霊夢＆紫　　　　",
    "魔理沙＆アリス　",
    "咲夜＆レミリア　",
    "妖夢＆幽々子　　",
    "博麗　霊夢　　　",
    "八雲　紫　　　　",
    "霧雨　魔理沙　　",
    "アリス・Ｍ　　　",
    "十六夜　咲夜　　",
    "レミリア・Ｓ　　",
    "魂魄　妖夢　　　",
    "西行寺　幽々子　",
    "全主人公合計　　",
];
// Difficulty names 0x4C8628 for info line 1 (16 §6.3; specs/data/spell_tables.md).
const DIFF_NAMES: [&str; 6] = [
    "初月（イージー）",
    "三日月（ノーマル）",
    "上つ弓張（ハード）",
    "待宵（ルナティック）",
    "望月（エキストラ）",
    "極意（ラストワード）",
];
// Easy / Normal / Hard card lists 0x4C6120 / 0x4C61C8 / 0x4C6290 (15 §2.8; specs/data/spell_tables.md §1); the
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
// Unlock hints 0x4C82C8 for ids 204–221, numbers already printed + 1 (16 §6.3; specs/data/spell_tables.md §7).
const HINTS: [[&str; 2]; 18] = [
    [
        "挑戦可能条件：望月（エキストラ）をスペルカード７枚以上取得して",
        "　　　　　　　クリアする。",
    ],
    [
        "挑戦可能条件：通常モードFinalBを２キャラ以上クリアする。",
        "　　　　　　　難易度不問。",
    ],
    [
        "挑戦可能条件：通常モードFinalBを３キャラ以上クリアする。",
        "　　　　　　　難易度不問。",
    ],
    [
        "挑戦可能条件：スペルカードモードでスペルカードを50枚以上取得する。",
        "　　　　　　　全キャラ合計。",
    ],
    [
        "挑戦可能条件：通常モードFinalBを４キャラ以上クリアする。",
        "　　　　　　　難易度不問。",
    ],
    ["挑戦可能条件：No.138を取得する。", " "],
    [
        "挑戦可能条件：スペルカードモードでラストスペルを15枚以上取得する。",
        "　　　　　　　全キャラ合計。",
    ],
    [
        "挑戦可能条件：No.146,196を取得し",
        "　　　　　　　かつ、No.205を一度でも見る。",
    ],
    [
        "挑戦可能条件：No.209,210,211を一度でも見る。",
        "　　　　　　　取得の必要はない。",
    ],
    [
        "挑戦可能条件：No.206,207,208,212を一度でも見る。",
        "　　　　　　　取得の必要はない。",
    ],
    [
        "挑戦可能条件：魔理沙（単独使用）で三日月（ノーマル）の",
        "　　　　　　　スペルカードを全て取得。",
    ],
    [
        "挑戦可能条件：霊夢（単独使用）で上つ弓張（ハード）以上をクリア。",
        "　　　　　　　最終面はFinalBのみ。",
    ],
    [
        "挑戦可能条件：スペルカードモードでスペルカードを120枚以上取得する",
        "　　　　　　　全キャラ合計。",
    ],
    [
        "挑戦可能条件：通常モードFinalBを６キャラ以上クリアする。",
        "　　　　　　　",
    ],
    [
        "挑戦可能条件：望月（エキストラ）を３キャラ以上クリアする。",
        " ",
    ],
    [
        "挑戦可能条件：スペルカードモードでラストスペルを30枚以上取得する。",
        "　　　　　　　全キャラ合計。",
    ],
    [
        "挑戦可能条件：待宵（ルナティック）をクリアする。",
        "　　　　　　　コンティニュー可、使用キャラ不問。",
    ],
    [
        "挑戦可能条件：これ以外のラストワードを全て一度見る。",
        "　　　　　　　取得の必要はない。",
    ],
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
fn rpy_stage_step(have: &[bool; 9], cur: usize, d: i32) -> usize {
    let mut c = cur as i32;
    if d == 0 || !have.contains(&true) {
        return cur;
    }
    c = (c + d).rem_euclid(9);
    while !have[c as usize] {
        c = (c + d).rem_euclid(9);
    }
    c as usize
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
        d => (d as i32, None),
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

// 8 and 9 full-width '?' (16 §6.3; the 9-char one is string 0x4B9330).
const UNKNOWN8: &str = "？？？？？？？？";
const UNKNOWN9: &str = "？？？？？？？？？";

// CATK counter rows (15 §2.5): story attempts / captures, practice bonus / attempts / captures.
const SA: usize = 1;
const SC: usize = 2;
const PB: usize = 3;
const PA: usize = 4;
const PC: usize = 5;

// State of the list screens.
#[derive(Default)]
pub struct Lists {
    // Scratch VMs from menu+0xE7B8 (screen 16: E7B8 / EA5C / ED00; screen 17: 15 row VMs) and the screen they belong to.
    scratch: Vec<AnmVm>,
    scratch_for: i32,
    // Screen-17 header VM menu+0x10F54 and the 7 info VMs menu+0x11F2C (16 §6.3, §6.5).
    header: AnmVm,
    info: Vec<AnmVm>,
    // ascii.anm VM for Ascii_DrawGaugePercent.
    glyph: AnmVm,
    // Eased pie values +0xC2A0..+0xC2AC (16 §6.4).
    pies: [f32; 4],
    // Spell-practice stage row (g_StageIdx on screens 16/17).
    pub sp_row: i32,
    // Screen 16 in its input sub-state (16 §6.1).
    sp_input: bool,
    // Screen-17 cursor (index in the row's card list), page = card / 15.
    card: i32,
    page: i32,
    prev_card: i32,
    // Info refresh counter +0xC29C (16 §6.3).
    info_counter: u32,
    // Replay screen sub-state +0x0C (1 list, 2 stage, 3 speed), stage cursor (+0 in sub-state 2), displayed / chosen
    // stage row +0xC290 (15 §3.8), speed, loaded file +0xC280.
    rpy_sub: u8,
    rpy_cur: usize,
    // Zeroed with the menu object (memset at MainMenu_Register); the replay screen entry keeps it (15 §3.8).
    rpy_stage: usize,
    rpy_speed: i32,
    rpy_loaded: Option<usize>,
    // Sub-state counter +0x10 of the list (sub-state 1): zeroed in call 8 and by a press, += 1 per call (12 §2).
    rpy_t: u32,
}

fn catk(ctx: &Ctx, id: usize) -> Catk {
    ctx.game.score.catk.get(id).cloned().unwrap_or_default()
}

// Story or practice attempts of shot index `s` ≠ 0.
fn seen(c: &Catk, s: usize) -> bool {
    c.counters[SA][s] != 0 || c.counters[PA][s] != 0
}

// Card_IsSelectable 0x46FD5F: attempted (ids < 205) or the Last Word's FLSP byte == id (0xCD + k, 15 §2.8).
fn selectable(ctx: &Ctx, id: usize) -> bool {
    if id < 205 {
        return seen(&catk(ctx, id), 12);
    }
    ctx.game
        .score
        .flsp
        .get(id - 205)
        .is_some_and(|&v| v as usize == id - 205 + 0xCD)
}

fn sjis(b: &[u8]) -> String {
    let n = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    crate::text::decode_sjis(&b[..n])
}

// Three full-width digits with leading zeros (info line 0, 16 §6.3).
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
    let col = [
        (argb >> 16) as u8,
        (argb >> 8) as u8,
        argb as u8,
        (argb >> 24) as u8,
    ];
    let p = |x: f32, y: f32| Vertex::screen(x + 0.5, y + 0.5, z, [0.0, 0.0], col);
    let mut verts = vec![p(c[0], c[1])];
    let step = 0.101342 * v;
    for k in 0..63 {
        let a = -std::f32::consts::FRAC_PI_2 + k as f32 * step;
        verts.push(p(c[0] + r * a.cos(), c[1] + r * a.sin()));
    }
    ctx.gfx.fan(Blend::Alpha, &verts);
}

// Text_RenderToVm (white, outline 0, font 15, 16 §6.3 / §6.5) into the sprite's texture strip.
pub(crate) fn render_strip(
    ctx: &mut Ctx,
    raster: &mut crate::text::Raster,
    slot: usize,
    sprite: i32,
    s: &str,
) {
    let Some(sp) = ctx.anm.sprite(slot, sprite.max(0) as usize).copied() else {
        return;
    };
    // 10 §6: x = u0 = 0, W = texture-row width (512), 15 rows from v0; rows 15–16 of a 17-high sprite stay untouched.
    let t = ctx.gfx.texture(sp.tex);
    let (w, ty) = (t.width, (sp.uv[1] * t.height as f32).round() as u32);
    let rgba = raster.render(s, [255, 255, 255], w as usize, 15);
    ctx.gfx.write_texture(sp.tex, 0, ty, w, 15, &rgba);
}

impl MainMenu {
    fn anchor(&self) -> [f32; 2] {
        let p = self.vms[VM_ANCHOR].pos;
        [p[0], p[1]]
    }

    fn render_vm(&mut self, ctx: &mut Ctx, slot: usize, sprite: i32, s: &str) {
        render_strip(ctx, &mut self.raster, slot, sprite, s);
    }

    fn glyph_vm(&mut self, ctx: &mut Ctx) {
        if self.lists.glyph.script.is_none() {
            ctx.anm
                .set_script(&mut self.lists.glyph, crate::ascii::ANM_SLOT, 4);
        }
    }

    // ---- screen 11 (12 §2.5) ----

    // Any-clear mask CLRD[shot].any[difficulty]: 0 → 1 (stage 1 only); the 6B bit adds 4A and 4B (12 §2.5).
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

    // Entry (12 §2.5): range interrupt 18, portraits entries 1–3, +0x6C = 0, cursor = g_StageIdx (0 if unavailable).
    fn practice_enter(&mut self, ctx: &mut Ctx) {
        {
            self.back = 0;
            self.interrupt_all(18);
            self.char_vms_list(ctx.game.shot as i32);
            self.cursor = (ctx.game.stage as i32).clamp(0, 7);
            let m = Self::practice_mask(ctx);
            self.stages = std::array::from_fn(|i| crate::menu::unlock_all() || m & (1 << i) != 0);
            if !self.stages[self.cursor.clamp(0, 7) as usize] {
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
        self.move_cursor(ctx, 8, UP, DOWN, |c| stages[c as usize]);
        if ctx.input.pressed(CONFIRM) {
            sound::play(sound::OK);
            let g = &mut ctx.game;
            g.difficulty = g.cfg.difficulty.min(3) as i32;
            g.stage = self.cursor as usize;
            g.flags.set(g.flags & !F_REPLAY | F_PRACTICE);
            g.fresh_start = true;
            ctx.audio.stop_bgm();
            g.request(gamemgr::SCENE_GAME);
        } else if ctx.input.pressed(CANCEL) {
            sound::play(sound::CANCEL);
            // 11 → 9 returns 2: screen 9 runs in this frame (12 §2).
            self.set_screen(crate::menu::CHAR_PRACTICE);
            self.again = true;
        }
    }

    // MainMenu_PracticeStageDraw 0x46F429 (12 §2.5): header at A − (0, 96), rows `"%s %9d0 (%3d)"` from A − (0, 80),
    // PSCR hiscore / plays; cursor row white + highlight, others 0xFFA0A0A0 or 0xFF404040 (locked).
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
            let s = format!("{} {:9}0 ({:3})", STAGES[r], p.hiscore[r][d], p.plays[r][d]);
            ctx.ascii
                .push(text(ax, ay - 80.0 + 16.0 * r as f32, s, c, cur));
        }
    }

    // ---- screen 16 (16 §6.1, §6.4) ----

    // Screen 16 offers shots whose team has a Final A clear (Clear_TeamFinalA); solo shots always pass (16 §6.1).
    pub(crate) fn sp_shot_ok(ctx: &Ctx, s: i32) -> bool {
        s >= 4 || crate::menu::team_final_a(&ctx.game, s as usize)
    }

    pub(crate) fn spell_char(&mut self, ctx: &mut Ctx) {
        let n = if crate::menu::extra_all(&ctx.game) {
            12
        } else {
            4
        };
        if self.frames == 0 {
            self.spell_char_enter(ctx, n);
        }
        // Back from a spell-practice game: 16 → 17 at once, stage row 9 if the last card was a Last Word,
        // return 2 so screen 17 runs in this frame (12 §2.2, 16 §6.1).
        if self.back != 0 {
            if ctx.game.card >= 205 {
                ctx.game.stage = 9;
            }
            self.lists.sp_row = ctx.game.stage.min(9) as i32;
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
            self.lists.sp_row = ctx.game.stage.min(9) as i32;
            self.lists.sp_input = false;
            self.set_surface(ctx, "select00.png");
            ctx.game.flags |= F_PRACTICE | F_SPELL_PRACTICE;
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

    // Portraits redone on entry and on every shot change; VMs 138/139 get pending 27 after the range interrupt (16 §6.1).
    // A shot change also zeroes +0x1442C, so the pie easing (16 §6.4 step 5) restarts.
    fn spell_char_shot(&mut self, ctx: &mut Ctx, entry: bool) {
        if self.cursor != self.prev_cursor {
            if !entry {
                self.frames = 0;
            }
            self.prev_cursor = self.cursor;
            ctx.game.shot = self.cursor as u8;
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
            // 16 → 17 returns 2: screen 17 runs in this frame (12 §2).
            self.set_screen(crate::menu::SPELL_CARD);
            self.again = true;
        } else if inp.pressed(CANCEL) {
            sound::play(sound::CANCEL);
            // Bomb also stores the row into g_StageIdx (16 §6.1).
            ctx.game.stage = self.lists.sp_row as usize;
            self.interrupt_all(6);
            ctx.game.flags &= !F_PRACTICE;
            self.pending = Some(Pending {
                frames: 20,
                screen: crate::menu::TOP,
                cursor: 2,
            });
        }
    }

    // MainMenu_SpellPracticeCharDraw 0x46F5CB (16 §6.4 steps 1–6).
    pub(crate) fn spell_char_draw(&mut self, ctx: &mut Ctx) {
        // The draw follows the screen variable, so the press frame already draws this screen (VMs not created yet).
        let own =
            self.lists.scratch_for == crate::menu::SPELL_PRACTICE && self.lists.scratch.len() >= 3;
        let [ax, ay] = self.anchor();
        // The heading x (A.x − 32) is also the row pen x (16 §6.4 step 1).
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
        // Pie values v1..v4 (16 §6.4 step 5), eased over the first 50 screen frames.
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

    // The 15 row VMs (resulttext sprites 2..16) of the current page with their names (16 §6.5).
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

    // Info panel line k (16 §6.3, 0x46D7F9).
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
            1 => format!(
                "使用者  {}　　{} {}",
                if att { sjis(&c.owner) } else { UNKNOWN8.into() },
                DIFF_NAMES[difficulty_of(id)],
                if is_last(id) { "Last" } else { " " }
            ),
            2 => format!("取得数/挑戦数[最高点]　{}　全主人公合計", SHOT_NAMES[s]),
            3 => {
                if !att {
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
                }
            }
            4 => "カードの補足".into(),
            _ => {
                if id >= 204 && !att && !selectable(ctx, id) {
                    HINTS[id - 204][k - 5].into()
                } else if c.counters[PC][12] != 0 {
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

    // Screen 17, MainMenu_SpellPracticeCard 0x46BBC0 (16 §6.2); input from +0x1442C == 8 (12 §2).
    pub(crate) fn spell_card(&mut self, ctx: &mut Ctx) {
        let (a, b) = self.card_ids();
        let count = (b - a) as i32;
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
        let count = (b - a) as i32;
        {
            self.back = 0;
            for vm in &mut self.vms {
                vm.pending_interrupt = 26;
            }
            compute_last_word_unlocks(&mut ctx.game);
            let id = ctx.game.card as i32 - a as i32;
            self.lists.card = if (0..count).contains(&id) { id } else { 0 };
            self.lists.page = self.lists.card / 15;
            self.lists.prev_card = self.lists.card;
            self.card_rows(ctx);
            self.lists.header = scratch(ctx, RT_SLOT, 17, false);
            self.render_vm(ctx, RT_SLOT, 17, "取得数/挑戦数（()内は本編での集計）");
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
        // Page keys only when count > 15 (16 §6.2); Left also flushes the sound queue (Sound_Update, 13 §1.3).
        } else if count > 15 && inp.pressed_or_repeat(LEFT) {
            c -= 15;
            if c < 0 {
                c = count - 1;
            }
            sound::play(sound::SELECT);
            ctx.sound_update();
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
        let id = a + self.lists.card as usize;
        if inp.pressed(CONFIRM) {
            // Locked card: SE 0x29 (16 §6.2).
            let ok = seen(&catk(ctx, id), 12) || selectable(ctx, id);
            if !ok {
                sound::play(sound::INVALID);
                return;
            }
            sound::play(sound::OK);
            let g = &mut ctx.game;
            g.flags |= F_SPELL_PRACTICE;
            g.flags &= !F_REPLAY;
            g.card = id as i16;
            let (d, st) = spell_start(id);
            g.difficulty = d;
            // Other cards keep g_StageIdx = the row set by the screen-16 confirm (16 §6.1).
            g.stage = st.unwrap_or(self.lists.sp_row as usize);
            g.fresh_start = true;
            ctx.audio.stop_bgm();
            g.request(gamemgr::SCENE_GAME);
        } else if inp.pressed(CANCEL) {
            sound::play(sound::CANCEL);
            ctx.game.card = id as i16;
            // 17 → 16 returns 2: screen 16 runs in this frame (12 §2).
            self.set_screen(crate::menu::SPELL_PRACTICE);
            self.again = true;
        }
    }

    // MainMenu_SpellPracticeCardDraw 0x46FFC0 (16 §6.5): header VM at (334, 78), rows from y 94, then the info VMs
    // at x 64 (lines 5/6 at x 96), y 344 + 16k (+8 from line 4).
    pub(crate) fn spell_card_draw(&mut self, ctx: &mut Ctx) {
        let own =
            self.lists.scratch_for == crate::menu::SPELL_CARD && self.lists.scratch.len() >= 15;
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
            ctx.ascii.push(text(
                16.0,
                y,
                format!("{marker}No.{:03}", id + 1),
                col,
                false,
            ));
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

    // Listing (15 §3.8): slots th8_01..15, then up to 45 th8_ud????.rpy files; unreadable files are skipped.
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

    // The list-row VM 80 + (row mod 15) (12 §2.8).
    fn rpy_row_vm(&self) -> usize {
        80 + (self.cursor.max(0) % 15) as usize
    }

    // Speed plates VMs 108–110: 21 = not selected, 20 = selected (12 §2.8).
    fn rpy_speed_vms(&mut self) {
        for k in 0..3 {
            self.vms[108 + k].pending_interrupt = 21;
        }
        self.vms[108 + self.lists.rpy_speed as usize].pending_interrupt = 20;
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
            self.load_replays();
            self.lists.rpy_sub = 1;
            self.lists.rpy_loaded = None;
            if self.prev_screen == crate::menu::TOP {
                self.cursor = 0;
            }
            self.cursor = self.cursor.clamp(0, (self.replays.len() as i32 - 1).max(0));
        }
        // Sub-state 0 in calls 0–7; sub-state 1 starts in call 8 with +0x10 := 0, so the list moves from call 9 and
        // takes confirm / cancel from call 18 (+0x10 > 9) (12 §2 intro, 15 §3.8).
        if self.frames == 8 {
            self.lists.rpy_t = 0;
        }
        if self.frames < 9 || self.pending.is_some() {
            return true;
        }
        let n = self.replays.len() as i32;
        let inp = ctx.input;
        match self.lists.rpy_sub {
            1 => {
                if n > 0 {
                    self.move_cursor(ctx, n, UP, DOWN, |_| true);
                }
                // Left / Right page by 15 only with more than 15 files, wrapping by the count; SE 12 (15 §3.8).
                if n > 15 && (inp.pressed_or_repeat(LEFT) || inp.pressed_or_repeat(RIGHT)) {
                    self.cursor = rpy_page(self.cursor, n, inp.pressed_or_repeat(LEFT));
                    sound::play(sound::SELECT);
                }
                // The +0x10 > 9 gate is armed once per visit: only the list cancel zeroes +0x10 again; the file
                // confirm and the cancels 2 → 1 / 3 → 2 leave it counting (12 §2 intro, 15 §3.8).
                if self.lists.rpy_t <= 9 {
                    return true;
                }
                if inp.pressed(CONFIRM) && n > 0 {
                    sound::play(sound::OK);
                    let i = self.cursor as usize;
                    // Stage cursor = first stage with a record; +0xC290 keeps its old value until sub-state 2. None:
                    // error log line, scene −1, return 0 (15 §3.8, 00 §2).
                    let st = &self.replays[i].1.stages;
                    let Some(first) = (0..9).find(|&s| st[s].is_some()) else {
                        log::error!("リプレイデータが異常");
                        ctx.game.request(-1);
                        return false;
                    };
                    self.lists.rpy_loaded = Some(i);
                    // Card-name scratch VM menu+0xE7B8 (12 §2.8 step 3): header +0x7E into resulttext sprite 11, set up
                    // for every file, drawn only for spell replays.
                    let name = sjis(self.replays[i].1.card_name());
                    self.notice = scratch(ctx, RT_SLOT, 11, false);
                    self.render_vm(ctx, RT_SLOT, 11, &name);
                    self.lists.rpy_cur = first;
                    self.lists.rpy_sub = 2;
                    self.rpy_choose();
                } else if inp.pressed(CANCEL) {
                    sound::play(sound::CANCEL);
                    self.lists.rpy_t = 0;
                    self.interrupt_all(16);
                    // Replay leave: top menu (cursor 4) after 30 frames (12 §2).
                    self.pending = Some(Pending {
                        frames: 30,
                        screen: crate::menu::TOP,
                        cursor: 4,
                    });
                }
            }
            2 => {
                let Some(i) = self.lists.rpy_loaded else {
                    return true;
                };
                let have: [bool; 9] =
                    std::array::from_fn(|s| self.replays[i].1.stages[s].is_some());
                let d = if inp.pressed_or_repeat(UP) {
                    -1
                } else if inp.pressed_or_repeat(DOWN) {
                    1
                } else {
                    0
                };
                if d != 0 {
                    sound::play(sound::SELECT);
                }
                // +0xC290 is written only here, so the row shown on the file-confirm frame lags by one (15 §3.8).
                self.lists.rpy_cur = rpy_stage_step(&have, self.lists.rpy_cur, d);
                self.lists.rpy_stage = self.lists.rpy_cur;
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
                    // SPEC-GAP: the cancels 2 → 1 / 3 → 2 zero +0x1442C; whether its entry block then reruns is not given.
                    self.lists.rpy_sub = 1;
                    self.lists.rpy_loaded = None;
                    self.interrupt_all(14);
                }
            }
            _ => {
                let Some(i) = self.lists.rpy_loaded else {
                    return true;
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
                // Spell replays: 2 speeds, VM 110's alpha byte 0 (12 §2.8).
                if spell {
                    set_alpha(&mut self.vms[110], 0);
                }
                if inp.pressed(CONFIRM) {
                    sound::play(sound::OK);
                    ctx.game.replay_speed = self.lists.rpy_speed as u8;
                    // The loaded replay stays listed: this frame's draw still shows its detail rows.
                    let r = self.replays[i].1.clone();
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
        true
    }

    // MainMenu_ReplayDraw 0x46EF8D (12 §2.8 draw steps 1–3); text anchors are the VMs' raw positions.
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
                let (x, y) = at(self, 97);
                ctx.ascii.push(text(
                    x,
                    y,
                    format!("No.{:03} {:9}0", r.spell_id() + 1, r.score()),
                    0xFFFFFFFF,
                    false,
                ));
                // The card name under the row: scratch VM at VM 97's pos + (0, 16), top-left, white (12 §2.8).
                let mut vm = std::mem::take(&mut self.notice);
                draw_at(ctx, &mut vm, x, y + 16.0);
                self.notice = vm;
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

#[cfg(test)]
mod tests {
    use super::*;

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

    // 15 §3.8: Menu_CursorUpDown(9) wraps, then skips stages without a record in the same direction.
    #[test]
    fn replay_stage_cursor() {
        let mut have = [false; 9];
        have[1] = true;
        have[4] = true;
        have[8] = true;
        assert_eq!(rpy_stage_step(&have, 1, 0), 1);
        assert_eq!(rpy_stage_step(&have, 1, 1), 4);
        assert_eq!(rpy_stage_step(&have, 4, 1), 8);
        assert_eq!(rpy_stage_step(&have, 8, 1), 1);
        assert_eq!(rpy_stage_step(&have, 1, -1), 8);
        assert_eq!(rpy_stage_step(&[false; 9], 0, 1), 0);
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
