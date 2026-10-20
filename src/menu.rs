//! Main menu (12 §2, `MainMenu_Register` 0x47146D): top menu, difficulty / character select, option screen; the
//! practice, replay and spell-practice screens are in menulist.rs.

use crate::anmvm::{self, AnmVm};
use crate::ascii::Text;
use crate::config::Config;
use crate::game::Ctx;
use crate::gamemgr::{self, F_PRACTICE, F_REPLAY, F_SPELL_PRACTICE};
use crate::gfx::{FULLSCREEN, TextureId};
use crate::input::{BOMB, DOWN, ENTER, LEFT, PAUSE, RIGHT, SHOT, UP};
use crate::sound;
use crate::task::{Chain, Ret, Task};
use anyhow::Result;

// title01.anm in ANM slot 20; calc prio 4 (0x467399), draw prio 3 (0x47087F) (12 §2, 00 §4).
pub const ANM_SLOT: usize = 20;
pub const CALC_PRIO: u16 = 4;
pub const DRAW_PRIO: u16 = 3;
// Surface 0 reaches the device before the batched prio-2 nowloading quads (12 §2.2).
pub const SURFACE_PRIO: u16 = 1;
// +0x14424 VM count 0x8E; Confirm = 0x1001, Cancel = 0x0A (12 intro).
const VM_COUNT: usize = 142;
pub(crate) const CONFIRM: u16 = SHOT | ENTER;
pub(crate) const CANCEL: u16 = BOMB | PAUSE;

// Screen ids (menu +0x14428, 12 §2 dispatch table).
pub(crate) const TOP: i32 = 0;
const OPTION: i32 = 2;
const KEY_CONFIG: i32 = 3;
const DIFF: i32 = 4;
const DIFF_PRACTICE: i32 = 8;
const DIFF_EXTRA: i32 = 12;
const CHAR: i32 = 5;
pub(crate) const CHAR_PRACTICE: i32 = 9;
const CHAR_EXTRA: i32 = 13;
const REPLAY: i32 = 7;
const PRACTICE_STAGE: i32 = 11;
pub(crate) const SPELL_PRACTICE: i32 = 16;
pub(crate) const SPELL_CARD: i32 = 17;

// Top-menu items = VMs 1–9; difficulty VMs 131–134 and 135 (Extra); option labels from VM 10 (12 §2.1, §2.7).
const ITEMS: usize = 9;
const ITEM_QUIT: i32 = 8;
const VM_DIFF: usize = 131;
const VM_DIFF_EXTRA: usize = 135;
const VM_OPTION: usize = 10;
const OPTION_ITEMS: i32 = 10;

// Top-menu item descriptions (12 §2.2 table).
const DESCRIPTIONS: [&str; ITEMS] = [
    "ゲームを開始します",
    "エキストラステージを開始します",
    "敵にスペルカード戦をお願いします",
    "ステージを選択し、練習を開始します",
    "リプレイを鑑賞できます",
    "過去のスコアやスペルカードの取得歴を見られます",
    "音楽を聴けます",
    "各種設定できます",
    "いろいろと終了します",
];

// Character select VMs per shot (table 0x4B8CD8): shown VMs, then the VM that gets interrupt 23.
pub(crate) const CHAR_VMS: [[i32; 4]; 12] = [
    [119, 111, 112, -1],
    [120, 114, 113, -1],
    [121, 115, 116, -1],
    [122, 118, 117, -1],
    [123, 111, -1, 112],
    [124, 112, -1, 111],
    [125, 114, -1, 113],
    [126, 113, -1, 114],
    [127, 115, -1, 116],
    [128, 116, -1, 115],
    [129, 118, -1, 117],
    [130, 117, -1, 118],
];

// Final-route notice sprite for the cursor team (MainMenu_CharacterDraw 0x47052D, 12 §2.4 table, first match);
// 0x80 = 6B (stage index 7) cleared, 0x40 = 6A.
fn final_notice(g: &gamemgr::Game, c: usize, d: usize) -> Option<i32> {
    let (nc, any) = (&g.clear_nc[c % 12], &g.clear_any[c % 12]);
    let d = d.min(4);
    if nc[d] & 0x80 != 0 {
        return Some(if any[d] & 0x40 != 0 { 146 } else { 148 });
    }
    if c < 4 && !nc[..4].iter().any(|m| m & 0x80 != 0) {
        return any[..4].iter().any(|m| m & 0x40 != 0).then_some(145);
    }
    Some(147)
}

// Unlock checks (15 §2.4) on the score.dat clear masks; debug `TH08_UNLOCK=1` unlocks everything.
pub(crate) fn unlock_all() -> bool {
    crate::settings::get().unlock
}

// 0x43C15F: a no-continue Final B clear on E–L.
fn team_final_b(g: &gamemgr::Game, t: usize) -> bool {
    unlock_all() || t >= 4 || g.clear_nc[t].iter().take(4).any(|m| m & 0x4000 != 0)
}

// 0x43C23B: a Final A clear on E–L.
pub(crate) fn team_final_a(g: &gamemgr::Game, t: usize) -> bool {
    unlock_all() || t >= 4 || g.clear_any[t].iter().take(4).any(|m| m & 0x8000 != 0)
}

// Extra character select (screen 13) offers only shots whose team passes 0x43C15F (12 §2.4).
fn extra_unlocked(g: &gamemgr::Game, shot: usize) -> bool {
    team_final_b(g, shot % 12)
}

// 0x43C1E9: Extra unlocked (GM flags bit 15).
fn extra_any(g: &gamemgr::Game) -> bool {
    (0..4).any(|t| team_final_b(g, t))
}

// 0x447D04: all teams → the 8 solo shots are selectable (flags bit 17).
pub(crate) fn extra_all(g: &gamemgr::Game) -> bool {
    (0..4).all(|t| team_final_b(g, t))
}

// 0x43C2C5: some team has a Final A clear (flags bit 16), gating top-menu item 2.
fn spell_practice_unlocked(g: &gamemgr::Game) -> bool {
    (0..4).any(|t| team_final_a(g, t))
}

// PLST total plays (block 6 [0], 0x164CED0); option lives max 4 / 5 / 6 below 30 / 60 plays (15 §1.3).
fn play_count(g: &gamemgr::Game) -> u32 {
    if unlock_all() {
        60
    } else {
        g.score.plst.total_plays()
    }
}

fn lives_max(g: &gamemgr::Game) -> i32 {
    match play_count(g) {
        ..30 => 4,
        30..60 => 5,
        _ => 6,
    }
}

// A screen switch that waits `frames` calcs (option 30, difficulty cancel 20, 12 §2.2–§2.3).
pub(crate) struct Pending {
    pub(crate) frames: u32,
    pub(crate) screen: i32,
    pub(crate) cursor: i32,
}

// The 0x14490-byte menu object, global 0x18BDE08 (12 §2 field table).
pub struct MainMenu {
    // +0xC2B8: VM i runs title01.anm script i; `base` = each script's own (selected) sprite.
    pub(crate) vms: Vec<AnmVm>,
    pub(crate) base: Vec<i32>,
    // Surface slot 0 (title00.png / select00.png), copied before everything (12 §2.2).
    pub(crate) surface: Option<(TextureId, &'static str)>,
    pub(crate) raster: crate::text::Raster,
    // Current description VM +0xC2BC (text.anm script 9, sprite 21 + item).
    // Port: one VM re-rendered per item instead of the original's 14 pre-rendered strips.
    pub(crate) desc_vm: AnmVm,
    pub(crate) desc_on: bool,
    pub(crate) desc_first: bool,
    // Scratch VM menu+0xE7B8 (final-route notice, replay card name).
    pub(crate) notice: AnmVm,
    // +0x14428 screen, +0x68 previous screen, +0x1442C frames on this screen.
    pub(crate) screen: i32,
    pub(crate) prev_screen: i32,
    pub(crate) frames: u32,
    // +0x00 cursor, +0x08 previous cursor (−1 forces the change test).
    pub(crate) cursor: i32,
    pub(crate) prev_cursor: i32,
    // +0x14430 idle frames (demo timer).
    pub(crate) idle: u32,
    // Quit (item 8) frame counter; scene −1 after 60 frames.
    pub(crate) quitting: Option<u32>,
    pub(crate) pending: Option<Pending>,
    // Config snapshot taken on entering the option screen (12 §2.2 item 7).
    pub(crate) snapshot: Config,
    // Practice stages available on screen 11 (12 §2.5).
    pub(crate) stages: [bool; 8],
    pub(crate) replays: Vec<(String, crate::replay::Replay)>,
    pub(crate) lists: crate::menulist::Lists,
}

impl MainMenu {
    pub fn new() -> Self {
        Self {
            vms: Vec::new(),
            base: Vec::new(),
            surface: None,
            raster: Default::default(),
            desc_vm: AnmVm::default(),
            desc_on: false,
            desc_first: true,
            notice: AnmVm::default(),
            screen: TOP,
            prev_screen: TOP,
            frames: 0,
            cursor: 0,
            prev_cursor: -1,
            idle: 0,
            quitting: None,
            pending: None,
            snapshot: Config::default(),
            stages: [false; 8],
            replays: Vec::new(),
            lists: Default::default(),
        }
    }

    // Menu_SetScreen 0x4694C3 (12 §2 entry rule): the new handler runs from the next calc with frames = 0.
    pub(crate) fn set_screen(&mut self, s: i32) {
        self.prev_screen = self.screen;
        self.screen = s;
        self.frames = 0;
        self.prev_cursor = -1;
    }

    // AnmManager_InterruptRange over the 142 VMs; a later per-VM write replaces it (one pending slot, 12 §2.1).
    pub(crate) fn interrupt_all(&mut self, id: i32) {
        for vm in &mut self.vms {
            vm.pending_interrupt = id as _;
        }
    }

    pub(crate) fn set_surface(&mut self, ctx: &mut Ctx, name: &'static str) {
        if self.surface.is_some_and(|(_, n)| n == name) {
            return;
        }
        if let Some((t, _)) = self.surface.take() {
            ctx.gfx.free_texture(t);
        }
        match ctx
            .archive
            .read(name)
            .and_then(|b| ctx.gfx.load_image(name, &b))
        {
            Ok(t) => self.surface = Some((t, name)),
            Err(e) => log::warn!("{name}: {e:#}"),
        }
    }

    // Selected = the script's own sprite, unselected = sprite + 1 (12 §2.1).
    fn highlight(&mut self, ctx: &Ctx, vm: usize, on: bool) {
        let b = self.base[vm];
        if b >= 0 {
            self.vms[vm].set_sprite(&ctx.anm, b + if on { 0 } else { 1 });
        }
    }

    // Text_RenderCentered 0x466650 (12 §2.2): description `i` into the VM's text.anm sprite rect, centred, colour
    // 0xFFF0E0.
    fn render_description(&mut self, ctx: &mut Ctx, i: usize) {
        let vm = &self.desc_vm;
        let Some(sp) = ctx.anm.sprite(0, vm.sprite_index.max(0) as usize).copied() else {
            return;
        };
        let t = ctx.gfx.texture(sp.tex);
        let (tx, ty) = (
            (sp.uv[0] * t.width as f32).round() as u32,
            (sp.uv[1] * t.height as f32).round() as u32,
        );
        let (w, h) = (sp.w as usize, sp.h as usize);
        let mut rgba = self
            .raster
            .render(DESCRIPTIONS[i], [0xFF, 0xF0, 0xE0], w, h);
        let width = (0..w)
            .rev()
            .find(|&x| (0..h).any(|y| rgba[(y * w + x) * 4 + 3] != 0))
            .map_or(0, |x| x + 1);
        let shift = (w - width) / 2;
        if shift > 0 {
            for row in rgba.chunks_exact_mut(w * 4) {
                row.copy_within(0..width * 4, shift * 4);
                row[..shift * 4].fill(0);
            }
        }
        ctx.gfx
            .write_texture(sp.tex, tx, ty, w as u32, h as u32, &rgba);
    }

    // Menu_CursorUpDown 0x47030B / Menu_CursorLeftRight 0x470424: wraps, skips locked entries, held keys use the
    // repeat flag, sfx 12 (12 §2.2, §2.4).
    pub(crate) fn move_cursor(
        &mut self,
        ctx: &Ctx,
        n: i32,
        prev: u16,
        next: u16,
        allowed: impl Fn(i32) -> bool,
    ) {
        let i = &ctx.input;
        let d = if i.pressed_or_repeat(prev) {
            -1
        } else if i.pressed_or_repeat(next) {
            1
        } else {
            return;
        };
        let mut c = self.cursor;
        for _ in 0..n {
            c = (c + d).rem_euclid(n);
            if allowed(c) {
                break;
            }
        }
        if c != self.cursor {
            self.cursor = c;
            sound::play(sound::SELECT);
        }
    }

    // Items 1 (Extra) and 2 (Spell Practice) are skipped and drawn 0xFF404040 while locked (12 §2.2).
    fn item_locked(ctx: &Ctx, i: usize) -> bool {
        match i {
            1 => !extra_any(&ctx.game),
            2 => !spell_practice_unlocked(&ctx.game),
            _ => false,
        }
    }

    // Screen 0, 0x4674E0 (12 §2.2).
    fn top(&mut self, ctx: &mut Ctx) {
        if self.frames == 0 {
            self.set_surface(ctx, "title00.png");
            self.interrupt_all(2);
        }
        // The cursor moves before the highlight, so a press shows in its own frame.
        if self.frames >= 10 && self.pending.is_none() && self.quitting.is_none() {
            let locked: Vec<bool> = (0..ITEMS).map(|i| Self::item_locked(ctx, i)).collect();
            self.move_cursor(ctx, ITEMS as i32, UP, DOWN, |c| !locked[c as usize]);
        }
        for i in 0..ITEMS {
            let on = i as i32 == self.cursor;
            self.highlight(ctx, 1 + i, on);
            if Self::item_locked(ctx, i) {
                self.vms[1 + i].color = [0x40, 0x40, 0x40, 0xFF];
            } else {
                self.vms[1 + i].color[..3].fill(0xFF);
            }
        }
        // One description VM per item (sprite 21 + i, 12 §2.2). The loader leaves VM 0 running; on the 9th frame the
        // handler sets +0x08 = −1 and enters sub-state 1, so the change test switches to the cursor's VM (12 §2.2).
        if self.frames == 0 {
            self.desc_on = self.desc_first;
            if self.desc_first {
                ctx.anm.set_script(&mut self.desc_vm, 0, 9);
                self.desc_vm.set_sprite(&ctx.anm, 21);
                self.render_description(ctx, 0);
            }
        }
        if self.frames >= 9 && self.cursor != self.prev_cursor {
            self.prev_cursor = self.cursor;
            self.desc_first = false;
            self.desc_on = true;
            ctx.anm.set_script(&mut self.desc_vm, 0, 9);
            self.desc_vm.set_sprite(&ctx.anm, 21 + self.cursor);
            self.desc_vm.pending_interrupt = 1;
            self.render_description(ctx, self.cursor as usize);
        }

        if let Some(f) = &mut self.quitting {
            // Item 8: after 60 frames free the VMs, scene −1 (12 §2.2).
            *f += 1;
            if *f >= 60 {
                ctx.game.request(-1);
            }
            return;
        }
        if self.frames < 10 || self.pending.is_some() {
            return;
        }
        self.idle = if ctx.input.held != 0 {
            0
        } else {
            self.idle + 1
        };
        // Demo (12 §2.2): idle counter reset by any raw input; after 1500 frames load demorpy((n+1) mod 4).
        if self.idle >= 1500 {
            self.idle = 0;
            let n = (ctx.game.demo + 1) % 4;
            let r = ctx
                .archive
                .read(&format!("demorpy{n}.rpy"))
                .and_then(|b| crate::replay::Replay::parse(&b));
            match r {
                Ok(r) => {
                    if crate::replay::start_playback(&mut ctx.game, r, None, Some(n)) {
                        ctx.audio.stop_bgm();
                        return;
                    }
                }
                Err(e) => log::warn!("demorpy{n}.rpy: {e:#}"),
            }
        }
        // Confirm actions of the 12 §2.2 item table. SPEC-GAP: the sound flush after sfx 10 is not done.
        if ctx.input.pressed(CONFIRM) {
            sound::play(sound::OK);
            let g = &mut ctx.game;
            match self.cursor {
                0 => {
                    g.flags &= !(F_PRACTICE | F_SPELL_PRACTICE);
                    self.interrupt_all(5);
                    self.set_screen(DIFF);
                    self.cursor = last_difficulty(g);
                }
                1 => {
                    g.flags &= !(F_PRACTICE | F_SPELL_PRACTICE);
                    self.interrupt_all(5);
                    self.set_screen(DIFF_EXTRA);
                }
                2 => {
                    g.flags |= F_SPELL_PRACTICE | F_PRACTICE;
                    self.interrupt_all(5);
                    self.set_screen(SPELL_PRACTICE);
                }
                3 => {
                    g.flags = g.flags & !F_SPELL_PRACTICE | F_PRACTICE;
                    self.interrupt_all(5);
                    self.set_screen(DIFF_PRACTICE);
                    self.cursor = last_difficulty(g);
                }
                4 => {
                    self.interrupt_all(13);
                    self.set_screen(REPLAY);
                }
                5 => g.request(gamemgr::SCENE_RESULT_MENU),
                6 => g.request(gamemgr::SCENE_MUSIC_ROOM),
                7 => {
                    self.interrupt_all(5);
                    self.pending = Some(Pending {
                        frames: 30,
                        screen: OPTION,
                        cursor: 0,
                    });
                }
                _ => {
                    self.interrupt_all(1);
                    // SPEC-GAP: 12 §2.2 says "fade music 30" without a unit; 30 frames assumed.
                    ctx.audio.fade_bgm(0.5);
                    self.quitting = Some(0);
                }
            }
        } else if ctx.input.pressed(CANCEL) && self.cursor != ITEM_QUIT {
            self.cursor = ITEM_QUIT;
            sound::play(sound::CANCEL);
        }
    }

    // Screens 4 / 8 / 12, 0x46A071 (12 §2.3): interactive from frame 8, cursor = config +0x21 (> 3 → 1), Extra fixed.
    fn difficulty(&mut self, ctx: &mut Ctx) {
        let extra = self.screen == DIFF_EXTRA;
        if self.frames == 0 {
            if !matches!(self.prev_screen, CHAR | CHAR_PRACTICE | CHAR_EXTRA) {
                self.set_surface(ctx, "select00.png");
            }
            self.interrupt_all(if extra { 12 } else { 7 });
            self.cursor = if extra { 0 } else { last_difficulty(&ctx.game) };
        }
        let active = self.frames >= 8 && self.pending.is_none();
        if !extra {
            if active {
                self.move_cursor(ctx, 4, UP, DOWN, |_| true);
            }
            if self.cursor != self.prev_cursor {
                self.prev_cursor = self.cursor;
                for i in 0..4 {
                    self.vms[VM_DIFF + i].pending_interrupt =
                        if i as i32 == self.cursor { 24 } else { 25 };
                }
            }
        }
        if extra {
            self.highlight(ctx, VM_DIFF_EXTRA, true);
        } else {
            for i in 0..4 {
                self.highlight(ctx, VM_DIFF + i, i as i32 == self.cursor);
            }
        }
        if !active {
            return;
        }
        let store =
            |g: &mut gamemgr::Game, c: i32| g.cfg.difficulty = if extra { 4 + c } else { c } as u8;
        if ctx.input.pressed(CONFIRM) {
            store(&mut ctx.game, self.cursor);
            sound::play(sound::OK);
            self.set_screen(self.screen + 1);
            self.cursor = 0;
        } else if ctx.input.pressed(CANCEL) {
            store(&mut ctx.game, self.cursor);
            sound::play(sound::CANCEL);
            self.interrupt_all(6);
            let cursor = match self.screen {
                DIFF_PRACTICE => 3,
                DIFF_EXTRA => 1,
                _ => 0,
            };
            ctx.game.flags &= !F_PRACTICE;
            self.pending = Some(Pending {
                frames: 20,
                screen: TOP,
                cursor,
            });
        }
    }

    // Portrait VMs of character `c` (12 §2.4): a cursor move first enables all of them with 8, so the old ones fade out.
    pub(crate) fn char_vms(&mut self, c: i32, moved: bool) {
        if moved {
            for vm in &mut self.vms[111..=130] {
                vm.flags |= anmvm::F_ENABLED;
                vm.pending_interrupt = 8;
            }
        }
        self.char_row(c, 0);
    }

    // Entries `from`..3 of the team row: enabled with 9, the partner entry 3 with 23.
    pub(crate) fn char_row(&mut self, c: i32, from: usize) {
        let row = CHAR_VMS[c.clamp(0, 11) as usize];
        for (k, &v) in row.iter().enumerate().skip(from) {
            if v >= 0 {
                let vm = &mut self.vms[v as usize];
                vm.flags |= anmvm::F_ENABLED;
                vm.pending_interrupt = if k == 3 { 23 } else { 9 };
            }
        }
    }

    // Screens 11 / 16: portraits disabled with 8, then the row without the description (entry 0).
    pub(crate) fn char_vms_list(&mut self, c: i32) {
        for vm in &mut self.vms[111..=130] {
            vm.flags &= !anmvm::F_ENABLED;
            vm.pending_interrupt = 8;
        }
        self.char_row(c, 1);
    }

    fn char_allowed(&self, ctx: &Ctx, c: i32) -> bool {
        self.screen != CHAR_EXTRA || extra_unlocked(&ctx.game, c as usize)
    }

    fn char_cursor(&mut self, ctx: &Ctx) {
        let n = if extra_all(&ctx.game) { 12 } else { 4 };
        self.cursor = (ctx.game.shot as i32).min(n - 1);
        if !self.char_allowed(ctx, self.cursor) {
            self.cursor = (0..n).find(|&c| self.char_allowed(ctx, c)).unwrap_or(0);
        }
    }

    // Screens 5 / 9 / 13, 0x46A625 (12 §2.4); screen 15 is handled with spell practice (menulist.rs).
    // SPEC-GAP: 12 §2.4 gives no input start frame; 8 is used as on the difficulty screen.
    fn character(&mut self, ctx: &mut Ctx) {
        let n = if extra_all(&ctx.game) { 12 } else { 4 };
        if self.frames == 0 {
            self.interrupt_all(8);
            // Difficulty plate slides to (16, 384) (12 §2.4).
            self.vms[VM_DIFF + (ctx.game.cfg.difficulty as usize).min(4)].pending_interrupt = 9;
            self.char_cursor(ctx);
        }
        let allowed: Vec<bool> = (0..n).map(|c| self.char_allowed(ctx, c)).collect();
        if self.frames >= 8 {
            self.move_cursor(ctx, n, LEFT, RIGHT, |c| allowed[c as usize]);
        }
        if self.cursor != self.prev_cursor {
            let moved = self.frames != 0;
            self.prev_cursor = self.cursor;
            self.char_vms(self.cursor, moved);
        }
        if self.frames < 8 {
            return;
        }
        // Confirm (12 §2.4): shot = cursor; practice → screen 11; else difficulty = config +0x21 (Extra 4, stage 8),
        // replay flag cleared, music stopped, scene 2.
        if ctx.input.pressed(CONFIRM) {
            sound::play(sound::OK);
            let g = &mut ctx.game;
            g.shot = self.cursor as u8;
            match self.screen {
                CHAR_PRACTICE => {
                    self.set_screen(PRACTICE_STAGE);
                    self.practice_enter(ctx);
                    self.frames = 1;
                }
                _ => {
                    let extra = self.screen == CHAR_EXTRA;
                    g.difficulty = if extra {
                        4
                    } else {
                        g.cfg.difficulty.min(3) as i32
                    };
                    g.stage = if extra { 8 } else { 0 };
                    g.flags &= !F_REPLAY;
                    g.fresh_start = true;
                    ctx.audio.stop_bgm();
                    g.request(gamemgr::SCENE_GAME);
                }
            }
        } else if ctx.input.pressed(CANCEL) {
            // No interrupt here (12 §2.4): the difficulty screen's enter interrupt 7 does the transition.
            sound::play(sound::CANCEL);
            self.set_screen(self.screen - 1);
        }
    }

    // Live config byte behind option item `item` (12 §2.7, 15 §1.3).
    fn option_value(cfg: &mut Config, item: i32) -> Option<&mut u8> {
        Some(match item {
            0 => &mut cfg.lives,
            1 => &mut cfg.colour_mode,
            2 => &mut cfg.music,
            3 => &mut cfg.bgm_volume,
            4 => &mut cfg.se_volume,
            5 => &mut cfg.windowed,
            6 => &mut cfg.slow,
            _ => return None,
        })
    }

    // Option screen 2, MainMenu_Option 0x46837B (12 §2.7, 15 §1.3).
    // SPEC-GAP: the return value 5 (device recreate on colour/window change) and the 3600-frame idle exit are not done.
    fn option(&mut self, ctx: &mut Ctx) {
        if self.frames == 0 {
            self.interrupt_all(3);
            self.cursor = 0;
            self.snapshot = ctx.game.cfg;
        }
        for i in 0..OPTION_ITEMS {
            self.highlight(ctx, VM_OPTION + i as usize, i == self.cursor);
        }
        self.option_values(ctx);
        if self.frames < 8 || self.pending.is_some() {
            return;
        }
        self.move_cursor(ctx, OPTION_ITEMS, UP, DOWN, |_| true);
        let inp = ctx.input;
        let dir = if inp.pressed_or_repeat(LEFT) {
            -1
        } else if inp.pressed_or_repeat(RIGHT) {
            1
        } else {
            0
        };
        let item = self.cursor;
        let lmax = lives_max(&ctx.game);
        let cfg = &mut ctx.game.cfg;
        if dir != 0
            && let Some(v) = Self::option_value(cfg, item)
        {
            let old = *v;
            // SPEC-GAP: 15 §1.3 steps a volume ±4 then ±1 on an edge (±5 per press) and ±1 per frame once the
            // unchanged-input counter is > 29; this steps ±1 per press and ±4 on repeat frames. The volume preview
            // (SE 0x1D every 50 frames on items 3/4) is not played.
            let (max, step) = match item {
                0 => (lmax, 1),
                2 => (2, 1),
                3 | 4 => (100, if inp.repeat { 4 } else { 1 }),
                _ => (1, 1),
            };
            *v = if matches!(item, 3 | 4) {
                (*v as i32 + dir * step).clamp(0, max) as u8
            } else {
                (*v as i32 + dir).rem_euclid(max + 1) as u8
            };
            if *v != old {
                sound::play(sound::TIMEOUT);
                // Item 2: any change restarts bgm/th08_01.mid in slot 8 (15 §1.3); music off simply stops it.
                match item {
                    2 if cfg.music == 0 => ctx.audio.stop_bgm(),
                    2 => ctx.audio.play_bgm("bgm/th08_01.mid"),
                    _ => {}
                }
                ctx.audio.set_volumes(&ctx.game.cfg);
            }
        }
        if ctx.input.pressed(CONFIRM) {
            match item {
                7 => {
                    sound::play(sound::OK);
                    ctx.game.cfg.reset();
                    ctx.audio.set_volumes(&ctx.game.cfg);
                }
                8 => {
                    sound::play(sound::OK);
                    self.set_screen(KEY_CONFIG);
                }
                9 => {
                    sound::play(sound::OK);
                    self.set_screen(TOP);
                    self.cursor = 7;
                }
                _ => {}
            }
        } else if ctx.input.pressed(CANCEL) {
            // SPEC-GAP: cancel on the option screen is not described in 12 §2.7 / 15 §1.3.
            sound::play(sound::CANCEL);
            if self.cursor == OPTION_ITEMS - 1 {
                self.set_screen(TOP);
                self.cursor = 7;
            } else {
                self.cursor = OPTION_ITEMS - 1;
            }
        }
    }

    // Value VMs 20–43 (12 §2.7): lit = base sprite, dim = base + 1; volume digits are base + digit·2; VMs 25 / 26 are
    // hidden below 30 / 60 plays.
    fn option_values(&mut self, ctx: &Ctx) {
        let cfg = ctx.game.cfg;
        let choices: [(usize, usize, u8); 5] = [
            (20, 7, cfg.lives),
            (27, 2, cfg.colour_mode),
            (29, 3, cfg.music),
            (40, 2, cfg.windowed),
            (42, 2, cfg.slow),
        ];
        for (first, n, v) in choices {
            for k in 0..n {
                self.highlight(ctx, first + k, k == v as usize);
            }
        }
        let plays = play_count(&ctx.game);
        for (vm, need) in [(25, 30), (26, 60)] {
            if plays < need {
                self.vms[vm].set_flag(anmvm::F_VISIBLE, false);
            }
        }
        for (first, item, v) in [(32usize, 3, cfg.bgm_volume), (36, 4, cfg.se_volume)] {
            let dim = if self.cursor == item { 0 } else { 1 };
            let digits = [v as i32 / 100, v as i32 / 10 % 10, v as i32 % 10];
            for (k, d) in digits.into_iter().enumerate() {
                let base = self.base[first + k];
                let vm = &mut self.vms[first + k];
                if base >= 0 {
                    vm.set_sprite(
                        &ctx.anm,
                        if k == 0 {
                            base + dim
                        } else {
                            base + 2 * d + dim
                        },
                    );
                }
                let hidden = k == 0 && v < 100 || k == 1 && v < 10;
                vm.set_flag(anmvm::F_VISIBLE, !hidden && vm.color[anmvm::A] > 0);
            }
            self.highlight(ctx, first + 3, self.cursor == item);
        }
    }

    // SPEC-GAP: key config (screen 3, 15 §1.3) is a later task; the screen returns on any confirm/cancel.
    fn stub(&mut self, ctx: &mut Ctx, name: &str, back: i32, cursor: i32) {
        ctx.ascii.push(Text::new(
            200.0,
            220.0,
            format!("{name}: not implemented yet"),
        ));
        ctx.ascii
            .push(Text::new(200.0, 244.0, "Press Shot or Bomb").argb(0xFF808080));
        if self.frames >= 10 && ctx.input.pressed(CONFIRM | CANCEL) {
            sound::play(sound::CANCEL);
            self.set_screen(back);
            self.cursor = cursor;
            if back == TOP {
                ctx.game.flags &= !(F_PRACTICE | F_SPELL_PRACTICE);
            }
        }
    }
}

impl Default for MainMenu {
    fn default() -> Self {
        Self::new()
    }
}

impl Task<Ctx> for MainMenu {
    // MainMenu_OnRegister 0x470A6C + the loader thread 0x470E10 (00 §3.2, 12 §2.2, 15 §2.2), run synchronously.
    fn on_register(&mut self, ctx: &mut Ctx, chain: Chain, _: u8) -> Result<()> {
        if chain != Chain::Calc {
            return Ok(());
        }
        // SPEC-GAP: 00 §3.2 skips the title music when the previous scene was 5 (result viewer); not tested here.
        if ctx.game.cfg.music != 0 {
            ctx.audio.play_bgm("bgm/th08_01.mid");
        }
        ctx.anm
            .load(&mut ctx.gfx, &ctx.archive, ANM_SLOT, "title01.anm")?;
        ctx.anm.load(
            &mut ctx.gfx,
            &ctx.archive,
            crate::menulist::RT_SLOT,
            "resulttext.anm",
        )?;
        // SPEC-GAP: gauge readout limits are reset on menu load; no spec section names this call.
        let l = crate::player::GaugeLimits::default();
        crate::ascii::gauge_setup(&mut ctx.anm, l.min, l.max);
        for i in 0..VM_COUNT {
            let mut vm = AnmVm::default();
            ctx.anm.set_script(&mut vm, ANM_SLOT, i);
            self.base.push(if vm.has_sprite {
                vm.sprite_index as i32
            } else {
                -1
            });
            self.vms.push(vm);
        }
        // GM flags 15–17 from the clear records (15 §2.2, §2.4, 16 §4). SPEC-GAP: the info00/01/02.jpg notices are not shown yet.
        {
            let g = &mut ctx.game;
            let nc_b = |t: usize| g.clear_nc[t].iter().take(4).any(|m| m & 0x4000 != 0);
            let any_a = |t: usize| g.clear_any[t].iter().take(4).any(|m| m & 0x8000 != 0);
            let bits = ((0..4).any(nc_b) as u32) << 15
                | ((0..4).any(any_a) as u32) << 16
                | ((0..4).all(nc_b) as u32) << 17;
            g.flags = g.flags & !(7 << 15) | bits;
        }
        crate::menulist::compute_last_word_unlocks(&mut ctx.game);
        // A replay was being watched → straight to the replay screen (12 §2.2).
        if std::mem::take(&mut ctx.game.watched_replay) {
            self.screen = REPLAY;
        }
        let g = &ctx.game;
        if g.previous == gamemgr::SCENE_GAME || g.previous == gamemgr::SCENE_RETRY {
            // SPEC-GAP: 12 §2.2 goes through screen 8 → 9 → 11 (practice) or 15 → 16 → 17 (spell practice) on
            // consecutive frames with +0x6C; the port enters 8 (and stays) or 17 directly.
            if g.spell_practice() {
                // Back from a spell-practice game: straight to the card list (16 §6.1).
                self.screen = SPELL_CARD;
                self.lists.sp_row = if g.card >= 205 { 9 } else { g.stage as i32 % 9 };
            } else if g.practice() {
                self.screen = DIFF_PRACTICE;
            }
        }
        // Cursor on menu load by previous scene (12 §2.2), practice flags override.
        self.cursor = if self.screen == TOP {
            let g = &ctx.game;
            match g.previous {
                _ if g.spell_practice() => 2,
                _ if g.practice() => 3,
                gamemgr::SCENE_GAME | gamemgr::SCENE_NEXT_STAGE | gamemgr::SCENE_RESULT => {
                    (g.difficulty > 3) as i32
                }
                gamemgr::SCENE_RESULT_MENU => 5,
                gamemgr::SCENE_MUSIC_ROOM => 6,
                _ => 0,
            }
        } else {
            ctx.game.cfg.difficulty.min(3) as i32
        };
        Ok(())
    }

    fn on_remove(&mut self, ctx: &mut Ctx, chain: Chain, _: u8) {
        if chain != Chain::Calc {
            return;
        }
        if let Some((t, _)) = self.surface.take() {
            ctx.gfx.free_texture(t);
        }
        ctx.anm.unload(&mut ctx.gfx, ANM_SLOT);
        ctx.anm.unload(&mut ctx.gfx, crate::menulist::RT_SLOT);
    }

    fn calc(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        // Session tool hook: screen/cursor at calc entry, as the trace samples them.
        ctx.game.trace_menu = Some((self.screen, self.cursor));
        // The menu starts when its loader has finished (Supervisor loading screen, 12 §1).
        if ctx.game.menu_loading > 0 {
            return Ret::Next;
        }
        // Delayed switch: the new screen starts on the frame after the counter ran out.
        // SPEC-GAP: "after 20 frames" (12 §2.3) is placed at press + 21 from the draw log.
        if let Some(p) = &mut self.pending {
            if p.frames == 0 {
                let (s, c) = (p.screen, p.cursor);
                self.pending = None;
                self.set_screen(s);
                self.cursor = c;
            } else {
                p.frames -= 1;
            }
        }
        let screen = self.screen;
        match self.screen {
            TOP => self.top(ctx),
            OPTION => self.option(ctx),
            DIFF | DIFF_PRACTICE | DIFF_EXTRA => self.difficulty(ctx),
            CHAR | CHAR_PRACTICE | CHAR_EXTRA => self.character(ctx),
            PRACTICE_STAGE => self.practice_stage(ctx),
            KEY_CONFIG => self.stub(ctx, "Key config", OPTION, 8),
            REPLAY => self.replay(ctx),
            SPELL_PRACTICE => self.spell_char(ctx),
            SPELL_CARD => self.spell_card(ctx),
            _ => {}
        }
        if self.screen == screen {
            self.frames += 1;
        }

        // After the screen handler all 142 VMs run, then the description VM (12 §2).
        for vm in self.vms.iter_mut().filter(|v| v.script.is_some()) {
            vm.run(&mut ctx.anm);
        }
        // Leaving the top menu fades the description out (script 9 interrupt 2).
        if screen == TOP && self.screen != TOP {
            self.desc_vm.pending_interrupt = 2;
        }
        if self.desc_on {
            self.desc_vm.run(&mut ctx.anm);
        }
        Ret::Next
    }

    // Menu draw 0x47087F (12 §2.2): surface 0 on its own earlier node (SURFACE_PRIO), VMs 0–141 in order, the
    // description VM, then the per-screen text and the final-route notice.
    fn draw(&mut self, ctx: &mut Ctx, tag: u8) -> Ret {
        if ctx.game.menu_loading > 0 {
            return Ret::Next;
        }
        ctx.gfx.set_viewport(FULLSCREEN);
        if tag == SURFACE_PRIO as u8 {
            if let Some((t, _)) = self.surface {
                ctx.gfx.image(t, 0.0, 0.0);
            }
            return Ret::Next;
        }
        for vm in &mut self.vms {
            ctx.anm.draw(&mut ctx.gfx, vm);
        }
        // Description VM: text.anm script 9 at (320, 448), centred (12 §2.2).
        // SPEC-GAP: 12 §2.2 draws it on every screen, but the port's VM misses the fade-out interrupts off the top menu.
        let top = self.screen == TOP
            || (self.prev_screen == TOP && (self.frames == 0 || self.screen == SPELL_PRACTICE));
        if self.desc_on && top && self.quitting.is_none() && self.pending.is_none() {
            ctx.anm.draw(&mut ctx.gfx, &mut self.desc_vm);
        }
        // A delayed switch whose counter ran out has already set the new screen in the original.
        let screen = match &self.pending {
            Some(p) if p.frames == 0 => p.screen,
            _ => self.screen,
        };
        match screen {
            PRACTICE_STAGE => self.practice_stage_draw(ctx),
            REPLAY => self.replay_draw(ctx),
            SPELL_PRACTICE => self.spell_char_draw(ctx),
            SPELL_CARD => self.spell_card_draw(ctx),
            _ => {}
        }
        // Final-route notice: screen 5 only, frame counter > 8, scratch VM at (400, 170) top-left, Draw2DSnap (12 §2.4).
        if self.screen == CHAR
            && self.frames > 8
            && let Some(spr) = final_notice(
                &ctx.game,
                self.cursor as usize,
                ctx.game.cfg.difficulty as usize,
            )
        {
            let vm = &mut self.notice;
            vm.init();
            vm.anm_slot = ANM_SLOT as _;
            vm.set_sprite(&ctx.anm, spr);
            vm.flags |= anmvm::F_ANCHOR_X | anmvm::F_ANCHOR_Y;
            vm.pos = [400.0, 170.0, 0.0];
            ctx.anm.draw_mode(&mut ctx.gfx, vm, 0);
        }
        Ret::Next
    }
}

#[cfg(test)]
mod tests {
    use super::final_notice;
    use crate::gamemgr::Game;

    #[test]
    fn notice_table() {
        let mut g = Game::default();
        assert_eq!(final_notice(&g, 0, 1), None);
        g.clear_any[0][2] = 0x41;
        assert_eq!(final_notice(&g, 0, 1), Some(145));
        g.clear_nc[0][3] = 0x80;
        assert_eq!(final_notice(&g, 0, 1), Some(147));
        g.clear_nc[0][1] = 0x80;
        assert_eq!(final_notice(&g, 0, 1), Some(148));
        g.clear_any[0][1] = 0x40;
        assert_eq!(final_notice(&g, 0, 1), Some(146));
        assert_eq!(final_notice(&g, 5, 1), Some(147));
    }
}

// Cursor on the difficulty screen: config +0x21, Extra values → Normal (12 §2.3).
// SPEC-GAP: 12 §2.2 item 0 says "clamp ≤ 3 → 2", 12 §2.3 says "> 3 → 1"; the latter is used.
fn last_difficulty(g: &gamemgr::Game) -> i32 {
    match g.cfg.difficulty as i32 {
        d if d > 3 => 1,
        d => d,
    }
}
