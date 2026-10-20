//! Result object `Result_Register(mode)` 0x4582A0 (16 §2): name entry, ranking and end panel after a game (mode 1),
//! result viewer from the main menu (mode 0). score.dat is saved when it is removed (15 §2.2).

use crate::anmvm::{self, AnmVm};
use crate::ascii::Text;
use crate::game::Ctx;
use crate::gamemgr::{self, F_ENDING_OK};
use crate::gfx::{FULLSCREEN, TextureId};
use crate::input::{BOMB, DOWN, ENTER, LEFT, PAUSE, RIGHT, SHOT, UP};
use crate::menu::{CANCEL, CONFIRM};
use crate::scoredat::{C_ATTEMPTS, C_BONUS, C_CAPTURES, C_PRACTICE, CARDS, Hscr};
use crate::sound;
use crate::task::{Chain, Ret, Task};

// Result_Calc 0x4584B0 at prio 16, Result_Draw 0x4586B4 at prio 18 (16 §2).
pub const CALC_PRIO: u16 = 16;
pub const DRAW_PRIO: u16 = 18;
// result00.anm in slot 0x15, resulttext.anm in 0x16 (16 §2).
const ANM_SLOT: usize = 21;
const RT_SLOT: usize = 22;
const VM_COUNT: usize = 72;
// Invisible anchor of the ranking / card list (16 §2.3).
const VM_ANCHOR: usize = 40;
// End-panel picture result03.png (16 §2.5).
const VM_PANEL: usize = 71;

// Result states (+8, 16 §2.1 / §2.4 / §2.6).
const S_INIT: i32 = 0;
const S_TOP: i32 = 1;
const S_LEAVE: i32 = 2;
const S_DIFF: i32 = 3;
const S_CHAR: i32 = 4;
const S_RANK: i32 = 5;
const S_GROUP: i32 = 6;
const S_CARD_CHAR: i32 = 7;
const S_CARDS: i32 = 8;
const S_NAME: i32 = 9;
const S_SAVE: i32 = 10;
const S_PANEL: i32 = 0xF;
const S_PANEL_OUT: i32 = 0x10;
const S_PRACTICE: i32 = 0x11;
const S_PLAY: i32 = 0x13;
const S_PLAY_WAIT: i32 = 0x14;
const S_PLAY_OUT: i32 = 0x15;
const S_SPELL: i32 = 0x16;

// Name-entry charset 0x4B7340, 6 rows × 16 (16 §2.2). 0x5D is the only unselectable cell; 0x5E / 0x5F are
// drawn as the "SP" / "END" glyphs 0x80 / 0x81.
const CHARSET: &[u8; 96] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZ.,:;_@abcdefghijklmnopqrstuvwxyz+-/*=%0123456789#!?'\"$(){}[]<>&\\|~^ --";
const CELL_SP: i32 = 0x5E;
const CELL_END: i32 = 0x5F;
// Difficulty names 0x4C7FFC (end panel, right-aligned to 10).
const DIFF_PANEL: [&str; 5] = [
    "      Easy",
    "    Normal",
    "      Hard",
    "   Lunatic",
    "     Extra",
];
// Last-attempt difficulty letter of a card (CATK +0x0F, 16 §2.4 card list).
const DIFF_LETTER: [char; 5] = ['E', 'N', 'H', 'L', '-'];
const UNKNOWN5: &str = "？？？？？";

// Ranking stage map, table 0x4C7F80 (16 §2.3): stage byte 0..8 → shown number (4A/4B → 4, 6A/6B → 6, Extra → 1).
const STAGE_SHOWN: [u8; 9] = [1, 2, 3, 4, 4, 5, 6, 6, 1];

fn stage_shown(b: u8) -> u8 {
    // The game writes only 0..8 or 99 (15 §2.3); other bytes from a foreign file are shown raw instead of indexing past the table.
    STAGE_SHOWN.get(b as usize).copied().unwrap_or(b)
}

// Card ids of result-viewer group g (16 §2.4; spell_tables.md §1): 0–4 = Spell_DifficultyOf, 5 = all 222.
fn group_cards(g: i32) -> Vec<usize> {
    (0..CARDS)
        .filter(|&id| g >= 5 || (id < 205 && crate::menulist::difficulty_of(id) == g as usize))
        .collect()
}

// The 0x477B0-byte result object (16 §2).
pub struct ResultScreen {
    // Result_Register mode: 0 viewer, 1 after a game (2 = flush only, handled by Game::save_score).
    mode: u8,
    state: i32,
    // Result +4 frame counter, +0x10 sub-state, +0x18 sub-frame, +0x1C cursor (16 §2.4).
    frame: i32,
    sub: i32,
    sub_frame: i32,
    cursor: i32,
    // +0x30 card-list character (12 = all), +0x34 character the list was built for, +0x38 accordion.
    card_char: i32,
    card_built_char: i32,
    accordion: bool,
    // +0x3C ranking shot, +0x40 card page built, +0x44 ranking difficulty, +0x48 card group, +0x54 leaving.
    rank_shot: i32,
    card_page: i32,
    rank_diff: i32,
    card_group: i32,
    card_leaving: bool,
    // Name entry: name being typed (+0x58), write position, charset cursor.
    name: [u8; 8],
    name_pos: i32,
    char_cursor: i32,
    // 72 result00.anm VMs at +0x1A0, VM i runs script i.
    vms: Vec<AnmVm>,
    // 28 Text_RenderToVm targets: k 0–13 text.anm sprites 21–34, k 14–27 resulttext.anm sprites 2–15.
    text_vms: Vec<AnmVm>,
    // VM +0x1119C: result00.anm sprite 32, the 256×16 separator bar (centred anchor).
    bar: AnmVm,
    raster: crate::text::Raster,
    // Surface slot 0: result/result.jpg.
    surface: Option<TextureId>,
    // +0x64: captured-card counts [group][character] (Result_OnRegister).
    captured: [[u32; 13]; 6],
    // Run-time seconds kept at Result+0x19C (play-data page, 16 §2.4).
    run_secs: u32,
}

// AnmVm_Init + sprite, top-left anchor unless centred, white (16 §2.4 text VMs).
fn text_vm(ctx: &Ctx, slot: usize, sprite: i32, centred: bool) -> AnmVm {
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

// SPEC-GAP: initial alpha of the 28 text VMs is not given; 0 until a page renders them (alpha 255).
fn text_vm_hidden(ctx: &Ctx, slot: usize, sprite: i32) -> AnmVm {
    let mut vm = text_vm(ctx, slot, sprite, false);
    vm.color[anmvm::A] = 0;
    vm
}

fn draw_at(ctx: &mut Ctx, vm: &mut AnmVm, x: f32, y: f32) {
    vm.pos = [x, y, 0.0];
    ctx.anm.draw_mode(&mut ctx.gfx, vm, 0);
}

fn vm_argb(vm: &AnmVm) -> u32 {
    let c = vm.color;
    (c[anmvm::A] as u32) << 24
        | (c[anmvm::R] as u32) << 16
        | (c[anmvm::G] as u32) << 8
        | c[anmvm::B] as u32
}

fn put(ctx: &mut Ctx, x: f32, y: f32, s: impl Into<String>, argb: u32) {
    ctx.ascii.push(Text::new(x, y, s).argb(argb));
}

impl ResultScreen {
    // Mode 0: viewer (scene 5); mode 1: after a game (scene 6).
    pub fn new(mode: u8) -> Self {
        Self {
            mode,
            state: S_INIT,
            frame: 0,
            sub: 0,
            sub_frame: 0,
            cursor: 0,
            card_char: 12,
            card_built_char: 12,
            accordion: false,
            rank_shot: 0,
            card_page: -1,
            rank_diff: 1,
            card_group: 5,
            card_leaving: false,
            name: [b' '; 8],
            name_pos: 0,
            char_cursor: 0,
            vms: Vec::new(),
            text_vms: Vec::new(),
            bar: AnmVm::default(),
            raster: Default::default(),
            surface: None,
            captured: [[0; 13]; 6],
            run_secs: 0,
        }
    }

    // Result_SetState 0x4550B7: the frame and sub counters restart (the original also stores prev = state).
    fn set_state(&mut self, s: i32) {
        self.state = s;
        self.frame = 0;
        self.sub = 0;
        self.sub_frame = 0;
    }

    fn interrupt_all(&mut self, id: u16) {
        for vm in &mut self.vms {
            vm.pending_interrupt = id;
        }
    }

    // Text_RenderToVm into text VM k (white, outline 0, font 15) and alpha 255.
    fn render(&mut self, ctx: &mut Ctx, k: usize, s: &str) {
        let (slot, sprite) = if k < 14 {
            (0, 21 + k as i32)
        } else {
            (RT_SLOT, 2 + k as i32 - 14)
        };
        crate::menulist::render_strip(ctx, &mut self.raster, slot, sprite, s);
        self.text_vms[k].color[anmvm::A] = 255;
    }

    // Menu page pattern of states 1, 3, 4, 6, 7 (16 §2.4). Returns Some(true) on confirm, Some(false) on cancel.
    fn page(
        &mut self,
        ctx: &mut Ctx,
        first: usize,
        n: i32,
        remembered: i32,
        wait: i32,
    ) -> Option<bool> {
        if self.sub == 0 {
            if self.sub_frame == 0 {
                self.cursor = remembered.clamp(0, n - 1);
                for i in 0..n {
                    let vm = &mut self.vms[first + i as usize];
                    vm.pending_interrupt = 22;
                    vm.run(&mut ctx.anm);
                    vm.pending_interrupt = if i == self.cursor { 20 } else { 21 };
                }
            }
            self.sub_frame += 1;
            if self.sub_frame >= wait {
                self.sub = 1;
            }
            return None;
        }
        let inp = ctx.input;
        // Result_CursorUpDown 0x45A0F4: edge or repeat, wraps.
        let d = if inp.pressed_or_repeat(UP) {
            -1
        } else if inp.pressed_or_repeat(DOWN) {
            1
        } else {
            0
        };
        if d != 0 {
            self.cursor = (self.cursor + d).rem_euclid(n);
            sound::play(sound::SELECT);
            self.highlight(first, n);
        }
        if inp.pressed(CONFIRM) {
            for i in 0..n {
                self.vms[first + i as usize].pending_interrupt =
                    if i == self.cursor { 23 } else { 1 };
            }
            return Some(true);
        }
        if inp.pressed(CANCEL) {
            sound::play(sound::CANCEL);
            return Some(false);
        }
        None
    }

    fn highlight(&mut self, first: usize, n: i32) {
        for i in 0..n {
            self.vms[first + i as usize].pending_interrupt = if i == self.cursor { 20 } else { 21 };
        }
    }

    fn calc_state(&mut self, ctx: &mut Ctx) {
        match self.state {
            // Mode 1 (16 §2.1): name entry, or 0x11 / 0x16 for stage / spell practice.
            S_INIT if self.mode == 1 => {
                let g = &ctx.game;
                let s = if g.spell_practice() {
                    S_SPELL
                } else if g.practice() {
                    S_PRACTICE
                } else {
                    S_NAME
                };
                self.state = s;
                self.calc_state(ctx);
            }
            S_INIT => {
                // State 0 continues as state 1 in the same frame (16 §2.4 play data exit).
                self.set_state(S_TOP);
                self.calc_state(ctx);
            }
            S_TOP => self.top(ctx),
            S_LEAVE => {
                // Exit (16 §2.1 state 2, §2.4 exit): frame 1 Loading_Begin + interrupt 2, scene 1 after 20.
                if self.frame == 1 {
                    ctx.game.loading_req = true;
                    self.interrupt_all(2);
                }
                if self.frame > 19 {
                    ctx.game.request(gamemgr::SCENE_MENU);
                }
            }
            // State 3, Result_CheatUnlock 0x4550FC (16 §2.4).
            // SPEC-GAP: the hidden unlock code of this state (15 §2.12) is not implemented.
            S_DIFF => match self.page(ctx, 4, 5, self.rank_diff, 6) {
                Some(true) => {
                    sound::play(sound::OK);
                    self.rank_diff = self.cursor;
                    self.set_state(S_CHAR);
                    self.cursor = 0;
                }
                Some(false) => {
                    self.rank_diff = self.cursor;
                    self.set_state(S_TOP);
                    self.cursor = 0;
                }
                None => {}
            },
            // State 4 (0x45567D).
            S_CHAR => match self.page(ctx, 15, 12, self.rank_shot, 6) {
                Some(true) => {
                    sound::play(sound::OK);
                    self.vms[VM_ANCHOR].pending_interrupt = 3;
                    self.rank_shot = -1;
                    self.set_state(S_RANK);
                }
                Some(false) => {
                    self.rank_shot = self.cursor;
                    for vm in &mut self.vms[15..27] {
                        vm.pending_interrupt = 1;
                    }
                    self.set_state(S_DIFF);
                }
                None => {}
            },
            S_RANK => self.ranking(ctx),
            // State 6 (0x455A33).
            S_GROUP => match self.page(ctx, 9, 6, self.card_group, 6) {
                Some(true) => {
                    sound::play(sound::OK);
                    self.card_group = self.cursor;
                    self.set_state(S_CARD_CHAR);
                }
                Some(false) => {
                    self.card_group = self.cursor;
                    self.set_state(S_TOP);
                    self.cursor = 1;
                }
                None => {}
            },
            // State 7 (0x455CB0).
            S_CARD_CHAR => match self.page(ctx, 27, 13, self.card_char, 6) {
                Some(true) => {
                    sound::play(sound::OK);
                    self.card_char = self.cursor;
                    self.set_state(S_CARDS);
                    self.vms[VM_ANCHOR].pending_interrupt = 3;
                    self.cursor = 0;
                    self.card_page = -1;
                }
                Some(false) => {
                    self.card_char = self.cursor;
                    for vm in &mut self.vms[27..40] {
                        vm.pending_interrupt = 1;
                    }
                    self.set_state(S_GROUP);
                }
                None => {}
            },
            S_CARDS => self.cards(ctx),
            S_NAME => self.name_entry(ctx),
            // State 0xF: end banner; after 90 frames Shot/Enter → interrupt 2, state 0x10 (16 §2.1).
            S_PANEL => {
                if self.frame >= 90 && ctx.input.pressed(SHOT | ENTER) {
                    self.vms[VM_PANEL].pending_interrupt = 2;
                    self.set_state(S_PANEL_OUT);
                }
            }
            // State 0x10: after 30 frames → state 10 with frame counter 9 (16 §2.1).
            S_PANEL_OUT => {
                if self.frame >= 30 {
                    self.set_state(S_SAVE);
                    self.frame = 9;
                }
            }
            S_SAVE => {
                // SPEC-GAP: the replay-save dialogue (16 §2.6, states 10–14) is not implemented; straight to the exit.
                self.set_state(S_LEAVE);
                self.calc_state(ctx);
            }
            S_PRACTICE | S_SPELL => {
                let g = &mut ctx.game;
                // Slow mode drops this run's card credit (15 §2.5, modes 0x11 / 0x16).
                if g.cfg.slow != 0 {
                    g.restore_catk_backup();
                }
                if self.state == S_PRACTICE {
                    // 15 §2.6: practice hiscore [shot + 0x164D0B2][stage][difficulty] := max.
                    let shot = (g.shot as usize + g.demo_shot_bias as usize) % 12;
                    let d = g.difficulty.clamp(0, 4) as usize;
                    let score = g.stats.score10.clamp(0, 999_999_999) as u32;
                    let h = &mut g.score.pscr[shot].hiscore[g.stage % 9][d];
                    *h = (*h).max(score);
                }
                self.set_state(S_SAVE);
            }
            S_PLAY | S_PLAY_WAIT | S_PLAY_OUT => self.play_data(ctx),
            _ => {}
        }
    }

    // State 1, Result_TopMenu 0x454CB2 (16 §2.4): all 72 VMs get interrupt 1 on entry, then the page pattern (N = 20).
    // SPEC-GAP: the D key (score.txt dump, 16 §2.7) is not implemented.
    fn top(&mut self, ctx: &mut Ctx) {
        if self.sub == 0 && self.sub_frame == 0 {
            self.interrupt_all(1);
        }
        let c = self.cursor;
        match self.page(ctx, 0, 4, c, 20) {
            Some(true) => match self.cursor {
                0 => {
                    sound::play(sound::OK);
                    self.set_state(S_DIFF);
                }
                1 => {
                    sound::play(sound::OK);
                    self.set_state(S_GROUP);
                }
                2 => {
                    sound::play(sound::OK);
                    self.set_state(S_PLAY);
                }
                _ => {
                    sound::play(sound::CANCEL);
                    self.set_state(S_LEAVE);
                }
            },
            Some(false) => {
                if self.cursor == 3 {
                    self.set_state(S_LEAVE);
                } else {
                    self.cursor = 3;
                    self.highlight(0, 4);
                }
            }
            None => {}
        }
    }

    // State 5 (0x455925, 16 §2.4): the shot is applied at frame 10, while VM 40 is off-screen.
    fn ranking(&mut self, ctx: &mut Ctx) {
        if self.frame == 10 {
            self.rank_shot = self.cursor;
        }
        if self.frame < 6 {
            return;
        }
        let inp = ctx.input;
        // Result_CursorLeftRight 0x45A2F2.
        let d = if inp.pressed_or_repeat(LEFT) {
            -1
        } else if inp.pressed_or_repeat(RIGHT) {
            1
        } else {
            0
        };
        if d != 0 {
            let old = self.cursor;
            self.cursor = (self.cursor + d).rem_euclid(12);
            sound::play(sound::SELECT);
            self.frame = 0;
            self.vms[VM_ANCHOR].pending_interrupt = (3 + self.rank_diff) as u16;
            self.vms[15 + old as usize].pending_interrupt = 24;
            self.vms[15 + self.cursor as usize].pending_interrupt = 25;
        }
        if inp.pressed(CANCEL) {
            self.rank_shot = self.cursor;
            self.vms[VM_ANCHOR].pending_interrupt = 1;
            sound::play(sound::CANCEL);
            self.set_state(S_CHAR);
        }
    }

    // State 8 (0x455F6B): card list of group +0x48 for character +0x30, 10 cards per page (+0x1C).
    fn cards(&mut self, ctx: &mut Ctx) {
        let ids = group_cards(self.card_group);
        let n = ids.len() as i32;
        if self.card_leaving && self.frame > 9 {
            self.card_leaving = false;
            self.set_state(S_CARD_CHAR);
            return;
        }
        if self.frame == 10
            && (self.cursor != self.card_page || self.card_char != self.card_built_char)
        {
            self.card_page = self.cursor;
            self.card_built_char = self.card_char;
            for k in 0..10 {
                let idx = (self.cursor * 10 + k) as usize;
                let Some(&id) = ids.get(idx) else { break };
                let c = &ctx.game.score.catk[id];
                let s = if c.counters[C_ATTEMPTS][12] != 0 {
                    crate::text::decode_sjis(
                        &c.name[..c.name.iter().position(|&b| b == 0).unwrap_or(48)],
                    )
                } else {
                    UNKNOWN5.to_string()
                };
                self.render(ctx, k as usize, &s);
            }
            let s = format!(
                "{:3}/{:3}（キャラ切り替え↓↑）",
                self.captured[self.card_group as usize][self.card_built_char as usize], n
            );
            self.render(ctx, 10, &s);
        }
        if self.frame < 6 || self.card_leaving {
            return;
        }
        let inp = ctx.input;
        let lr = if inp.pressed_or_repeat(LEFT) {
            -1
        } else if inp.pressed_or_repeat(RIGHT) {
            1
        } else {
            0
        };
        if lr != 0 {
            self.cursor = (self.cursor + lr).rem_euclid((n + 9) / 10);
            sound::play(sound::SELECT);
            self.frame = 0;
            self.vms[VM_ANCHOR].pending_interrupt = 10;
        } else {
            // Result_CursorUpDown2 0x45A1F3 on +0x30. SPEC-GAP: SE 12 assumed as for the other cursors.
            let ud = if inp.pressed_or_repeat(UP) {
                -1
            } else if inp.pressed_or_repeat(DOWN) {
                1
            } else {
                0
            };
            if ud != 0 {
                let old = self.card_char;
                self.card_char = (self.card_char + ud).rem_euclid(13);
                sound::play(sound::SELECT);
                self.frame = 0;
                self.accordion = true;
                self.vms[27 + old as usize].pending_interrupt = 24;
                self.vms[27 + self.card_char as usize].pending_interrupt = 25;
            }
        }
        if inp.pressed(CANCEL) {
            self.card_leaving = true;
            self.frame = 0;
            sound::play(sound::CANCEL);
            self.vms[VM_ANCHOR].pending_interrupt = 1;
        }
    }

    // States 0x13–0x15, Result_PlayStats 0x4578AA (16 §2.4): fade in over 40 frames, wait (run time refreshed every
    // 60 frames), fade out over 20 frames, then back to state 1 in the same frame.
    fn play_data(&mut self, ctx: &mut Ctx) {
        match self.state {
            S_PLAY => {
                if self.frame == 1 {
                    ctx.game.update_run_time();
                    for k in 0..19 {
                        let s = self.play_line(ctx, k);
                        self.render(ctx, k, &s);
                    }
                    self.run_secs = ctx.game.score.plst.run_time[2];
                    // x 56; y 64, 81, 98, then +17, one extra +17 gap before k = 16 (16 §2.4).
                    for (k, vm) in self.text_vms[..20].iter_mut().enumerate() {
                        let y = 64 + 17 * k + if k >= 16 { 17 } else { 0 };
                        vm.pos = [56.0, y as f32, 0.0];
                    }
                }
                if self.frame < 40 {
                    self.set_play_alpha((self.frame * 255 / 40) as u8);
                } else {
                    // Alpha stays at 39·255/40; the frame counter is not reset.
                    self.state = S_PLAY_WAIT;
                }
            }
            S_PLAY_WAIT => {
                if self.frame % 60 == 0 {
                    ctx.game.update_run_time();
                    if ctx.game.score.plst.run_time[2] != self.run_secs {
                        self.run_secs = ctx.game.score.plst.run_time[2];
                        let s = self.play_line(ctx, 0);
                        self.render(ctx, 0, &s);
                    }
                }
                if ctx.input.pressed(SHOT | BOMB | PAUSE | ENTER) {
                    self.set_state(S_PLAY_OUT);
                }
            }
            _ => {
                // Byte arithmetic: −1 − f·255/20.
                self.set_play_alpha((255 - self.frame * 255 / 20) as u8);
                if self.frame > 19 {
                    self.set_state(S_INIT);
                    self.calc_state(ctx);
                }
            }
        }
    }

    fn set_play_alpha(&mut self, a: u8) {
        for vm in &mut self.text_vms[..20] {
            vm.color[anmvm::A] = a;
        }
    }

    // Play-data line k (16 §2.4 table; PLST 15 §2.9).
    fn play_line(&self, ctx: &Ctx, k: usize) -> String {
        let p = &ctx.game.score.plst;
        let row = |label: &str, idx: usize, last: usize| {
            let mut s = label.to_string();
            for b in [0, 1, 2, 3, 4, last] {
                s += &format!(" {:6}", p.counters[b][idx]);
            }
            s
        };
        let names = &crate::menulist::SHOT_NAMES;
        match k {
            0 => format!(
                "総起動時間   {:02}:{:02}:{:02}",
                p.run_time[0], p.run_time[1], p.run_time[2]
            ),
            1 => format!(
                "総プレイ時間 {:02}:{:02}:{:02}",
                p.play_time[0], p.play_time[1], p.play_time[2]
            ),
            2 => "プレイ回数　　　 　Easy 　Norm 　Hard 　Luna  Extra  Total".into(),
            3..=14 => row(names[k - 3], 1 + k - 3, 6),
            15 => row(names[12], 0, 6),
            16 => row("クリア回数  　　", 14, 6),
            // QUIRK: the continue row's last column is block 5 (always 0), not the total (16 §2.4).
            17 => row("コンティニュー  ", 15, 5),
            _ => row("プラクティス　  ", 16, 6),
        }
    }

    // State 9, Result_NameEntry 0x45621E (16 §2.2): input from frame 10, every key edge or repeat except the ESC finish.
    fn name_entry(&mut self, ctx: &mut Ctx) {
        if self.frame == 0 && !self.name_start(ctx) {
            return;
        }
        if self.frame < 10 {
            return;
        }
        let inp = ctx.input;
        let rep = |b: u16| inp.pressed_or_repeat(b);
        let (c, moves) = move_char_cursor(
            self.char_cursor,
            [rep(UP), rep(DOWN), rep(LEFT), rep(RIGHT)],
        );
        for _ in 0..moves {
            sound::play(sound::SELECT);
        }
        self.char_cursor = c;
        let mut finish = false;
        if rep(SHOT | ENTER) {
            if c < CELL_SP {
                self.type_char(CHARSET[c as usize]);
            } else if c == CELL_SP {
                self.type_char(b' ');
            } else {
                sound::play(sound::CANCEL);
                finish = true;
            }
        }
        // Backspace (16 §2.2): name[min(pos, 7)] = ' ', pos −= 1, name[pos] = ' '.
        if rep(BOMB | PAUSE) && self.name_pos > 0 {
            self.name[self.name_pos.min(7) as usize] = b' ';
            self.name_pos -= 1;
            self.name[self.name_pos as usize] = b' ';
            sound::play(sound::CANCEL);
        }
        if inp.pressed(PAUSE) {
            finish = true;
        }
        // The entry is edited in place inside the list, so the ranking shows it during entry.
        let name = self.name;
        if let Some(e) = ctx.game.score.new_entry_mut() {
            e.0[0x17..0x1F].copy_from_slice(&name);
        }
        if finish {
            let l = &mut ctx.game.score.lsnm;
            l.fill(0);
            l[..8].copy_from_slice(&name);
            self.interrupt_all(2);
            self.enter_panel();
        }
    }

    // name[min(pos, 7)] = ch; pos < 8 → pos += 1; at 8 the cursor jumps to END; SE 10 (16 §2.2).
    fn type_char(&mut self, ch: u8) {
        self.name[self.name_pos.min(7) as usize] = ch;
        if self.name_pos < 8 {
            self.name_pos += 1;
            if self.name_pos == 8 {
                self.char_cursor = CELL_END;
            }
        }
        sound::play(sound::OK);
    }

    // SPEC-GAP: who sends VM 71 interrupt 18 is not given; sent on entering state 0xF.
    fn enter_panel(&mut self) {
        self.set_state(S_PANEL);
        self.vms[VM_PANEL].pending_interrupt = 18;
    }

    // Frame 0 of state 9: the HSCR entry from the in-game record (16 §2.2, 15 §2.3). False = no entry.
    fn name_start(&mut self, ctx: &mut Ctx) -> bool {
        let g = &mut ctx.game;
        let shot = (g.shot as usize + g.demo_shot_bias as usize) % 12;
        let d = g.difficulty.clamp(0, 4) as usize;
        (self.rank_shot, self.rank_diff) = (shot as i32, d as i32);
        // Slow mode skips the entry (16 §2.1). The cfg copy's slow byte is 0 for Extra (15 §2.9).
        if g.cfg.slow != 0 && g.difficulty < 4 {
            g.restore_catk_backup();
            self.enter_panel();
            return false;
        }
        let st = &g.stats;
        let r = &st.rec;
        let mut h = Hscr::new();
        let stage = if g.flags & F_ENDING_OK != 0 {
            99
        } else {
            (g.stage % 9) as u8
        };
        let lsnm = g.score.lsnm;
        h.set(
            st.score10.clamp(0, 999_999_999) as u32,
            shot as u8,
            d as u8,
            stage,
            &lsnm[..8],
            &crate::supervisor::date_md(),
        );
        h.0[0x26] = st.continues;
        h.0[0x28..0x64].copy_from_slice(&g.cfg_bytes());
        h.put_u32(0x64, r.frames);
        h.put_u32(0x68, r.points);
        h.put_u32(0x70, st.deaths.max(0) as u32);
        h.put_u32(0x74, st.bombs_used.max(0) as u32);
        h.put_u32(0x78, r.last_spells);
        h.put_u32(0x7C, r.pauses);
        h.put_u32(0x80, r.orbs as u32);
        let rate = (r.human as u64 * 10000)
            .checked_div(r.total as u64)
            .unwrap_or(0);
        h.put_u32(0x84, rate as u32);
        h.0[0x88..0x88 + CARDS].copy_from_slice(&g.rec_cards[..CARDS]);
        // Slowdown % (15 §2.11): the port never drops frames, so x = 1 and the value is 0.
        h.0[0x10..0x14].copy_from_slice(&0f32.to_le_bytes());
        // Port marker for the entry being named (Result list node byte +9, 16 §2.3); rank > 9 → state 0xF.
        h.0[9] = 1;
        if g.score.insert_hscr(h).is_none() {
            self.enter_panel();
            return false;
        }
        self.name.copy_from_slice(&lsnm[..8]);
        self.name_pos = 0;
        // The cursor starts on END if an LSNM chunk was found in the file, else on 'A'.
        self.char_cursor = if g.lsnm_found { CELL_END } else { 0 };
        // SPEC-GAP: the slide-in of the ranking (VM 40), the difficulty plate (41–46) and the title (48) is
        // not specified for state 9; interrupt 3 + difficulty is sent to all VMs as in the viewer's state 5.
        self.interrupt_all((3 + d) as u16);
        true
    }

    // Ranking table (16 §2.3): drawn while VM 40's x < 640; A = VM 40's raw pos.
    fn draw_ranking(&self, ctx: &mut Ctx) {
        let a = self.vms[VM_ANCHOR].pos;
        if a[0] >= 640.0 || self.rank_shot < 0 {
            return;
        }
        let list = ctx
            .game
            .score
            .ranking(self.rank_shot as usize, self.rank_diff.clamp(0, 4) as usize);
        put(
            ctx,
            a[0] + 24.0,
            a[1] + 18.0,
            "No  Name       Score(Stage)   Date   Slow",
            0xFFE0E0EF,
        );
        for (i, e) in list.iter().enumerate() {
            let y = a[1] + 36.0 + 18.0 * i as f32;
            let mut x = a[0] + 24.0;
            let new = e.0[9] != 0;
            let col = match (self.state == S_NAME, new) {
                (true, true) => 0xFFF0F0FF,
                (true, false) => 0xC0FFC0C0,
                _ => 0xFFFFC0C0,
            };
            put(ctx, x, y, format!("{:2}", i + 1), col);
            x += 48.0;
            if self.state == S_NAME && new {
                let mut u = [b' '; 8];
                u[self.name_pos.min(7) as usize] = b'_';
                put(ctx, x, y, String::from_utf8_lossy(&u).into_owned(), col);
            }
            let stage = if e.stage() >= 99 {
                "C".to_string()
            } else {
                stage_shown(e.stage()).to_string()
            };
            put(
                ctx,
                x,
                y,
                format!(
                    "{:>8} {:9}{:1}({stage})",
                    e.name(),
                    e.score(),
                    e.continues()
                ),
                col,
            );
            x += 320.0;
            put(
                ctx,
                x,
                y,
                format!(" {:>5}   {:.2}", e.date(), e.slowdown()),
                col,
            );
        }
    }

    // Name grid (16 §2.2): origin (160, 356), cells 20 × 18; the selected glyph pulses 1.2 → 2.0 → 1.2.
    fn draw_grid(&self, ctx: &mut Ctx) {
        let f = self.frame;
        for cell in 0..96 {
            let (c, r) = (cell % 16, cell / 16);
            let (x, y) = (160.0 + 20.0 * c as f32, 356.0 + 18.0 * r as f32);
            let ch = match cell {
                CELL_SP => '\u{80}',
                CELL_END => '\u{81}',
                _ => CHARSET[cell as usize] as char,
            };
            let mut t = Text::new(x, y, ch.to_string()).argb(0xC0C0C0C0);
            if cell == self.char_cursor {
                let m = (f % 32) as f32 * 0.8 / 32.0;
                let s = if f % 64 < 32 { 1.2 + m } else { 2.0 - m };
                let o = (s - 1.0) * 8.0;
                t = Text::new(x - o, y - o, ch.to_string())
                    .argb(0xFFFFFFC0)
                    .scale(s);
            }
            ctx.ascii.push(t);
        }
    }

    // Result_DrawEndPanel 0x457E00 (16 §2.5); the queue colour follows VM 71 so the text fades with it.
    fn draw_panel(&self, ctx: &mut Ctx) {
        let vm = &self.vms[VM_PANEL];
        let col = vm_argb(vm);
        let (x, mut y) = (vm.pos[0] + 210.0, vm.pos[1] + 32.0);
        let g = &ctx.game;
        let st = &g.stats;
        let d = g.difficulty.clamp(0, 4) as usize;
        let progress = if g.flags & F_ENDING_OK != 0 {
            "      100%".to_string()
        } else {
            let t = if d >= 4 { 80000.0 } else { 195559.0 };
            format!("    {:.2}%", (st.rec.progress as f32 / t).min(0.99) * 100.0)
        };
        let lines = [
            format!("{:9}", st.score_disp),
            DIFF_PANEL[d].to_string(),
            progress,
            format!("{:9}", st.continues),
            format!("{:9}", st.deaths),
            format!("{:9}", st.bombs_used),
            format!("{:9}", st.spells),
            // Slowdown %: the port has no fps meter (15 §2.11), so it is always 0.
            "    0.00%".to_string(),
        ];
        let cont = st.continues.min(9);
        for (i, l) in lines.into_iter().enumerate() {
            let lx = if i >= 2 { x + 13.0 } else { x };
            put(ctx, lx, y, l, col);
            if i == 0 {
                put(ctx, x + 9.0 * 13.0, y, format!("{cont}"), col);
            }
            y += 22.0;
        }
    }

    // Card list (16 §2.4 state 8): bar, row text VM, counts and MaxBonus per row; accordion step after a character change.
    fn draw_cards(&mut self, ctx: &mut Ctx) {
        let a = self.vms[VM_ANCHOR].pos;
        if a[0] >= 640.0 || self.card_page < 0 {
            return;
        }
        let ids = group_cards(self.card_group);
        let s = self.card_built_char.clamp(0, 12) as usize;
        let mut vm10 = self.text_vms[10].clone();
        draw_at(ctx, &mut vm10, a[0] + 320.0, a[1] - 16.0);
        let (x, mut y) = (a[0], a[1] + 16.0);
        let f = self.frame;
        if f > 19 {
            self.accordion = false;
        }
        for i in 0..10 {
            let idx = (self.card_page * 10 + i) as usize;
            let Some(&id) = ids.get(idx) else { break };
            let c = ctx.game.score.catk[id].clone();
            let mut bar = self.bar.clone();
            bar.scale = [2.375, 1.0];
            bar.flags |= anmvm::F_SCALE_DIRTY;
            draw_at(ctx, &mut bar, x + 320.0, y + 16.0);
            let mut row = self.text_vms[i as usize].clone();
            draw_at(ctx, &mut row, x + 78.0, y);
            let (att, cap) = (c.counters[C_ATTEMPTS][s], c.counters[C_CAPTURES][s]);
            let col = if att == 0 {
                0xC0C0C0FF
            } else if cap == 0 {
                0xFFC0A0A0
            } else {
                0xFFF0F0FFu32 - i as u32 * 0x0008_0800
            };
            put(ctx, x, y, format!("No.{:02}", id + 1), col);
            let base = if id < 205 { 0 } else { C_PRACTICE };
            let (pa, pc) = (
                c.counters[base + C_ATTEMPTS][s],
                c.counters[base + C_CAPTURES][s],
            );
            let counts = if pa == 0 {
                "---/---(-)".to_string()
            } else {
                let l = DIFF_LETTER
                    .get(c.difficulty as usize)
                    .copied()
                    .unwrap_or('-');
                format!("{pc:3}/{pa:3}({l})")
            };
            put(ctx, x + 446.0, y, counts, col);
            if cap != 0 {
                ctx.ascii.push(
                    Text::new(
                        x + 424.0,
                        y - 13.0,
                        format!("MaxBonus {:8}", c.counters[C_BONUS][s]),
                    )
                    .argb(0xFFA08090)
                    .scale(0.8),
                );
            }
            y += if !self.accordion {
                33.0
            } else if f < 10 {
                ((10 - f) * 33 / 10) as f32
            } else {
                ((f - 10) * 33 / 10) as f32
            };
        }
    }
}

impl Task<Ctx> for ResultScreen {
    // Result_OnRegister 0x45964D.
    fn on_register(&mut self, ctx: &mut Ctx, chain: Chain, _: u8) -> anyhow::Result<()> {
        if chain != Chain::Calc {
            return Ok(());
        }
        // 0x18B8A2C := 0 (13 §3).
        crate::enemy::set_load_frames(0);
        // The 5×12 ranking lists are rebuilt from score.dat (15 §2.2–§2.3).
        ctx.game.score.rebuild_lists();
        match ctx
            .archive
            .read("result.jpg")
            .and_then(|b| ctx.gfx.load_image("result.jpg", &b))
        {
            Ok(t) => self.surface = Some(t),
            Err(e) => log::warn!("result.jpg: {e:#}"),
        }
        ctx.anm
            .load(&mut ctx.gfx, &ctx.archive, ANM_SLOT, "result00.anm")?;
        ctx.anm
            .load(&mut ctx.gfx, &ctx.archive, RT_SLOT, "resulttext.anm")?;
        for i in 0..VM_COUNT {
            let mut vm = AnmVm::default();
            ctx.anm.set_script(&mut vm, ANM_SLOT, i);
            self.vms.push(vm);
        }
        self.text_vms = (0..28)
            .map(|k| {
                if k < 14 {
                    text_vm_hidden(ctx, 0, 21 + k)
                } else {
                    text_vm_hidden(ctx, RT_SLOT, 2 + k - 14)
                }
            })
            .collect();
        self.bar = text_vm(ctx, ANM_SLOT, 32, true);
        // Captured counts +0x64 per group and character: CATK present (v3) and a story or practice capture (16 §2.4).
        let catk = &ctx.game.score.catk;
        for g in 0..6 {
            for id in group_cards(g as i32) {
                let c = &catk[id];
                if !c.present {
                    continue;
                }
                for s in 0..13 {
                    if c.counters[C_CAPTURES][s] != 0 || c.counters[C_PRACTICE + C_CAPTURES][s] != 0
                    {
                        self.captured[g][s] += 1;
                    }
                }
            }
        }
        Ok(())
    }

    // Result_OnRemove 0x459FD2: saves score.dat (2 RNG draws, 15 §2.1).
    fn on_remove(&mut self, ctx: &mut Ctx, chain: Chain, _: u8) {
        if chain != Chain::Calc {
            return;
        }
        ctx.game.score.clear_new_flags();
        ctx.game.save_score(&mut ctx.anm.rng, false);
        if let Some(t) = self.surface.take() {
            ctx.gfx.free_texture(t);
        }
        ctx.anm.unload(&mut ctx.gfx, ANM_SLOT);
        ctx.anm.unload(&mut ctx.gfx, RT_SLOT);
    }

    // Result_Calc 0x4584B0: state, then the 72 VMs, then frame += 1.
    fn calc(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        self.calc_state(ctx);
        // Session tool hook.
        ctx.game.trace_result = Some(self.state);
        for vm in &mut self.vms {
            vm.run(&mut ctx.anm);
        }
        self.frame += 1;
        Ret::Next
    }

    // Result_Draw 0x4586B4.
    fn draw(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        // The object is removed by the calc that requested scene 1 (16 §2.4 exit, P+20): nothing drawn then.
        if self.state == S_LEAVE && self.frame > 20 {
            return Ret::Next;
        }
        ctx.gfx.set_viewport(FULLSCREEN);
        if let Some(t) = self.surface {
            ctx.gfx.image(t, 0.0, 0.0);
        }
        // SPEC-GAP: pos_offset (op 24/6, the selected item's (−4, −4)) is added by the owner (05 §2 +0x288, INFERRED).
        for vm in &mut self.vms {
            let p = vm.pos;
            for k in 0..3 {
                vm.pos[k] += vm.pos_offset[k];
            }
            ctx.anm.draw(&mut ctx.gfx, vm);
            vm.pos = p;
        }
        match self.state {
            S_CARDS => self.draw_cards(ctx),
            _ => self.draw_ranking(ctx),
        }
        if matches!(self.state, S_PANEL | S_PANEL_OUT) {
            self.draw_panel(ctx);
        }
        if matches!(self.state, S_PLAY | S_PLAY_WAIT | S_PLAY_OUT) {
            for vm in &mut self.text_vms[..20] {
                ctx.anm.draw_mode(&mut ctx.gfx, vm, 0);
            }
        }
        if self.state == S_NAME {
            self.draw_grid(ctx);
        }
        Ret::Next
    }
}

// Name-entry cursor (16 §2.2): Up, Down, Left, Right are four independent tests in that order, each skipping the
// space cell and playing SE 12, so Up+Down held both move. Keys are [up, down, left, right]; returns (cursor, moves).
fn move_char_cursor(mut c: i32, keys: [bool; 4]) -> (i32, u32) {
    let space = |c: i32| CHARSET[c as usize] == b' ';
    let mut moves = 0;
    for (k, held) in keys.into_iter().enumerate() {
        if !held {
            continue;
        }
        loop {
            c = match k {
                0 => (c - 16).rem_euclid(96),
                1 => (c + 16).rem_euclid(96),
                // Left/Right wrap inside the 16-cell row.
                2 => c / 16 * 16 + (c % 16 + 15) % 16,
                _ => c / 16 * 16 + (c % 16 + 1) % 16,
            };
            if !space(c) {
                break;
            }
        }
        moves += 1;
    }
    (c, moves)
}

#[cfg(test)]
mod tests {
    use super::*;

    // 16 §2.3 (audit A.46): the ranking prints table 0x4C7F80[stage byte], so a default row (byte 0) shows "(1)".
    #[test]
    fn ranking_stage_map() {
        let shown: Vec<u8> = (0..9).map(stage_shown).collect();
        assert_eq!(shown, [1, 2, 3, 4, 4, 5, 6, 6, 1]);
    }

    // 16 §2.2: Up and Down held in one frame both apply (−16 then +16), with SE 12 twice; same for Left+Right.
    #[test]
    fn name_cursor_keys_independent() {
        assert_eq!(
            move_char_cursor(0x12, [true, true, false, false]),
            (0x12, 2)
        );
        assert_eq!(
            move_char_cursor(0x12, [false, false, true, true]),
            (0x12, 2)
        );
        assert_eq!(
            move_char_cursor(0x02, [true, false, false, false]),
            (0x52, 1)
        );
        assert_eq!(
            move_char_cursor(0x30, [false, false, true, false]),
            (0x3F, 1)
        );
        // Up from 0x0D (`N`) lands on 0x5D (space) and repeats to 0x4D.
        assert_eq!(
            move_char_cursor(0x0D, [true, false, false, false]),
            (0x4D, 1)
        );
        // Up then Down from 0x0D: 0x4D, then 0x5D (space) is skipped to 0x0D.
        assert_eq!(
            move_char_cursor(0x0D, [true, true, false, false]),
            (0x0D, 2)
        );
    }

    // Headless: stage 1 with Shot held until game over, "No" on the continue menu, name "PP" (cursor starts on END
    // because the game-start flush wrote LSNM), END, end panel, title; then score.dat is reloaded from the save dir.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn game_over_name_entry_saves() {
        let Some(dir) = crate::settings::test_game_dir() else {
            return;
        };
        let Ok(gfx) = pollster::block_on(crate::gfx::Gfx::headless()) else {
            return;
        };
        let src = crate::assets::Source::native(dir.into());
        let mut ctx = pollster::block_on(crate::game::Ctx::load_with(src, gfx, false)).unwrap();
        let save =
            crate::settings::temp_dir().join(format!("th08_result_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&save);
        crate::scoredat::set_save_dir(Some(save.clone()));
        ctx.game.persist = true;
        crate::supervisor::setup_new_game(&mut ctx.game, ctx.settings, 0);
        let keys: [(u32, u16); 6] = [
            (20, DOWN),
            (28, SHOT),
            (36, SHOT),
            (44, UP),
            (52, SHOT),
            (200, SHOT),
        ];
        let (mut k, mut over, mut seen_result) = (0u32, 0u32, false);
        for _ in 0..60_000 {
            let g = &ctx.game;
            ctx.input.raw = if g.scene == gamemgr::SCENE_RESULT {
                seen_result = true;
                k += 1;
                keys.iter()
                    .find(|&&(f, _)| (f..f + 3).contains(&k))
                    .map_or(0, |e| e.1)
            } else if g.game_over != 0 {
                // Lift the score above the default table so the run ranks.
                if over == 0
                    && let Some(gm) = gamemgr::current()
                {
                    gm.borrow().player.borrow_mut().stats.score10 = 1_234_567;
                }
                over += 1;
                if over % 40 == 39 { SHOT } else { 0 }
            } else if g.scene == gamemgr::SCENE_GAME {
                SHOT
            } else {
                0
            };
            if seen_result && ctx.game.scene == gamemgr::SCENE_MENU {
                break;
            }
            ctx.calc();
            ctx.draw();
            let on = ctx.game.scene == gamemgr::SCENE_RESULT;
            shot(&mut ctx, on, k, "after");
        }
        assert!(
            seen_result && ctx.game.scene == gamemgr::SCENE_MENU,
            "no result screen / no return"
        );
        let h = ctx
            .game
            .score
            .hscr
            .iter()
            .find(|h| h.shot() == 0 && h.difficulty() == 1)
            .cloned();
        let h = h.expect("no HSCR entry (score below the default table?)");
        assert_eq!(&h.name()[..2], "PP");
        assert_eq!(h.score(), 1_234_567);
        assert_eq!(h.0[9], 0);
        let file = std::fs::read(save.join("score.dat")).unwrap();
        let s = crate::scoredat::ScoreDat::parse(&file).unwrap();
        assert_eq!(&s.lsnm[..8], b"PP      ");
        assert!(s.hscr.contains(&h));
        assert_eq!(s.hiscore(0, 1), h.score().max(100_000));
        // Save → load → save: identical with the same two random header bytes (15 §2.1).
        let mut r1 = crate::rng::Rng::seeded(3);
        let mut r2 = crate::rng::Rng::seeded(3);
        let again = crate::scoredat::ScoreDat::parse(&s.encode(&mut r1)).unwrap();
        assert_eq!(
            again.encode(&mut r2),
            s.encode(&mut crate::rng::Rng::seeded(3))
        );
        // The viewer (mode 0): top → difficulty (Normal) → character 0 → ranking.
        let mut k = 0;
        for _ in 0..400 {
            if ctx.game.scene == gamemgr::SCENE_MENU && ctx.game.menu_loading == 0 && k == 0 {
                ctx.game.request(gamemgr::SCENE_RESULT_MENU);
            }
            if ctx.game.scene == gamemgr::SCENE_RESULT_MENU {
                k += 1;
            }
            ctx.input.raw = if [40, 80, 120].iter().any(|&f| (f..f + 3).contains(&k)) {
                SHOT
            } else {
                0
            };
            ctx.calc();
            ctx.draw();
            shot(&mut ctx, k > 0, k, "viewer");
        }
        crate::scoredat::set_save_dir(None);
        let _ = std::fs::remove_dir_all(&save);
    }

    // `TH08_RESULT_SHOTS=k1,k2,…`: /tmp/th08_result_<tag>_<k>.png at those result frames.
    #[cfg(not(target_arch = "wasm32"))]
    fn shot(ctx: &mut Ctx, on: bool, k: u32, tag: &str) {
        let want = ctx.settings.result_shots.contains(&k);
        if on && want {
            let img = ctx.gfx.capture(None);
            let _ = img.save(format!("/tmp/th08_result_{tag}_{k}.png"));
        }
        ctx.gfx.present();
    }

    #[test]
    fn charset_layout() {
        assert_eq!(CHARSET[0x5C], b'^');
        assert_eq!(CHARSET[0x5D], b' ');
        assert_eq!(CHARSET.iter().filter(|&&c| c == b' ').count(), 1);
        assert_eq!(group_cards(0).len(), 42);
        assert_eq!(group_cards(1).len(), 49);
        assert_eq!(group_cards(2).len(), 50);
        assert_eq!(group_cards(3).len(), 50);
        assert_eq!(group_cards(4).len(), 14);
        assert_eq!(group_cards(5).len(), 222);
    }
}
