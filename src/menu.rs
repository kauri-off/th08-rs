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

pub const ANM_SLOT: usize = 20;
pub const CALC_PRIO: u16 = 4;
pub const DRAW_PRIO: u16 = 3;
const VM_COUNT: usize = 142;
pub(crate) const CONFIRM: u16 = SHOT | ENTER;
pub(crate) const CANCEL: u16 = BOMB | PAUSE;

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

// Unlock checks (15 §2.4) on the score.dat clear masks; `TH08_UNLOCK=1` unlocks everything.
pub(crate) fn unlock_all() -> bool {
    crate::env::var("TH08_UNLOCK").is_some_and(|v| v != "0")
}

// 0x43C15F: a no-continue Final B clear on E–L.
fn team_final_b(g: &gamemgr::Game, t: usize) -> bool {
    unlock_all() || t >= 4 || g.clear_nc[t].iter().take(4).any(|m| m & 0x4000 != 0)
}

// 0x43C23B: a Final A clear on E–L.
pub(crate) fn team_final_a(g: &gamemgr::Game, t: usize) -> bool {
    unlock_all() || t >= 4 || g.clear_any[t].iter().take(4).any(|m| m & 0x8000 != 0)
}

fn extra_unlocked(g: &gamemgr::Game, shot: usize) -> bool {
    team_final_b(g, shot % 12)
}

fn extra_any(g: &gamemgr::Game) -> bool {
    (0..4).any(|t| team_final_b(g, t))
}

pub(crate) fn extra_all(g: &gamemgr::Game) -> bool {
    (0..4).all(|t| team_final_b(g, t))
}

fn spell_practice_unlocked(g: &gamemgr::Game) -> bool {
    (0..4).any(|t| team_final_a(g, t))
}

// Play count (score.dat); lives max 4 / 5 / 6 below 30 / 60 plays (answers_impl9 §16).
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

pub(crate) struct Pending {
    pub(crate) frames: u32,
    pub(crate) screen: i32,
    pub(crate) cursor: i32,
}

pub struct MainMenu {
    pub(crate) vms: Vec<AnmVm>,
    pub(crate) base: Vec<i32>,
    pub(crate) surface: Option<(TextureId, &'static str)>,
    pub(crate) raster: crate::text::Raster,
    pub(crate) desc_vm: AnmVm,
    pub(crate) desc_on: bool,
    pub(crate) desc_first: bool,
    pub(crate) notice: AnmVm,
    pub(crate) screen: i32,
    pub(crate) prev_screen: i32,
    pub(crate) frames: u32,
    pub(crate) cursor: i32,
    pub(crate) prev_cursor: i32,
    pub(crate) idle: u32,
    pub(crate) quitting: Option<u32>,
    pub(crate) pending: Option<Pending>,
    pub(crate) snapshot: Config,
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

    // Menu_SetScreen.
    pub(crate) fn set_screen(&mut self, s: i32) {
        self.prev_screen = self.screen;
        self.screen = s;
        self.frames = 0;
        self.prev_cursor = -1;
    }

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

    // Rasterises description `i` into the description VM's text.anm sprite rect, centred.
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
        // One description VM per item (sprite 21 + i, answers_impl9 §15); after (re-)entering the top menu it
        // is chosen from frame 9 (measured), except on the very first visit.
        if self.frames == 0 && !self.desc_first {
            self.desc_on = false;
        }
        if (self.frames >= 9 || self.desc_first) && self.cursor != self.prev_cursor {
            self.prev_cursor = self.cursor;
            self.desc_first = false;
            self.desc_on = true;
            ctx.anm.set_script(&mut self.desc_vm, 0, 9);
            self.desc_vm.set_sprite(&ctx.anm, 21 + self.cursor);
            self.desc_vm.pending_interrupt = 1;
            self.render_description(ctx, self.cursor as usize);
        }

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
        self.idle = if ctx.input.held != 0 {
            0
        } else {
            self.idle + 1
        };
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

    // Portrait VMs of character `c` (12 §2.4): shown 9, main 23, others 8.
    pub(crate) fn char_vms(&mut self, c: i32) {
        let row = CHAR_VMS[c.clamp(0, 11) as usize];
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
        }
        let allowed: Vec<bool> = (0..n).map(|c| self.char_allowed(ctx, c)).collect();
        if self.frames >= 8 {
            self.move_cursor(ctx, n, LEFT, RIGHT, |c| allowed[c as usize]);
        }
        if self.cursor != self.prev_cursor {
            self.prev_cursor = self.cursor;
            self.char_vms(self.cursor);
        }
        if self.frames < 8 {
            return;
        }
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
        let lmax = lives_max(&ctx.game);
        let cfg = &mut ctx.game.cfg;
        if dir != 0
            && let Some(v) = Self::option_value(cfg, item)
        {
            let old = *v;
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

    // SPEC-GAP: key config is a later task; the screen returns on any confirm/cancel.
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
        ctx.anm.load(
            &mut ctx.gfx,
            &ctx.archive,
            crate::menulist::RT_SLOT,
            "resulttext.anm",
        )?;
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
        // GM flags 15–17 from the clear records (16 §4). SPEC-GAP: the info00/01/02.jpg notices are not shown yet.
        {
            let g = &mut ctx.game;
            let nc_b = |t: usize| g.clear_nc[t].iter().take(4).any(|m| m & 0x4000 != 0);
            let any_a = |t: usize| g.clear_any[t].iter().take(4).any(|m| m & 0x8000 != 0);
            let bits = ((0..4).any(nc_b) as u32) << 15
                | ((0..4).any(any_a) as u32) << 16
                | ((0..4).all(nc_b) as u32) << 17;
            g.flags = g.flags & !(7 << 15) | bits;
        }
        if std::mem::take(&mut ctx.game.watched_replay) {
            self.screen = REPLAY;
        }
        let g = &ctx.game;
        if g.previous == gamemgr::SCENE_GAME || g.previous == gamemgr::SCENE_RETRY {
            if g.spell_practice() {
                // Back from a spell-practice game: straight to the card list (16 §6.1).
                self.screen = SPELL_CARD;
                self.lists.sp_row = if g.card >= 205 { 9 } else { g.stage as i32 % 9 };
            } else if g.practice() {
                self.screen = DIFF_PRACTICE;
            }
        }
        // SPEC-GAP: back from the result viewer / music room the cursor stays on that item (menu_base log).
        self.cursor = if self.screen == TOP {
            match ctx.game.previous {
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
        // The menu starts when its loader has finished (Supervisor loading screen, 12 §1).
        if ctx.game.menu_loading > 0 {
            return Ret::Next;
        }
        // Delayed switch: the new screen starts on the frame after the counter ran out (measured: press + 21).
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

    fn draw(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        if ctx.game.menu_loading > 0 {
            return Ret::Next;
        }
        ctx.gfx.set_viewport(FULLSCREEN);
        if let Some((t, _)) = self.surface {
            ctx.gfx.image(t, 0.0, 0.0);
        }
        for vm in &mut self.vms {
            ctx.anm.draw(&mut ctx.gfx, vm);
        }
        // Description VM: text.anm script 9 at (320, 448), centred (answers_impl9 §15).
        // SPEC-GAP: on spell practice the fading description stays drawn (mopup1 log), not on the other screens.
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
