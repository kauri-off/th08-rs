//! Main menu (12 §2, `MainMenu_Register` 0x47146D): top menu, difficulty / character select, option screen; the
//! practice, replay and spell-practice screens are in menulist.rs.

use crate::anmvm::{self, AnmVm};
use crate::config::Config;
use crate::game::Ctx;
use crate::gamemgr::{self, F_DEMO, F_PRACTICE, F_REPLAY, F_SPELL_PRACTICE};
use crate::gfx::{FULLSCREEN, TextureId};
use crate::input::{BOMB, DOWN, ENTER, Input, LEFT, PAUSE, RIGHT, SHOT, UP};
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
// Spell-practice character select: only reached through the +0x6C forward (12 §2.2); the top menu goes to 16.
const CHAR_SPELL: i32 = 15;
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
// Option / key-config idle exit (15 §1.3).
const IDLE_EXIT: u32 = 3600;
// Top menu: sub-state 1 from call 9, confirm / cancel from call 19 of each entry (12 §2.2 "Input timing").
const TOP_INPUT: u32 = 9;
const TOP_CONFIRM: u32 = 19;

// Key config rows 0–8 (15 §1.3): the pad field (file order, +0x00…+0x10) each row edits; Skip is 4th.
const KEY_ROWS: [usize; 9] = [0, 1, 2, 8, 3, 4, 5, 6, 7];
// Key-config VMs (15 §1.3 "Key config VMs"): rows 45–56, digit pairs from 57, ShotSlow Off / On 75 / 76.
const VM_KEY_ROW: usize = 45;
const VM_KEY_DIGITS: usize = 57;
const VM_KEY_SHOT_SLOW: usize = 75;
const KEY_ITEMS: i32 = 12;
const KEY_SHOT_SLOW: i32 = 9;
const KEY_RESET: i32 = 10;
const KEY_QUIT: i32 = 11;

// Description VMs (12 §2.2): text.anm script 9, sprite 21 + i; 14 strips shared by the top menu, option and key config.
const DESC_VMS: usize = 14;

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

// Option descriptions, table 0x4C8664 (12 §2.2).
const OPTION_DESCRIPTIONS: [&str; 10] = [
    "プレイヤーの初期数を変更します。（初期設定　３）",
    "画面の色数を変更します。３２ＢＩＴだと最も綺麗に表示されます。",
    "ＢＧＭの再生方法を変更します。（初期設定　ＷＡＶ）",
    "ＢＧＭの音量を調整します",
    "効果音の音量を調整します",
    "ウィンドウかフルスクリーンか選択します",
    "弾が多い場面でわざと処理落ちさせます(スコア、リプレイ記録不可)",
    "全て初期設定にします",
    "パッド操作のボタン配置を変更します",
    "おいそれと終了します",
];

// Key-config descriptions, table 0x4C8690 (12 §2.2).
const KEY_DESCRIPTIONS: [&str; 12] = [
    "ショット、決定ボタンを設定します",
    "ボム、キャンセルボタンを設定します",
    "低速移動ボタンを設定します",
    "メッセージスキップボタンを設定します",
    "ポーズボタンを設定します",
    "上移動ボタンを設定します",
    "下移動ボタンを設定します",
    "左移動ボタンを設定します",
    "右移動ボタンを設定します",
    "ショット押しっぱなしで低速移動になるようにします",
    "初期設定に戻します",
    "おおよそ終了します",
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

// `Info_Show` 0x471181 (16 §4): the unlock notices queued by MainMenu_OnRegister. The original runs each as a
// blocking loop with its own Present; the port runs one loop step per frame in place of the calc and draw chains.
#[derive(Default)]
pub struct InfoNotices {
    pub(crate) queue: std::collections::VecDeque<&'static str>,
    // 0x164D530 as of the registering frame: only Supervisor_Calc writes it, so it stays frozen in the loop.
    pub(crate) frozen_prev: u16,
    // Surface 0 (title/infoNN.jpg), the loop counter t and the t drawn by this iteration.
    cur: Option<(TextureId, i32)>,
    drawn: i32,
}

const INFO_FRAMES: i32 = 6000;
const INFO_FADE_OUT: i32 = 5980;
// Shot | Bomb | Enter.
const INFO_KEYS: u16 = 0x1003;

impl InfoNotices {
    pub fn active(&self) -> bool {
        self.cur.is_some() || !self.queue.is_empty()
    }

    // A notice loop is running (queued ones start at the next calc).
    pub fn showing(&self) -> bool {
        self.cur.is_some()
    }

    // Logic of one loop iteration (16 §4): the draw of t, poll, the press test, t := 5980, t += 1. So a press shows one
    // iteration later as t = 5981. False once no notice is left (the frame then runs normally).
    pub fn calc(ctx: &mut Ctx) -> bool {
        loop {
            match ctx.info.cur {
                Some((t, f)) if f >= INFO_FRAMES => {
                    // After the loop, surface 0 is released.
                    ctx.gfx.free_texture(t);
                    ctx.info.cur = None;
                }
                Some(_) => break,
                None => {
                    let Some(file) = ctx.info.queue.pop_front() else {
                        return false;
                    };
                    match ctx
                        .archive
                        .read(file)
                        .and_then(|b| ctx.gfx.load_image(file, &b))
                    {
                        Ok(t) => ctx.info.cur = Some((t, 0)),
                        Err(e) => log::warn!("{file}: {e:#}"),
                    }
                }
            }
        }
        // g_InputRawCur := Input_Poll(); the previous word and the repeat counter are left alone.
        let pad = ctx.pad.poll();
        ctx.input.poll_joystick(&ctx.game.cfg, pad);
        ctx.input.held = ctx.input.raw | ctx.input.joy;
        let (held, frozen) = (ctx.input.held, ctx.info.frozen_prev);
        let Some((_, t)) = &mut ctx.info.cur else {
            return false;
        };
        ctx.info.drawn = *t;
        let (next, pressed) = info_step(*t, held, frozen);
        if pressed {
            sound::play(sound::OK);
        }
        *t = next;
        true
    }

    // Draw of the iteration: the picture full-screen and the black quad (0, 0)–(639, 479).
    pub fn draw(ctx: &mut Ctx) {
        let Some((tex, _)) = ctx.info.cur else {
            return;
        };
        ctx.gfx.set_viewport(FULLSCREEN);
        ctx.gfx.image(tex, 0.0, 0.0);
        if let Some(a) = info_fade_alpha(ctx.info.drawn) {
            ctx.gfx.rect(0.0, 0.0, 639.0, 479.0, [0, 0, 0, a]);
        }
    }
}

// Press test and counter step of one notice iteration (16 §4): 179 < t < 5980 and the masked word non-zero and unlike
// the frozen previous word → t := 5980 (SE 10); then t += 1.
fn info_step(t: i32, held: u16, frozen: u16) -> (i32, bool) {
    let (h, f) = (held & INFO_KEYS, frozen & INFO_KEYS);
    let pressed = 179 < t && t < INFO_FADE_OUT && h != 0 && h != f;
    (if pressed { INFO_FADE_OUT } else { t } + 1, pressed)
}

// Notices of the unlock bits `bits` (GM flags 15–17) not yet set in `flags`, in the 16 §4 table order.
fn info_files(bits: u32, flags: u32) -> impl Iterator<Item = &'static str> {
    [(15, "info00.jpg"), (16, "info02.jpg"), (17, "info01.jpg")]
        .into_iter()
        .filter(move |(b, _)| bits & !flags & 1 << b != 0)
        .map(|(_, f)| f)
}

// Black-quad alpha byte of notice frame t (16 §4). QUIRK: the fade-in byte starts at 253 and falls (wrapping) before the
// quad vanishes; the fade-out byte wraps for t < 5980.
// The fade-in ends at 10 (t = 19), 16 §4.
fn info_fade_alpha(t: i32) -> Option<u8> {
    if t < 20 {
        Some(((60 - t) * 255 / 20) as u8)
    } else if t > 5940 {
        Some(((t - INFO_FADE_OUT) * 255 / 20) as u8)
    } else {
        None
    }
}

// Unlock checks (15 §2.4) on the score.dat clear masks; debug `TH08_UNLOCK=1` unlocks everything.
// Text_RenderCentered start x (12 §2.2): trunc(256 − len·15/4), len = the Shift-JIS byte length (ASCII and
// half-width katakana 1 byte, the rest 2).
fn centred_x(s: &str) -> f32 {
    let len: usize = s
        .chars()
        .map(|c| {
            if c.is_ascii() || ('\u{FF61}'..='\u{FF9F}').contains(&c) {
                1
            } else {
                2
            }
        })
        .sum();
    (256.0 - len as f32 * 15.0 / 4.0).trunc()
}

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
    // GameFlags bits cleared together with the switch (spell-practice cancel clears bit 0 here, 16 §6.1).
    pub(crate) clear_flags: u32,
}

// The 0x14490-byte menu object, global 0x18BDE08 (12 §2 field table).
pub struct MainMenu {
    // +0xC2B8: VM i runs title01.anm script i; `base` = each script's own (selected) sprite.
    pub(crate) vms: Vec<AnmVm>,
    pub(crate) base: Vec<i32>,
    // Surface slot 0 (title00.png / select00.png), copied before everything (12 §2.2).
    pub(crate) surface: Option<(TextureId, &'static str)>,
    pub(crate) raster: crate::text::Raster,
    // The 14 description VMs and the current one, pointer +0xC2BC (None = 0), run and drawn while set (12 §2.2).
    pub(crate) desc: Vec<AnmVm>,
    pub(crate) desc_cur: Option<usize>,
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
    // +0x6C: 1 / 2 = back from a practice / spell-practice game; screens 8, 9, 15, 16 forward while set (12 §2.2).
    pub(crate) back: u8,
    // The handler returned 2: MainMenu_Calc runs again in the same frame (12 §2 screen-entry exception).
    pub(crate) again: bool,
    // The option handler returned 5 (15 §1.3).
    restart: bool,
    // Config snapshot taken on entering the option screen (12 §2.2 item 7).
    pub(crate) snapshot: Config,
    // Key config working copy of the 9 pad fields (menu +0x14440…+0x14451, file order) and the button found on the
    // previous frame (global 0x4C868C, 15 §1.3).
    pub(crate) keys: [i16; 9],
    pub(crate) last_button: i16,
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
            desc: Vec::new(),
            desc_cur: None,
            notice: AnmVm::default(),
            screen: TOP,
            prev_screen: TOP,
            frames: 0,
            cursor: 0,
            prev_cursor: -1,
            idle: 0,
            quitting: None,
            pending: None,
            back: 0,
            again: false,
            restart: false,
            snapshot: Config::default(),
            keys: crate::config::PAD_DEFAULT,
            last_button: -1,
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

    // Text_RenderCentered 0x466650 (12 §2.2, 10 §6): `text` into description strip `i` (text.anm sprite 21 + i) at
    // x = trunc(256 − len·15/4), len = Shift-JIS byte length; W = 512, font 15, COLORREF 0x00FFF0E0, ±2 px outline.
    fn render_description(&mut self, ctx: &mut Ctx, i: usize, text: &str) {
        let Some(sp) = ctx.anm.sprite(0, 21 + i).copied() else {
            return;
        };
        let t = ctx.gfx.texture(sp.tex);
        let (w, ty) = (t.width, (sp.uv[1] * t.height as f32).round() as u32);
        let size = 15;
        let rgba = self.raster.render_outline(
            text,
            [224, 240, 255],
            w as usize,
            size,
            centred_x(text),
            0x300000,
        );
        ctx.gfx.write_texture(sp.tex, 0, ty, w, size as u32, &rgba);
    }

    // Change test of the top menu, option and key config (12 §2.2): cursor ≠ +0x08 → +0xC2BC := description VM
    // [cursor], interrupt 1.
    fn desc_follow_cursor(&mut self) {
        if self.cursor != self.prev_cursor {
            self.prev_cursor = self.cursor;
            let i = (self.cursor.max(0) as usize).min(DESC_VMS - 1);
            self.desc_cur = Some(i);
            self.desc[i].pending_interrupt = 1;
        }
    }

    // Interrupt 2 (fade out over 60 frames) to the current description VM; range interrupts never reach it (12 §2.2).
    pub(crate) fn desc_fade_out(&mut self) {
        if let Some(i) = self.desc_cur {
            self.desc[i].pending_interrupt = 2;
        }
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

    // All 9 items unselected (base + 1), the cursor item its base sprite, then locked items 0xFF404040; the colour is
    // never written back to white (12 §2.2 "Highlight").
    fn highlight_items(&mut self, ctx: &Ctx) {
        for i in 0..ITEMS {
            self.highlight(ctx, 1 + i, i as i32 == self.cursor);
        }
        for i in 0..ITEMS {
            if Self::item_locked(ctx, i) {
                self.vms[1 + i].color = [0x40, 0x40, 0x40, 0xFF];
            }
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
            // Back from a watched replay: the entry block switches to the replay screen and returns 1 (12 §2.2).
            if ctx.game.replay() {
                self.set_screen(REPLAY);
                self.interrupt_all(13);
                self.desc_fade_out();
                ctx.game.flags &= !F_REPLAY;
                return;
            }
            // Back from a practice game (+0x6C ≠ 0): bit 14 set again for 2, screen 8 + 7·bit 14, range interrupt 5,
            // description interrupt 2, return 1; trace shows screen 0 on the release frame (12 §2.2, RE-Q55).
            if self.back != 0 {
                if self.back == 2 {
                    ctx.game.flags |= F_SPELL_PRACTICE;
                }
                self.set_screen(if ctx.game.spell_practice() {
                    CHAR_SPELL
                } else {
                    DIFF_PRACTICE
                });
                self.interrupt_all(5);
                self.desc_fade_out();
                return;
            }
        }
        // Calls 0–8 render description k = +0x1442C into strip k; the pointer keeps its VM until the change test of
        // call 9 (12 §2.2), so a previous screen's description stays drawn meanwhile.
        if self.frames < TOP_INPUT {
            let k = self.frames as usize;
            self.render_description(ctx, k, DESCRIPTIONS[k]);
        }
        // Calls 0–8 are sub-state 0; call 9 enters sub-state 1 (+0x08 := −1, +0x14430 := 0), and cursor, demo counter
        // and change test run from that call on (12 §2.2 "Input timing"). The port keeps counting `frames` instead of
        // the original's +0x1442C := 0 in call 9. Option's wait (sub-state 3) runs none of them.
        let input = self.frames >= TOP_INPUT && self.pending.is_none();
        if self.frames == TOP_INPUT {
            self.prev_cursor = -1;
            self.idle = 0;
        }
        // Highlight only in the call-0 entry block and when the cursor moved (12 §2.2 "Highlight").
        if self.frames == 0 {
            self.highlight_items(ctx);
        }
        if input && self.quitting.is_none() {
            let locked: Vec<bool> = (0..ITEMS).map(|i| Self::item_locked(ctx, i)).collect();
            let old = self.cursor;
            self.move_cursor(ctx, ITEMS as i32, UP, DOWN, |c| !locked[c as usize]);
            if self.cursor != old {
                self.highlight_items(ctx);
            }
        }
        if !input {
            return;
        }
        self.desc_follow_cursor();

        if let Some(f) = &mut self.quitting {
            // Item 8: after 60 frames free the VMs, scene −1 (12 §2.2).
            *f += 1;
            if *f >= 60 {
                ctx.game.request(-1);
            }
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
        // Confirm / cancel only while +0x1442C ≥ 10 after its reset in call 9, i.e. from call 19 (12 §2.2).
        if self.frames < TOP_CONFIRM {
            return;
        }
        // Confirm actions of the 12 §2.2 item table: sfx 10, then a sound flush (Sound_Update in place, 13 §1.3).
        if ctx.input.pressed(CONFIRM) {
            sound::play(sound::OK);
            ctx.sound_update();
            // Every confirm that leaves to another screen or scene fades the description out; Option and Quit do not.
            if self.cursor < 7 {
                self.desc_fade_out();
            }
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
                    self.cursor = g.shot as i32;
                }
                3 => {
                    g.flags.set(g.flags & !F_SPELL_PRACTICE | F_PRACTICE);
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
                    // The in-place MainMenu_Option call adds one to +0x10, so screen 2 first runs on P + 30 (12 §2).
                    // No range interrupt and no highlight: item 7 stays lit until the option entry's interrupt 3.
                    self.cursor = 0;
                    self.pending = Some(Pending {
                        frames: 29,
                        screen: OPTION,
                        cursor: 0,
                        clear_flags: 0,
                    });
                }
                _ => {
                    // 12 §2.2: no music change in WAV mode; MIDI mode plays the init.mid reset (02 §5), no fade.
                    ctx.audio.midi_reset();
                    self.interrupt_all(1);
                    self.quitting = Some(0);
                }
            }
            // Only the Option press falls through to the cancel test (12 §2.2 item 7).
            if self.pending.is_none() {
                return;
            }
        }
        if ctx.input.pressed(CANCEL) && self.cursor != ITEM_QUIT {
            // Cancel highlight: old item base + 1, cursor := Quit, Quit base sprite (12 §2.2 "Highlight" (c)).
            self.highlight(ctx, 1 + self.cursor as usize, false);
            self.cursor = ITEM_QUIT;
            self.highlight(ctx, 1 + ITEM_QUIT as usize, true);
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
            self.desc_cur = None;
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
        // Back from a practice game: 8 → 9 (cursor 0) at once, return 1 (12 §2.2).
        // The whole entry block runs before the forward, which skips the counter increments (12 §2.3).
        if self.back != 0 {
            self.set_screen(self.screen + 1);
            self.cursor = 0;
            return;
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
                clear_flags: 0,
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
        match self.screen {
            CHAR_EXTRA => extra_unlocked(&ctx.game, c as usize),
            CHAR_SPELL => Self::sp_shot_ok(ctx, c),
            _ => true,
        }
    }

    fn char_cursor(&mut self, ctx: &Ctx) {
        let n = if extra_all(&ctx.game) { 12 } else { 4 };
        self.cursor = (ctx.game.shot as i32).min(n - 1);
        if !self.char_allowed(ctx, self.cursor) {
            self.cursor = (0..n).find(|&c| self.char_allowed(ctx, c)).unwrap_or(0);
        }
    }

    // Screens 5 / 9 / 13 / 15, 0x46A625 (12 §2.4); input from +0x1442C == 8 (12 §2).
    fn character(&mut self, ctx: &mut Ctx) {
        let n = if extra_all(&ctx.game) { 12 } else { 4 };
        if self.frames == 0 {
            self.interrupt_all(8);
            // Difficulty plate slides to (16, 384), not on screen 15 (12 §2.4).
            if self.screen != CHAR_SPELL {
                self.vms[VM_DIFF + (ctx.game.cfg.difficulty as usize).min(4)].pending_interrupt = 9;
            }
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
        // Back from a game with +0x6C: 9 → 11 (cursor = g_StageIdx) / 15 → 16 (stage row = g_StageIdx) at once,
        // return 1, one screen per frame (12 §2.2).
        if self.back != 0 {
            self.set_screen(if self.screen == CHAR_SPELL {
                SPELL_PRACTICE
            } else {
                PRACTICE_STAGE
            });
            self.cursor = ctx.game.stage as i32;
            return;
        }
        if self.frames < 8 || self.pending.is_some() {
            return;
        }
        // Confirm (12 §2.4): shot = cursor; practice → screen 11; else difficulty = config +0x21 (Extra 4, stage 8),
        // replay flag cleared, music stopped, scene 2.
        if ctx.input.pressed(CONFIRM) {
            sound::play(sound::OK);
            let g = &mut ctx.game;
            g.shot = self.cursor as u8;
            match self.screen {
                // 9 → 11 / 15 → 16 return 2: the new screen runs in this frame (12 §2).
                CHAR_PRACTICE => {
                    self.set_screen(PRACTICE_STAGE);
                    self.again = true;
                }
                CHAR_SPELL => {
                    self.set_screen(SPELL_PRACTICE);
                    self.again = true;
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
            // Cancel (12 §2.4): SE 11 + sound flush, g_ShotType := cursor on every character screen.
            sound::play(sound::CANCEL);
            ctx.sound_update();
            ctx.game.shot = self.cursor as u8;
            if self.screen == CHAR_SPELL {
                // Screen 15: SE 11 again (the flush restarts the same buffer: one sound), range interrupt 6, top menu
                // cursor 2 after 30 frames (12 §2.4).
                sound::play(sound::CANCEL);
                self.interrupt_all(6);
                ctx.game.flags &= !F_PRACTICE;
                self.pending = Some(Pending {
                    frames: 30,
                    screen: TOP,
                    cursor: 2,
                    clear_flags: 0,
                });
            } else {
                // No interrupt here (12 §2.4): the difficulty screen's enter interrupt 7 does the transition.
                self.set_screen(self.screen - 1);
            }
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
    // The cursor is set by the switch that enters it (0 from the top menu, 8 from key config).
    fn option(&mut self, ctx: &mut Ctx) {
        self.option_run(ctx);
        // Idle counter +0xC294: += 1 at the end of every call; a switch zeroes it (15 §1.3 idle exit).
        self.idle = if self.screen == OPTION {
            self.idle + 1
        } else {
            0
        };
    }

    fn option_run(&mut self, ctx: &mut Ctx) {
        if self.frames == 0 {
            self.interrupt_all(3);
            self.idle = 0;
            for (k, t) in OPTION_DESCRIPTIONS.iter().enumerate() {
                self.render_description(ctx, k, t);
            }
        }
        for i in 0..OPTION_ITEMS {
            self.highlight(ctx, VM_OPTION + i as usize, i == self.cursor);
        }
        self.option_values(ctx);
        if self.pending.is_some() {
            return;
        }
        // The cursor moves from the first call; Left/Right/confirm/cancel from +0x1442C ≥ 4 (12 §2 input start).
        self.move_cursor(ctx, OPTION_ITEMS, UP, DOWN, |_| true);
        self.desc_follow_cursor();
        let item = self.cursor;
        // Volume preview (15 §1.3): SE 0x1D whenever the screen frame counter +0x1442C is a multiple of 50.
        if matches!(item, 3 | 4) && self.frames % 50 == 0 {
            sound::play(sound::TIMEOUT);
        }
        if self.frames < 4 {
            return;
        }
        let inp = ctx.input;
        let lmax = lives_max(&ctx.game);
        let cfg = &mut ctx.game.cfg;
        if matches!(item, 3 | 4) {
            let v = Self::option_value(cfg, item).unwrap();
            let old = *v;
            *v = volume_step(old, &inp);
            if *v != old {
                ctx.audio.set_volumes(&ctx.game.cfg);
            }
        } else {
            let dir = if inp.pressed_or_repeat(LEFT) {
                -1
            } else if inp.pressed_or_repeat(RIGHT) {
                1
            } else {
                0
            };
            if dir != 0
                && let Some(v) = Self::option_value(cfg, item)
            {
                let old = *v;
                let max = match item {
                    0 => lmax,
                    2 => 2,
                    _ => 1,
                };
                *v = (*v as i32 + dir).rem_euclid(max + 1) as u8;
                if *v != old {
                    sound::play(sound::TIMEOUT);
                    // Item 2 (15 §1.3): `Music_Stop` in the old mode, Left from mode 2 also the init.mid reset, then
                    // `Music_Play(8, th08_01)` + `Music_PlaySlot(8, 0)` in the new mode (nothing in mode 0).
                    if item == 2 {
                        ctx.audio.stop_bgm();
                        if dir < 0 && old == 2 {
                            ctx.audio.midi_reset();
                        }
                    }
                    ctx.audio.set_volumes(&ctx.game.cfg);
                    if item == 2 {
                        ctx.audio.load_bgm(8, "bgm/th08_01.mid");
                        ctx.audio.play_slot(8);
                        // Music_PlaySlot marks track 0 heard unless replay playback or demo; mode 0 marks nothing (02 §4).
                        let g = &mut ctx.game;
                        if matches!(g.cfg.music, 1 | 2)
                            && g.flags & (gamemgr::F_REPLAY | gamemgr::F_DEMO) == 0
                        {
                            g.score.plst.music[0] = 1;
                        }
                    }
                }
            }
        }
        // Idle exit (15 §1.3), from +0x1442C ≥ 4 only: a non-zero polled word g_InputRawCur resets the counter, then
        // ≥ 3600 leaves as Quit, so the exit comes in call 3600 after the last call with input.
        if ctx.input.held != 0 {
            self.idle = 0;
        }
        if self.idle >= IDLE_EXIT {
            self.option_leave(ctx);
            return;
        }
        if ctx.input.pressed(CONFIRM) {
            match item {
                7 => {
                    sound::play(sound::OK);
                    ctx.game.cfg.reset();
                    ctx.audio.set_volumes(&ctx.game.cfg);
                }
                8 => {
                    // Key-config cursor 0 is set here, not by its entry block (15 §1.3).
                    self.cursor = 0;
                    sound::play(sound::OK);
                    self.set_screen(KEY_CONFIG);
                }
                9 => self.option_leave(ctx),
                _ => {}
            }
        } else if ctx.input.pressed(CANCEL) {
            // Bomb/ESC moves the cursor to Quit (SE 11), on Quit it leaves (15 §1.3).
            if self.cursor == OPTION_ITEMS - 1 {
                self.option_leave(ctx);
            } else {
                sound::play(sound::CANCEL);
                self.cursor = OPTION_ITEMS - 1;
            }
        }
    }

    // Leaving the option menu (15 §1.3): SE 11, top cursor 7, screen 0 at once; return 5 when the colour depth or
    // window mode differs from the snapshot taken on entry (12 §2.2 item 7); return 5 restarts the program (00 §2).
    fn option_leave(&mut self, ctx: &mut Ctx) {
        sound::play(sound::CANCEL);
        self.set_screen(TOP);
        self.cursor = 7;
        let (a, b) = (&self.snapshot, &ctx.game.cfg);
        if a.colour_mode != b.colour_mode || a.windowed != b.windowed {
            self.restart = true;
        }
    }

    // Key config, screen 3, MainMenu_KeyConfig 0x469636 (15 §1.3).
    fn key_config(&mut self, ctx: &mut Ctx) {
        self.key_config_run(ctx);
        // Idle counter +0xC294 += 1 at the end of every call (15 §1.3 idle exit).
        self.idle = if self.screen == KEY_CONFIG {
            self.idle + 1
        } else {
            0
        };
    }

    fn key_config_run(&mut self, ctx: &mut Ctx) {
        // Entry: range interrupt 4, row sprites, the 12 description strips, working copy of the 9 fields; the live
        // Up/Down are −1 while the screen is open (15 §1.3 "Call order" step 1).
        if self.frames == 0 {
            self.interrupt_all(4);
            self.key_rows(ctx);
            for (k, t) in KEY_DESCRIPTIONS.iter().enumerate() {
                self.render_description(ctx, k, t);
            }
            let cfg = &mut ctx.game.cfg;
            self.keys = cfg.pad;
            cfg.pad[4] = -1;
            cfg.pad[5] = -1;
            self.idle = 0;
        }
        let old = self.cursor;
        self.move_cursor(ctx, KEY_ITEMS, UP, DOWN, |_| true);
        if self.cursor != old {
            self.key_rows(ctx);
        }
        self.desc_follow_cursor();
        // Steps 4–5 (0x469FA9 + ShotSlow) before assign / toggle / Reset / exit: changes show one call later.
        self.key_values(ctx);
        let inp = ctx.input;
        // Assigning (joystick only): a new first-down raw button on rows 0–8 swaps with any row holding it, SE 10.
        let b = inp.pad.first_button();
        if b >= 0 && b != self.last_button && self.cursor < KEY_ROWS.len() as i32 {
            assign_key(&mut self.keys, self.cursor as usize, b);
            sound::play(sound::OK);
        }
        // 0x4C868C := scan result every call (15 §1.3; the port's −1 for "none" stands for the original's 32).
        self.last_button = b;
        let cfg = &mut ctx.game.cfg;
        if self.cursor == KEY_SHOT_SLOW && (inp.pressed(LEFT) || inp.pressed(RIGHT)) {
            cfg.shot_slow ^= 1;
        }
        // Idle exit: only the polled word g_InputRawCur resets the counter, not unmapped pad buttons (15 §1.3).
        if inp.held != 0 {
            self.idle = 0;
        }
        if inp.pressed(CONFIRM) && self.cursor == KEY_RESET {
            sound::play(sound::OK);
            self.keys = crate::config::PAD_DEFAULT;
            cfg.shot_slow = 1;
        } else if inp.pressed(CONFIRM) && self.cursor == KEY_QUIT || self.idle >= IDLE_EXIT {
            // Quit (or 3600 idle frames): SE 11, screen 2 cursor 8, the working copy written back (restores Up/Down).
            sound::play(sound::CANCEL);
            cfg.pad = self.keys;
            self.set_screen(OPTION);
            self.cursor = 8;
        }
    }

    // Row VMs 45–56: cursor row = script sprite, others sprite + 1 (15 §1.3).
    fn key_rows(&mut self, ctx: &Ctx) {
        for i in 0..KEY_ITEMS {
            self.highlight(ctx, VM_KEY_ROW + i as usize, i == self.cursor);
        }
    }

    // 0x469FA9, every call (15 §1.3): digit pairs 57 + 2r / 58 + 2r of the working copy (both disabled for −1, else
    // script sprite + 2·digit), then ShotSlow VMs 75 / 76.
    fn key_values(&mut self, ctx: &Ctx) {
        for (r, &f) in KEY_ROWS.iter().enumerate() {
            let v = self.keys[f] as i32;
            for (k, d) in [v / 10, v % 10].into_iter().enumerate() {
                let i = VM_KEY_DIGITS + 2 * r + k;
                let vm = &mut self.vms[i];
                vm.set_flag(anmvm::F_ENABLED, v >= 0);
                if v >= 0 && self.base[i] >= 0 {
                    vm.set_sprite(&ctx.anm, self.base[i] + 2 * d);
                }
            }
        }
        let on = (ctx.game.cfg.shot_slow != 0) as usize;
        for k in 0..2 {
            self.highlight(ctx, VM_KEY_SHOT_SLOW + k, k == on);
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
        // 0x18B8A2C := 0 (13 §3).
        crate::enemy::set_load_frames(0);
        if title_music(&ctx.game) {
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
        // MainMenu_OnRegister → Ascii_SetupGaugeVms; hidden in the menu, redone at StageStart (12 §5.9).
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
        // GM flags 15–17 from the clear records (15 §2.2, §2.4, 16 §4).
        {
            let g = &mut ctx.game;
            let nc_b = |t: usize| g.clear_nc[t].iter().take(4).any(|m| m & 0x4000 != 0);
            let any_a = |t: usize| g.clear_any[t].iter().take(4).any(|m| m & 0x8000 != 0);
            let bits = ((0..4).any(nc_b) as u32) << 15
                | ((0..4).any(any_a) as u32) << 16
                | ((0..4).all(nc_b) as u32) << 17;
            // Unlock notices (16 §4): a condition now true whose flag bit is still clear. The port loads CLRD at boot,
            // where the original's in-memory CLRD is still empty, so the first menu after boot shows none.
            if g.previous != gamemgr::SCENE_BOOT {
                ctx.info.queue.extend(info_files(bits, g.flags.get()));
                ctx.info.frozen_prev = ctx.input.prev;
            }
            g.flags.set(g.flags & !(7 << 15) | bits);
        }
        crate::menulist::compute_last_word_unlocks(&mut ctx.game);
        // The 14 description VMs start script 9 (fade-in by itself); the loader ends with +0xC2BC := VM 0 (12 §2.2).
        self.desc = (0..DESC_VMS)
            .map(|i| {
                let mut vm = AnmVm::default();
                ctx.anm.set_script(&mut vm, 0, 9);
                vm.set_sprite(&ctx.anm, 21 + i as i32);
                vm
            })
            .collect();
        self.desc_cur = Some(0);
        // Back from a replay (12 §2.2): bit 3 → shot := 0; a demo also clears bit 3, so only a watched replay keeps it
        // for the Top entry block's switch to the replay screen.
        {
            let g = &mut ctx.game;
            if g.replay() {
                g.shot = 0;
            }
            if g.flags & F_DEMO != 0 {
                g.flags &= !F_REPLAY;
            }
        }
        // +0x6C := 1 + (flags bit 14) whenever flags bit 0 is set, whatever the previous scene; screen stays 0 and the
        // Top entry block does the switch to 8/15 (12 §2.2, RE-Q55). A spell-practice replay has bit 14 without bit 0.
        let g = &ctx.game;
        self.back = if g.practice() {
            1 + g.spell_practice() as u8
        } else {
            0
        };
        // Cursor on menu load by previous scene, then the practice override 3 / 2 (12 §2.2).
        self.cursor = match g.previous {
            _ if g.practice() && g.spell_practice() => 2,
            _ if g.practice() => 3,
            gamemgr::SCENE_GAME | gamemgr::SCENE_NEXT_STAGE | gamemgr::SCENE_RESULT => {
                (g.difficulty > 3) as i32
            }
            gamemgr::SCENE_RESULT_MENU => 5,
            gamemgr::SCENE_MUSIC_ROOM => 6,
            _ => 0,
        };
        // After the cursor rule: flags &= ~0x4001, later bit 1 := 0 (12 §2.2).
        ctx.game.flags &= !(F_PRACTICE | F_SPELL_PRACTICE);
        ctx.game.flags &= !F_DEMO;
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
        // The draw node can outlive a calc node removed by return 0 (00 §3.1); it then draws nothing.
        self.vms.clear();
        self.desc_cur = None;
    }

    fn calc(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        // Session tool hook: screen/cursor at calc entry, as the trace samples them.
        ctx.game.trace_menu = Some((self.screen, self.cursor));
        // The menu starts when its loader has finished (Supervisor loading screen, 12 §1).
        if ctx.game.menu_loading > 0 {
            return Ret::Next;
        }
        // Delayed switch: "+0x10 > 19" is true in the call P + 20 after a press on P, which calls Menu_SetScreen
        // after the handler's early return; the new screen first runs on P + 21 (12 §2).
        if let Some(p) = &mut self.pending {
            p.frames = p.frames.saturating_sub(1);
        }
        // A handler that returns 2 makes the chain call this node again at once (00 §3), so the new screen's
        // handler, RunRange and the description VM run a second time in this frame (12 §2 entry exception).
        loop {
            let screen = self.screen;
            match self.screen {
                TOP => self.top(ctx),
                OPTION => self.option(ctx),
                DIFF | DIFF_PRACTICE | DIFF_EXTRA => self.difficulty(ctx),
                CHAR | CHAR_PRACTICE | CHAR_EXTRA | CHAR_SPELL => self.character(ctx),
                PRACTICE_STAGE => self.practice_stage(ctx),
                KEY_CONFIG => self.key_config(ctx),
                // Replay with no recorded stage: the handler returns 0, which removes the menu's calc node (15 §3.8).
                REPLAY => {
                    if !self.replay(ctx) {
                        return Ret::Remove;
                    }
                }
                SPELL_PRACTICE => self.spell_char(ctx),
                SPELL_CARD => self.spell_card(ctx),
                _ => {}
            }
            // Option left with a changed colour depth / window mode: return 5, the program restarts (15 §1.3, 00 §2).
            if std::mem::take(&mut self.restart) {
                return Ret::ErrorQuit;
            }
            if let Some(p) = self.pending.take_if(|p| p.frames == 0) {
                self.set_screen(p.screen);
                self.cursor = p.cursor;
                ctx.game.flags &= !p.clear_flags;
                // Top item 7: screen 2 with the config snapshot (12 §2.2).
                if p.screen == OPTION {
                    self.snapshot = ctx.game.cfg;
                }
            }
            if self.screen == screen {
                self.frames += 1;
            }

            // After the screen handler all 142 VMs run, then the description VM (12 §2).
            for vm in self.vms.iter_mut().filter(|v| v.script.is_some()) {
                vm.run(&mut ctx.anm);
            }
            if let Some(i) = self.desc_cur {
                self.desc[i].run(&mut ctx.anm);
            }
            if !std::mem::take(&mut self.again) {
                return Ret::Next;
            }
        }
    }

    // Menu draw 0x47087F (12 §2.2): surface 0 on its own earlier node (SURFACE_PRIO), VMs 0–141 in order, the
    // description VM, then the per-screen text and the final-route notice.
    fn draw(&mut self, ctx: &mut Ctx, tag: u8) -> Ret {
        if ctx.game.menu_loading > 0 || self.vms.is_empty() {
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
        // The current description VM (text.anm script 9 at (320, 448)) on every screen while +0xC2BC ≠ 0 (12 §2.2).
        if let Some(i) = self.desc_cur {
            ctx.anm.draw(&mut ctx.gfx, &mut self.desc[i]);
        }
        match self.screen {
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

// Option items 3/4 volume, 15 §1.3: (a) Left/Right edge ∓/±4 clamped to 0..100, then (b) on an edge or with the
// direction held while the unchanged-input counter 0x164D53C > 29, ∓/±1 if still above 0 / below 100.
fn volume_step(v: u8, inp: &Input) -> u8 {
    let mut v = v as i32;
    if inp.pressed(LEFT) {
        v = (v - 4).max(0);
    } else if inp.pressed(RIGHT) {
        v = (v + 4).min(100);
    }
    let held = inp.same_frames > 29;
    if (inp.pressed(LEFT) || held && inp.down(LEFT)) && v > 0 {
        v -= 1;
    } else if (inp.pressed(RIGHT) || held && inp.down(RIGHT)) && v < 100 {
        v += 1;
    }
    v as u8
}

// Key config assign (15 §1.3): rows holding `b` get the row's old button, then the row gets `b`.
fn assign_key(keys: &mut [i16; 9], row: usize, b: i16) {
    let field = KEY_ROWS[row];
    let old = keys[field];
    for k in keys.iter_mut().filter(|k| **k == b) {
        *k = old;
    }
    keys[field] = b;
}

// Loader thread 0x470E10 (00 §3.2): title music unless music is off or the previous scene was 5 (result screen).
fn title_music(g: &gamemgr::Game) -> bool {
    g.cfg.music != 0 && g.previous != gamemgr::SCENE_RESULT_MENU
}

#[cfg(test)]
mod tests {
    use super::{
        INFO_FADE_OUT, assign_key, centred_x, final_notice, info_fade_alpha, info_files, info_step,
        title_music, volume_step,
    };
    use crate::gamemgr::{self, Game};

    fn ctx() -> Option<crate::game::Ctx> {
        let dir = crate::settings::test_game_dir()?;
        let gfx = pollster::block_on(crate::gfx::Gfx::headless()).ok()?;
        let src = crate::assets::Source::native(dir.into());
        Some(pollster::block_on(crate::game::Ctx::load_with(src, gfx, false)).unwrap())
    }

    // 12 §2.2 "Back from a replay": bit 3 survives register (screen 0, previous-scene cursor), the Top entry block
    // switches to screen 7 and clears it; a demo clears bit 3 at register and stays on the top menu.
    #[test]
    fn back_from_replay() {
        use crate::task::{Chain, Task};
        let Some(mut c) = ctx() else {
            return;
        };
        for demo in [false, true] {
            c.game.previous = gamemgr::SCENE_GAME;
            c.game.difficulty = 4;
            c.game.shot = 5;
            let f = gamemgr::F_REPLAY
                | gamemgr::F_SPELL_PRACTICE
                | if demo { gamemgr::F_DEMO } else { 0 };
            c.game.flags.set(f);
            let mut m = super::MainMenu::new();
            m.on_register(&mut c, Chain::Calc, 0).unwrap();
            let kept = if demo { 0 } else { gamemgr::F_REPLAY };
            assert_eq!(c.game.flags & 0xFFFF, kept);
            assert_eq!((m.screen, m.cursor, c.game.shot), (super::TOP, 1, 0));
            m.top(&mut c);
            let screen = if demo { super::TOP } else { super::REPLAY };
            assert_eq!((m.screen, m.prev_screen, m.frames), (screen, super::TOP, 0));
            assert_eq!(c.game.flags & 0xFFFF, 0);
        }
    }

    // 12 §2.2 (RE-Q55): +0x6C from flags bit 0 for any previous scene, bits 0/14 cleared, screen stays 0; the Top
    // entry block sets bit 14 again for +0x6C == 2 and switches to 15 (8 for plain practice).
    #[test]
    fn back_from_practice() {
        use crate::task::{Chain, Task};
        let Some(mut c) = ctx() else {
            return;
        };
        for spell in [false, true] {
            c.game.previous = gamemgr::SCENE_RESULT;
            let f = gamemgr::F_PRACTICE | if spell { gamemgr::F_SPELL_PRACTICE } else { 0 };
            c.game.flags.set(f);
            let mut m = super::MainMenu::new();
            m.on_register(&mut c, Chain::Calc, 0).unwrap();
            assert_eq!(
                (m.screen, m.back, m.cursor),
                (super::TOP, 1 + spell as u8, 3 - spell as i32)
            );
            assert_eq!(c.game.flags & 0xFFFF, 0);
            m.top(&mut c);
            let screen = if spell {
                super::CHAR_SPELL
            } else {
                super::DIFF_PRACTICE
            };
            assert_eq!((m.screen, m.prev_screen, m.frames), (screen, super::TOP, 0));
            assert_eq!(c.game.spell_practice(), spell);
        }
    }

    // 16 §4: notice order info00 (bit 15), info02 (16), info01 (17); only bits not yet in the flags.
    #[test]
    fn unlock_notices() {
        let all = 7 << 15;
        let v: Vec<_> = info_files(all, 0).collect();
        assert_eq!(v, ["info00.jpg", "info02.jpg", "info01.jpg"]);
        let v: Vec<_> = info_files(all, 1 << 15 | 1 << 3).collect();
        assert_eq!(v, ["info02.jpg", "info01.jpg"]);
        assert_eq!(info_files(1 << 16, 1 << 16).count(), 0);
        // Fade-in 253 … 23 (t 18), 10 (t 19), then gone; fade-out wraps below 5980 and rises to 242.
        assert_eq!(info_fade_alpha(0), Some(253));
        assert_eq!(info_fade_alpha(18), Some(23));
        assert_eq!(info_fade_alpha(19), Some(10));
        assert_eq!(info_fade_alpha(20), None);
        assert_eq!(info_fade_alpha(5940), None);
        assert_eq!(info_fade_alpha(5941), Some(15));
        assert_eq!(info_fade_alpha(5980), Some(0));
        assert_eq!(info_fade_alpha(5999), Some(242));
    }

    // 16 §4: the press iteration still advances t by one from its own value; the jump shows as 5981 one iteration
    // later. Only 179 < t < 5980 and a masked word unlike the frozen previous one count.
    #[test]
    fn info_loop_press() {
        use crate::input::{BOMB, ENTER, SHOT, UP};
        assert_eq!(info_step(179, SHOT, 0), (180, false));
        assert_eq!(info_step(180, SHOT, 0), (INFO_FADE_OUT + 1, true));
        assert_eq!(info_step(500, ENTER | UP, 0), (5981, true));
        assert_eq!(info_step(500, UP, 0), (501, false));
        // Held since the registering frame: equal to the frozen word, never a press.
        assert_eq!(info_step(500, SHOT, SHOT | UP), (501, false));
        assert_eq!(info_step(500, SHOT | BOMB, SHOT), (5981, true));
        assert_eq!(info_step(5979, SHOT, 0), (5981, true));
        assert_eq!(info_step(5980, SHOT, 0), (5981, false));
    }
    use crate::input::{Input, LEFT, RIGHT};

    // 15 §1.3: rows in screen order (Skip 4th); assigning swaps with every row holding the button.
    #[test]
    fn key_config_assign() {
        let mut k = crate::config::PAD_DEFAULT;
        assign_key(&mut k, 3, 0);
        assert_eq!(k, [3, 1, 2, 4, -1, -1, -1, -1, 0]);
        assign_key(&mut k, 5, 7);
        assert_eq!(k[4], 7);
        assign_key(&mut k, 6, 7);
        assert_eq!((k[4], k[5]), (-1, 7));
        assign_key(&mut k, 1, 1);
        assert_eq!(k, [3, 1, 2, 4, -1, 7, -1, -1, 0]);
    }

    #[test]
    fn volume_press_and_hold() {
        let mut i = Input::default();
        let mut v = 80;
        // One press steps 5 per edge: 80 → 75 → 70 → 65 (15 §1.3).
        for want in [75, 70, 65] {
            i.raw = LEFT;
            i.tick();
            v = volume_step(v, &i);
            assert_eq!(v, want);
            i.raw = 0;
            i.tick();
            assert_eq!(volume_step(v, &i), v);
        }
        // 98 → 100 on a Right edge; 2 → 0 on a Left edge.
        i.raw = RIGHT;
        i.tick();
        assert_eq!(volume_step(98, &i), 100);
        i.raw = 0;
        i.tick();
        i.raw = LEFT;
        i.tick();
        assert_eq!(volume_step(2, &i), 0);
        // Held: nothing until the counter passes 29, then −1 every frame.
        let mut v = 50;
        for n in 1..=35 {
            i.tick();
            let next = volume_step(v, &i);
            assert_eq!(next, if n < 30 { v } else { v - 1 }, "frame {n}");
            v = next;
        }
        assert_eq!(v, 44);
    }

    #[test]
    fn description_x() {
        // 12 §2.2: trunc(256 − len·15/4) over Shift-JIS bytes.
        assert_eq!(centred_x("abcd"), 241.0);
        assert_eq!(centred_x("あいう"), 233.0);
        assert_eq!(centred_x(""), 256.0);
    }

    #[test]
    fn title_music_after_result() {
        let mut g = Game::default();
        g.cfg.music = 1;
        g.previous = gamemgr::SCENE_GAME;
        assert!(title_music(&g));
        g.previous = gamemgr::SCENE_RESULT_MENU;
        assert!(!title_music(&g));
        g.previous = gamemgr::SCENE_BOOT;
        g.cfg.music = 0;
        assert!(!title_music(&g));
    }

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
// The top menu's > 3 → 2 store is dead; the difficulty entry's > 3 → 1 counts (12 §2.2 item 0, §2.3).
fn last_difficulty(g: &gamemgr::Game) -> i32 {
    match g.cfg.difficulty as i32 {
        d if d > 3 => 1,
        d => d,
    }
}
