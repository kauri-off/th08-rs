use crate::anmvm::{self, AnmVm};
use crate::ascii::Text;
use crate::config::Config;
use crate::game::Ctx;
use crate::gamemgr::{self, F_PRACTICE, F_REPLAY, F_SPELL_PRACTICE};
use crate::gfx::{Blend, FULLSCREEN, Rect, TextureId};
use crate::input::{BOMB, DOWN, ENTER, LEFT, PAUSE, RIGHT, SHOT, UP};
use crate::sound;
use crate::task::{Chain, Ret, Task};
use anyhow::Result;

pub const ANM_SLOT: usize = 20;
pub const CALC_PRIO: u16 = 4;
pub const DRAW_PRIO: u16 = 3;
const VM_COUNT: usize = 142;
const CONFIRM: u16 = SHOT | ENTER;
const CANCEL: u16 = BOMB | PAUSE;

const TOP: i32 = 0;
const OPTION: i32 = 2;
const KEY_CONFIG: i32 = 3;
const DIFF: i32 = 4;
const DIFF_PRACTICE: i32 = 8;
const DIFF_EXTRA: i32 = 12;
const CHAR: i32 = 5;
const CHAR_PRACTICE: i32 = 9;
const CHAR_EXTRA: i32 = 13;
const REPLAY: i32 = 7;
const PRACTICE_STAGE: i32 = 11;
const SPELL_PRACTICE: i32 = 16;

const ITEMS: usize = 9;
const ITEM_QUIT: i32 = 8;
const VM_DIFF: usize = 131;
const VM_DIFF_EXTRA: usize = 135;
const VM_OPTION: usize = 10;
const OPTION_ITEMS: i32 = 10;

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
const CHAR_VMS: [[i32; 4]; 12] = [
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

// Final-route notice sprite for the cursor team (MainMenu_CharacterDraw, 12 §2.4).
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

const STAGE_NAMES: [&str; 8] = [
    "Stage 1", "Stage 2", "Stage 3", "Stage 4A", "Stage 4B", "Stage 5", "Stage 6A", "Stage 6B",
];

// Unlock checks (15 §2.4) on the session's clear masks. SPEC-GAP: score.dat is not loaded yet; `TH08_UNLOCK=1` unlocks everything.
fn unlock_all() -> bool {
    crate::env::var("TH08_UNLOCK").is_some_and(|v| v != "0")
}

// 0x43C15F: a no-continue Final B clear on E–L.
fn team_final_b(g: &gamemgr::Game, t: usize) -> bool {
    unlock_all() || t >= 4 || g.clear_nc[t].iter().take(4).any(|m| m & 0x4000 != 0)
}

// 0x43C23B: a Final A clear on E–L.
fn team_final_a(g: &gamemgr::Game, t: usize) -> bool {
    unlock_all() || t >= 4 || g.clear_any[t].iter().take(4).any(|m| m & 0x8000 != 0)
}

fn extra_unlocked(g: &gamemgr::Game, shot: usize) -> bool {
    team_final_b(g, shot % 12)
}

fn extra_any(g: &gamemgr::Game) -> bool {
    (0..4).any(|t| team_final_b(g, t))
}

fn extra_all(g: &gamemgr::Game) -> bool {
    (0..4).all(|t| team_final_b(g, t))
}

fn spell_practice_unlocked(g: &gamemgr::Game) -> bool {
    (0..4).any(|t| team_final_a(g, t))
}

// Play count (score.dat); lives max 4 / 5 / 6 below 30 / 60 plays (answers_impl9 §16).
fn play_count() -> u32 {
    if unlock_all() { 60 } else { 0 }
}

fn lives_max() -> i32 {
    match play_count() {
        ..30 => 4,
        30..60 => 5,
        _ => 6,
    }
}

struct Pending {
    frames: u32,
    screen: i32,
    cursor: i32,
}

pub struct MainMenu {
    vms: Vec<AnmVm>,
    base: Vec<i32>,
    surface: Option<(TextureId, &'static str)>,
    raster: crate::text::Raster,
    desc: Vec<TextureId>,
    desc_vm: AnmVm,
    notice: AnmVm,
    screen: i32,
    prev_screen: i32,
    frames: u32,
    cursor: i32,
    prev_cursor: i32,
    idle: u32,
    quitting: Option<u32>,
    pending: Option<Pending>,
    snapshot: Config,
    stages: [bool; 8],
}

impl MainMenu {
    pub fn new() -> Self {
        Self {
            vms: Vec::new(),
            base: Vec::new(),
            surface: None,
            raster: Default::default(),
            desc: Vec::new(),
            desc_vm: AnmVm::default(),
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
        }
    }

    // Menu_SetScreen.
    fn set_screen(&mut self, s: i32) {
        self.prev_screen = self.screen;
        self.screen = s;
        self.frames = 0;
        self.prev_cursor = -1;
    }

    fn interrupt_all(&mut self, id: i32) {
        for vm in &mut self.vms {
            vm.pending_interrupt = id as _;
        }
    }

    fn set_surface(&mut self, ctx: &mut Ctx, name: &'static str) {
        if self.surface.is_some_and(|(_, n)| n == name) {
            return;
        }
        if let Some((t, _)) = self.surface.take() {
            ctx.gfx.free_texture(t);
        }
        match ctx.archive.read(name).and_then(|b| ctx.gfx.load_image(&b)) {
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

    fn render_description(&mut self, ctx: &mut Ctx, i: usize) {
        const H: usize = 16;
        let w = if self.desc_vm.sprite_w > 0.0 {
            self.desc_vm.sprite_w as usize
        } else {
            384
        };
        let mut rgba = self
            .raster
            .render(DESCRIPTIONS[i], [0xFF, 0xF0, 0xE0], w, H);
        let width = (0..w)
            .rev()
            .find(|&x| (0..H).any(|y| rgba[(y * w + x) * 4 + 3] != 0))
            .map_or(0, |x| x + 1);
        let shift = (w - width) / 2;
        if shift > 0 {
            for row in rgba.chunks_exact_mut(w * 4) {
                row.copy_within(0..width * 4, shift * 4);
                row[..shift * 4].fill(0);
            }
        }
        self.desc
            .push(ctx.gfx.create_texture(w as u32, H as u32, &rgba));
    }

    fn move_cursor(
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

    fn item_locked(ctx: &Ctx, i: usize) -> bool {
        match i {
            1 => !extra_any(&ctx.game),
            2 => !spell_practice_unlocked(&ctx.game),
            _ => false,
        }
    }

    fn top(&mut self, ctx: &mut Ctx) {
        if self.frames == 0 {
            self.set_surface(ctx, "title00.png");
            self.interrupt_all(2);
        }
        if self.desc_vm.script.is_none() {
            ctx.anm.set_script(&mut self.desc_vm, 0, 9);
        }
        if self.desc.len() < ITEMS {
            let i = self.desc.len();
            self.render_description(ctx, i);
        }
        for i in 0..ITEMS {
            let on = i as i32 == self.cursor;
            self.highlight(ctx, 1 + i, on);
            let grey = if Self::item_locked(ctx, i) {
                0x40
            } else {
                0xFF
            };
            self.vms[1 + i].color[..3].fill(grey);
        }
        if self.cursor != self.prev_cursor {
            self.prev_cursor = self.cursor;
            self.desc_vm.pending_interrupt = 1;
        }
        self.desc_vm.run(&mut ctx.anm);
        if let Some(f) = &mut self.quitting {
            *f += 1;
            if *f >= 60 {
                ctx.game.request(-1);
            }
            return;
        }
        if self.frames < 10 || self.pending.is_some() {
            return;
        }
        // SPEC-GAP: the attract demo (1500 idle frames → demo replay) needs replay playback; the counter only runs.
        self.idle = if ctx.input.held != 0 {
            0
        } else {
            self.idle + 1
        };
        let locked: Vec<bool> = (0..ITEMS).map(|i| Self::item_locked(ctx, i)).collect();
        self.move_cursor(ctx, ITEMS as i32, UP, DOWN, |c| !locked[c as usize]);
        if ctx.input.pressed(CONFIRM) {
            sound::play(sound::OK);
            let g = &mut ctx.game;
            match self.cursor {
                0 => {
                    g.flags &= !(F_PRACTICE | F_SPELL_PRACTICE);
                    self.interrupt_all(5);
                    self.set_screen(DIFF);
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
                    ctx.audio.fade_bgm(0.5);
                    self.quitting = Some(0);
                }
            }
        } else if ctx.input.pressed(CANCEL) && self.cursor != ITEM_QUIT {
            self.cursor = ITEM_QUIT;
            sound::play(sound::CANCEL);
        }
    }

    fn difficulty(&mut self, ctx: &mut Ctx) {
        let extra = self.screen == DIFF_EXTRA;
        if self.frames == 0 {
            if !matches!(self.prev_screen, CHAR | CHAR_PRACTICE | CHAR_EXTRA) {
                self.set_surface(ctx, "select00.png");
            }
            self.interrupt_all(if extra { 12 } else { 7 });
            let d = ctx.game.cfg.difficulty as i32;
            self.cursor = if extra {
                0
            } else if d > 3 {
                1
            } else {
                d
            };
        }
        if extra {
            self.highlight(ctx, VM_DIFF_EXTRA, true);
        } else {
            for i in 0..4 {
                self.highlight(ctx, VM_DIFF + i, i as i32 == self.cursor);
            }
        }
        if self.frames < 8 || self.pending.is_some() {
            return;
        }
        if !extra {
            self.move_cursor(ctx, 4, UP, DOWN, |_| true);
            if self.cursor != self.prev_cursor {
                self.prev_cursor = self.cursor;
                for i in 0..4 {
                    self.vms[VM_DIFF + i].pending_interrupt =
                        if i as i32 == self.cursor { 24 } else { 25 };
                }
            }
        }
        let store =
            |g: &mut gamemgr::Game, c: i32| g.cfg.difficulty = if extra { 4 + c } else { c } as u8;
        if ctx.input.pressed(CONFIRM) {
            store(&mut ctx.game, self.cursor);
            sound::play(sound::OK);
            self.set_screen(self.screen + 1);
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

    fn char_allowed(&self, ctx: &Ctx, c: i32) -> bool {
        self.screen != CHAR_EXTRA || extra_unlocked(&ctx.game, c as usize)
    }

    fn character(&mut self, ctx: &mut Ctx) {
        let n = if extra_all(&ctx.game) { 12 } else { 4 };
        if self.frames == 0 {
            self.interrupt_all(8);
            // Difficulty plate slides to (16, 384) (12 §2.4).
            self.vms[VM_DIFF + (ctx.game.cfg.difficulty as usize).min(4)].pending_interrupt = 9;
            self.cursor = (ctx.game.shot as i32).min(n - 1);
            if !self.char_allowed(ctx, self.cursor) {
                self.cursor = (0..n).find(|&c| self.char_allowed(ctx, c)).unwrap_or(0);
            }
            return;
        }
        if self.cursor != self.prev_cursor {
            self.prev_cursor = self.cursor;
            let row = CHAR_VMS[self.cursor as usize];
            for vm in 111..=130 {
                let v = vm as i32;
                self.vms[vm].pending_interrupt = if row[..3].contains(&v) {
                    9
                } else if row[3] == v {
                    23
                } else {
                    8
                };
            }
        }
        if self.frames < 8 {
            return;
        }
        let allowed: Vec<bool> = (0..n).map(|c| self.char_allowed(ctx, c)).collect();
        self.move_cursor(ctx, n, LEFT, RIGHT, |c| allowed[c as usize]);
        if ctx.input.pressed(CONFIRM) {
            sound::play(sound::OK);
            let g = &mut ctx.game;
            g.shot = self.cursor as u8;
            match self.screen {
                CHAR_PRACTICE => self.set_screen(PRACTICE_STAGE),
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
            sound::play(sound::CANCEL);
            self.interrupt_all(6);
            self.set_screen(self.screen - 1);
        }
    }

    // Practice stage select (12 §2.5). SPEC-GAP: the list's drawing is unspecified; ASCII text is used.
    fn practice_stage(&mut self, ctx: &mut Ctx) {
        let g = &ctx.game;
        if self.frames == 0 {
            self.interrupt_all(18);
            let mask = g.clear_any[g.shot as usize % 12][g.cfg.difficulty.min(3) as usize];
            self.stages = std::array::from_fn(|i| unlock_all() || i == 0 || mask & (1 << i) != 0);
            if mask & (1 << 7) != 0 {
                self.stages[3] = true;
                self.stages[4] = true;
            }
            if !self.stages[self.cursor.clamp(0, 7) as usize] {
                self.cursor = 0;
            }
        }
        for (i, name) in STAGE_NAMES.iter().enumerate() {
            let c = if i as i32 == self.cursor {
                0xFFFFFFFF
            } else if self.stages[i] {
                0xFF808080
            } else {
                0xFF303030
            };
            ctx.ascii
                .push(Text::new(400.0, 160.0 + 20.0 * i as f32, *name).argb(c));
        }
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
            self.set_screen(CHAR_PRACTICE);
        }
    }

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

    // Option screen (12 §2.7).
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
        let cfg = &mut ctx.game.cfg;
        if dir != 0
            && let Some(v) = Self::option_value(cfg, item)
        {
            let old = *v;
            let (max, step) = match item {
                0 => (lives_max(), 1),
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
            sound::play(sound::CANCEL);
            if self.cursor == OPTION_ITEMS - 1 {
                self.set_screen(TOP);
                self.cursor = 7;
            } else {
                self.cursor = OPTION_ITEMS - 1;
            }
        }
    }

    // Value VMs 20–43 (answers_impl9 §16): the lit VM is first + value; volume digits are base + digit·2.
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
        let plays = play_count();
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

    // SPEC-GAP: replay, spell practice and key config are later tasks; these screens return on any confirm/cancel.
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
    fn on_register(&mut self, ctx: &mut Ctx, chain: Chain, _: u8) -> Result<()> {
        if chain != Chain::Calc {
            return Ok(());
        }
        if ctx.game.cfg.music != 0 {
            ctx.audio.play_bgm("bgm/th08_01.mid");
        }
        ctx.anm
            .load(&mut ctx.gfx, &ctx.archive, ANM_SLOT, "title01.anm")?;
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
        let g = &ctx.game;
        if g.previous == gamemgr::SCENE_GAME || g.previous == gamemgr::SCENE_RETRY {
            if g.spell_practice() {
                self.screen = SPELL_PRACTICE;
            } else if g.practice() {
                self.screen = DIFF_PRACTICE;
            }
        }
        self.cursor = if self.screen == TOP {
            0
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
        for t in self.desc.drain(..) {
            ctx.gfx.free_texture(t);
        }
        ctx.anm.unload(&mut ctx.gfx, ANM_SLOT);
    }

    fn calc(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        if let Some(p) = &mut self.pending {
            p.frames = p.frames.saturating_sub(1);
            if p.frames == 0 {
                let (s, c) = (p.screen, p.cursor);
                self.pending = None;
                self.set_screen(s);
                self.cursor = c;
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
            REPLAY => self.stub(ctx, "Replay", TOP, 4),
            SPELL_PRACTICE => self.stub(ctx, "Spell practice", TOP, 2),
            _ => {}
        }
        if self.screen == screen {
            self.frames += 1;
        }
        for vm in self.vms.iter_mut().filter(|v| v.script.is_some()) {
            vm.run(&mut ctx.anm);
        }
        Ret::Next
    }

    fn draw(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        ctx.gfx.set_viewport(FULLSCREEN);
        if let Some((t, _)) = self.surface {
            ctx.gfx.image(t, 0.0, 0.0);
        }
        for vm in &mut self.vms {
            ctx.anm.draw(&mut ctx.gfx, vm);
        }
        // Description VM: text.anm script 9 at (320, 448), centred (answers_impl9 §15).
        let vm = &self.desc_vm;
        if self.screen == TOP
            && self.quitting.is_none()
            && self.pending.is_none()
            && vm.flags & anmvm::F_VISIBLE != 0
        {
            if let Some(&t) = self.desc.get(self.cursor as usize) {
                let (w, h) = (
                    if vm.sprite_w > 0.0 {
                        vm.sprite_w
                    } else {
                        384.0
                    },
                    16.0,
                );
                let r = Rect::new(vm.pos[0] - w / 2.0, vm.pos[1] - h / 2.0, w, h);
                ctx.gfx.sprite(
                    t,
                    Blend::Alpha,
                    r,
                    [0.0, 0.0, 1.0, 1.0],
                    [255, 255, 255, vm.color[anmvm::A]],
                );
            }
        }
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
