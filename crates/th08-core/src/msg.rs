//! Dialogue interpreter of the HUD (Msg_Load / Msg_Start / Msg_Run, msg*.dat) (10 §1–§5, §8).

use crate::anmmgr::AnmManager;
use crate::anmvm::{AnmVm, Timer};
use crate::input::{self, Input};
use crate::textref::{MSG_FILES, Text, TextRef};
use crate::types::{D3dColor, lo8};
use th08_formats::msg::Op;
use th08_formats::sjis::SjisString;

// ANM slots of the portrait files face[0..3]; text.anm is slot 0 (10 §6).
pub const FACE_SLOTS: [usize; 4] = [16, 17, 18, 19];
pub const TEXT_SLOT: usize = 0;

// Msg file per stage index 0–8 and team.
const FILES: [[&str; 4]; 9] = [
    ["msg1a", "msg1b", "msg1c", "msg1d"],
    ["msg2a", "msg2b", "msg2c", "msg2d"],
    ["msg3a", "msg3b", "msg3c", "msg3d"],
    ["msg4dm", "msg4ab", "msg4ac", "msg4dm"],
    ["msg4ba", "msg4dm", "msg4dm", "msg4bd"],
    ["msg5a", "msg5b", "msg5c", "msg5d"],
    ["msg6a", "msg6b", "msg6c", "msg6d"],
    ["msg7a", "msg7b", "msg7c", "msg7d"],
    ["msg8a", "msg8b", "msg8c", "msg8d"],
];

// Table 0x4C74C0: solos use their team's file.
pub fn file_name(stage: usize, shot: u8) -> String {
    format!(
        "{}.dat",
        FILES[stage % 9][usize::from(th08_formats::sht::team(shot))]
    )
}

// Gui_LoadFaces: player A, partner, stage face, second stage face. `card` is the practised card id in
// spell practice: player slots stay empty and slot 18 gets the `…sp` cut-in file (Ex: none for other cards).
pub fn face_files(stage: usize, shot: u8, card: Option<i16>) -> [Option<String>; 4] {
    let face = |s: &str| Some(format!("face_{s}.anm"));
    let Some(c) = card else {
        let team = usize::from(th08_formats::sht::team(shot));
        let (s2, s3) = match stage {
            0 => ("st01", None),
            1 => ("st02", None),
            3 => ("st04a", None),
            4 => ("st04b", None),
            5 => ("st05", Some("st05b")),
            6 => ("st06", None),
            7 => ("st06", Some("st07")),
            8 => ("st08m", Some("st08")),
            // Stage 2 and any out-of-range index.
            _ => ("st03", None),
        };
        return [
            face(&format!("{}00", ["rm", "mr", "sk", "ym"][team])),
            face(&format!("{}00", ["yk", "al", "rs", "yy"][team])),
            face(s2),
            s3.and_then(face),
        ];
    };
    let s2 = match stage {
        0 => Some("st01sp"),
        1 => Some("st02sp"),
        2 => Some("st03sp"),
        3 => Some(match c {
            0xD8 => "sksp",
            0xD9 => "ymsp",
            0xDA => "alsp",
            0xDB => "rssp",
            0xDC => "yysp",
            0xDD => "yksp",
            _ => "st04asp",
        }),
        4 => Some("st04bsp"),
        5 => Some(if c == 0xD4 { "st05msp" } else { "st05sp" }),
        6 => Some("st06sp"),
        7 => Some(if (0x93..=0x96).contains(&c) {
            "st06sp"
        } else {
            "st07sp"
        }),
        8 => match c {
            0xBF..=0xC1 | 0xD5 => Some("st08msp"),
            0xC2..=0xCC | 0xD3 => Some("st08sp"),
            _ => None,
        },
        _ => Some("st03"),
    };
    [None, None, s2.and_then(face), None]
}

pub use th08_formats::msg::{Ins, MsgError, MsgFile, MsgOff, parse};

// i16 face / line / speaker argument as an index: negatives map to ≥ 0x8000, which fails every `< 4` test and
// clamps under `min` the same as the original's sign extension.
fn arg_index(v: i16) -> usize {
    usize::from(v.cast_unsigned())
}

// Things the HUD / game manager must do after a Msg_Run.
// Bgm op 7, BgmFade op 12, Flash op 14, StageEnd op 9, HaltFinal op 11, Route = GM flags bits 11–12 (ops 21/22),
// Plate / SpellBase = entry-0 hooks.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Bgm(i32),
    BgmFade,
    Flash,
    StageEnd,
    HaltFinal,
    Route(u8),
    Sfx(u8),
    Plate(usize),
    SpellBase(usize),
}

// Facts Msg_Start needs about the running game.
#[derive(Clone, Copy, Debug, Default)]
pub struct StartInfo {
    pub stage: usize,
    pub shot: u8,
    // stats+0x29: 10 §4 step 3 calls it the practice flag, 16 §1 the continue count.
    pub continues: u8,
    // Route byte of the replay header (replay+8 → +0x11C) during playback.
    pub replay_route: Option<u8>,
    // Clear-mask bits 6 (6A) / 7 (6B) over all four difficulties for this shot.
    pub cleared_6a: bool,
    pub cleared_6b: bool,
    // Clock counter stats+0x28.
    pub clock: i32,
}

// Dialogue state at HudImpl+0x21814..; `Ctx.msg`.
#[derive(Clone)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "separate bytes +0x22D7C.. / +0x22D84 of the original and one port-only hand-off, not a flag word"
)]
pub struct Dialogue {
    // The stage's .msg file, shared immutable data.
    pub file: Option<std::sync::Arc<MsgFile>>,
    pub info: StartInfo,
    // Portrait file per face index; 6B / Extra swap slots 2 and 3 on entry 0 (Gui file pointers 0x4ECC9C ↔ 0x4ECCA0).
    pub face_slot: [usize; 4],
    // Offset of the next instruction.
    ip: MsgOff,
    // +0x2181C running entry, −1 = none; +0x21820 script clock (05 §2 Timer).
    pub entry: i32,
    pub clock: Timer,
    // +0x2182C frames since the last text op; +0x21830 minimum frames before Shot advances a wait (6, then 8 or 30).
    wait_count: i32,
    min_wait: i32,
    // face[k] +0x21834, line[l] +0x222C4 (text.anm strips), boss_card +0x2280C; VM +0x22AB0 is not modelled (unused).
    pub faces: [AnmVm; 4],
    pub lines: [AnmVm; 2],
    pub boss_card: AnmVm,
    // +0x22D54 per-speaker COLORREF as RGB; outline style +0x22D64 is always 0 and not stored.
    text_colour: [[u8; 3]; 4],
    pub font_size: usize,
    // +0x22D78: op 6 increments, each Msg_Run decrements; Msg_IsBlocking needs 0.
    pub release_counter: i32,
    // +0x22D7C..+0x22D82: skippable, text_speaker, new_box, next_line, active_speaker (0xFF none), textbox, choice.
    skippable: bool,
    text_speaker: u8,
    new_box: bool,
    next_line: u8,
    active_speaker: u8,
    pub textbox: bool,
    pub choice: u8,
    // +0x22D84, set by op 9; the HUD then draws the clear bonus panel (10 §9).
    pub stage_end: bool,
    // Port-only: pending Text_RenderToVm requests (line, text, colour) the HUD records as text commands (10 §6).
    pub texts: Vec<(usize, Text, [u8; 3])>,
    pub events: Vec<Event>,
    // Port-only: set by Msg_Start; the caller then performs the playfield side effects of 10 §4 step 6.
    pub side_effects: bool,
}

// tbl 0x4C7180 as RGB (10 §6).
const COLOURS: [[u8; 3]; 4] = [
    [255, 240, 232],
    [255, 232, 240],
    [240, 232, 255],
    [240, 232, 255],
];

impl Default for Dialogue {
    fn default() -> Self {
        Self {
            file: None,
            info: StartInfo::default(),
            face_slot: FACE_SLOTS,
            ip: MsgOff(0),
            entry: -1,
            clock: Timer::default(),
            wait_count: 0,
            min_wait: 0,
            faces: Default::default(),
            lines: Default::default(),
            boss_card: AnmVm::default(),
            text_colour: COLOURS,
            font_size: 15,
            release_counter: 0,
            skippable: false,
            text_speaker: 0,
            new_box: false,
            next_line: 0,
            active_speaker: 0xFF,
            textbox: false,
            choice: 0,
            stage_end: false,
            texts: Vec::new(),
            events: Vec::new(),
            side_effects: false,
        }
    }
}

enum Exec {
    Next,
    Stall,
    End,
    Choice(u8),
}

impl Dialogue {
    // Clone test state (worldclone.rs): entry, ip, clocks, counters and the line VMs' scripts.
    #[cfg(test)]
    pub fn trace(&self) -> Vec<u64> {
        let mut t = vec![
            i64::from(self.entry).cast_unsigned(),
            u64::from(self.ip.0),
            i64::from(self.clock.cur).cast_unsigned(),
            i64::from(self.wait_count).cast_unsigned(),
            i64::from(self.min_wait).cast_unsigned(),
            i64::from(self.release_counter).cast_unsigned(),
            u64::from(self.next_line),
            u64::from(self.new_box),
        ];
        t.extend(
            self.lines
                .iter()
                .map(|v| i64::from(v.script_index).cast_unsigned()),
        );
        t
    }

    pub fn skippable_shown(&self) -> bool {
        self.active() && self.skippable
    }

    // Msg_IsActive 0x4358BB / Msg_IsBlocking 0x43587E.
    pub fn active(&self) -> bool {
        self.file.is_some() && self.entry >= 0
    }

    pub fn blocking(&self) -> bool {
        self.active() && self.release_counter == 0
    }

    // Msg_Start; the caller performs the playfield side effects (10 §4 step 6) when `side_effects` is set.
    pub fn start(&mut self, n: i32) {
        let Some(file) = self.file.take() else { return };
        let (info, face_slot, events, texts) = (
            self.info,
            self.face_slot,
            std::mem::take(&mut self.events),
            std::mem::take(&mut self.texts),
        );
        *self = Dialogue {
            file: Some(file),
            info,
            face_slot,
            events,
            texts,
            ..Dialogue::default()
        };
        let mut n = n;
        if n == 0 {
            // Entry 0 hooks.
            if matches!(info.stage, 7 | 8) {
                self.face_slot.swap(2, 3);
            }
            if matches!(info.stage, 6..=8) {
                self.events.push(Event::SpellBase(2));
            }
            match info.stage {
                5 => self.events.push(Event::Plate(0x16)),
                7 => self.events.push(Event::Plate(0x18)),
                8 => self.events.push(Event::Plate(0x19)),
                _ => {}
            }
        }
        if info.stage == 5 && n == 10 {
            let (m, c) = if info.continues != 0 {
                (1, 0)
            } else if let Some(r) = info.replay_route {
                match r {
                    2 => (3, 1),
                    1 => (2, 1),
                    _ => (1, 0),
                }
            } else if !info.cleared_6b && info.shot < 4 {
                if info.cleared_6a { (2, 1) } else { (1, 0) }
            } else {
                (3, 0)
            };
            n = m;
            self.choice = c;
            self.events.push(Event::Route(c));
        }
        if info.stage == 7 && n > 5 && info.clock > 11 {
            n = 5;
        }
        // Port-only guard: a missing entry leaves the dialogue inactive.
        let Some(&ip) = self
            .file
            .as_ref()
            .and_then(|f| f.entries.get(n.cast_unsigned() as usize))
        else {
            return;
        };
        self.entry = n;
        self.ip = ip;
        // Script index −1 makes the first line-1 clear a no-op; the other defaults come from Default.
        self.lines.iter_mut().for_each(|l| l.script_index = -1);
        self.textbox = true;
        self.skippable = true;
        self.min_wait = 6;
        self.new_box = true;
        self.side_effects = true;
    }

    // Op 16 line VM at +0x222C4 + line·0x2A4 with no clamp: line 2 is boss_card (+0x2280C).
    // Line 3 (+0x22AB0, not modelled) never occurs: the data has at most three op-16 lines per box.
    pub fn line_vm(&self, line: usize) -> Option<&AnmVm> {
        match line {
            0 | 1 => Some(&self.lines[line]),
            2 => Some(&self.boss_card),
            _ => None,
        }
    }

    fn line_vm_mut(&mut self, line: usize) -> Option<&mut AnmVm> {
        match line {
            0 | 1 => Some(&mut self.lines[line]),
            2 => Some(&mut self.boss_card),
            _ => None,
        }
    }

    // Starts text.anm script `line` on the line VM and queues Text_RenderToVm (10 §6).
    fn render(
        &mut self,
        anm: &mut AnmManager,
        rng: &mut crate::rng::Rng,
        line: usize,
        s: Text,
        colour: [u8; 3],
    ) {
        let Some(vm) = self.line_vm_mut(line) else {
            return;
        };
        anm.set_script(rng, vm, TEXT_SLOT, line);
        self.texts.push((line, s, colour));
    }

    // Renders " " into line 1 if it has a script (ops 3/16).
    fn clear_line1(&mut self) {
        if self.lines[1].script_index >= 0 {
            self.texts
                .push((1, Text::dynamic(SjisString(b" ".to_vec())), [255; 3]));
        }
    }

    // Speaker highlight.
    fn highlight(&mut self, who: u8) {
        if who >= 4 {
            return;
        }
        if who != self.active_speaker {
            let prev = self.active_speaker;
            for k in 0..4u8 {
                self.faces[usize::from(k)].pending_interrupt = if k == prev && prev / 2 != who / 2 {
                    6
                } else {
                    4
                };
            }
        }
        self.faces[usize::from(who)].pending_interrupt = 3;
        self.active_speaker = who;
    }

    fn set_face_sprite(&mut self, anm: &AnmManager, k: usize, s: i32) {
        if s >= 0 && k < 4 {
            self.faces[k].set_sprite(anm, s);
        }
    }

    // One instruction of the 10 §5.1 opcode table, decoded by `msg::Op` and run by its group.
    fn exec(
        &mut self,
        f: &MsgFile,
        anm: &mut AnmManager,
        rng: &mut crate::rng::Rng,
        inp: &Input,
        ins: Ins,
    ) -> Exec {
        match f.op(&ins) {
            op @ (Op::FaceScript { .. }
            | Op::FaceSprite { .. }
            | Op::FaceInterrupt { .. }
            | Op::BossCard
            | Op::Speaker4 { .. }
            | Op::Speaker { .. }) => {
                self.exec_portrait(anm, rng, &op);
                Exec::Next
            }
            op @ (Op::TextLine { .. }
            | Op::Text(_)
            | Op::Text0(_)
            | Op::Text1(_)
            | Op::Skippable(_)
            | Op::Textbox(_)) => {
                let key = self.text_key(f, &ins);
                self.exec_text(anm, rng, op, key);
                Exec::Next
            }
            op => self.exec_flow(inp, &op),
        }
    }

    // Portrait ops 1, 2, 5, 8, 15, 17.
    fn exec_portrait(&mut self, anm: &mut AnmManager, rng: &mut crate::rng::Rng, op: &Op) {
        match *op {
            // Op 1. The pos_offset writes of ops 1/2 are omitted: they have no visible effect.
            Op::FaceScript { face, script } => {
                let k = arg_index(face);
                if k < 4 && script >= 0 {
                    anm.set_script(
                        rng,
                        &mut self.faces[k],
                        self.face_slot[k],
                        arg_index(script),
                    );
                }
            }
            // Op 2.
            Op::FaceSprite { face, sprite } => {
                self.set_face_sprite(anm, arg_index(face), i32::from(sprite));
            }
            // Op 5.
            Op::FaceInterrupt { face, interrupt } => {
                let k = arg_index(face);
                if k < 4 {
                    self.faces[k].pending_interrupt = u16::from(interrupt);
                }
            }
            // Op 8.
            Op::BossCard => {
                anm.set_script(rng, &mut self.boss_card, self.face_slot[2], 1);
                self.wait_count = 0;
            }
            // Op 15: the speaker byte is the arg's low byte.
            Op::Speaker4 { who, sprites } => {
                let who = lo8(who.cast_signed());
                self.highlight(who);
                for (k, s) in sprites.into_iter().enumerate() {
                    self.set_face_sprite(anm, k, s);
                }
                self.text_speaker = who.min(3);
                self.new_box = true;
            }
            // Op 17.
            Op::Speaker { who, sprite } => {
                let who = lo8(who.cast_signed());
                self.highlight(who);
                self.set_face_sprite(anm, usize::from(who), sprite);
                self.text_speaker = who.min(3);
                self.new_box = true;
            }
            // Other groups are dispatched by `exec`.
            _ => {}
        }
    }

    // Text ops 3, 13, 16, 18, 19, 20.
    // Translation key of a text op: msg file, entry (lowest index with its offset), text-op number.
    fn text_key(&self, f: &MsgFile, ins: &Ins) -> TextRef {
        let name = file_name(self.info.stage, self.info.shot);
        let file = MSG_FILES
            .iter()
            .position(|&n| name.strip_suffix(".dat") == Some(n));
        let start = f
            .entries
            .get(self.entry.max(0).cast_unsigned() as usize)
            .copied();
        match (file, start) {
            (Some(file), Some(start)) => TextRef::Msg {
                file: file as u8,
                entry: f.entries.iter().position(|&o| o == start).unwrap_or(0) as u16,
                nth: f.text_ops_before(start, ins.at),
            },
            _ => TextRef::Dynamic,
        }
    }

    fn exec_text(&mut self, anm: &mut AnmManager, rng: &mut crate::rng::Rng, op: Op, key: TextRef) {
        match op {
            // Op 3.
            Op::TextLine {
                speaker,
                line,
                text,
            } => {
                let (sp, line) = (arg_index(speaker), arg_index(line));
                if line == 0 {
                    self.clear_line1();
                }
                let c = self.text_colour[sp.min(3)];
                self.render(anm, rng, line.min(1), Text::new(key, text), c);
                self.wait_count = 0;
            }
            // Op 13.
            Op::Skippable(b) => self.skippable = b != 0,
            // Op 16.
            Op::Text(s) => {
                if self.new_box {
                    self.clear_line1();
                    self.next_line = 0;
                }
                let (line, c) = (
                    usize::from(self.next_line),
                    self.text_colour[usize::from(self.text_speaker)],
                );
                self.render(anm, rng, line, Text::new(key, s), c);
                self.wait_count = 0;
                self.new_box = false;
                self.next_line = self.next_line.wrapping_add(1);
            }
            // Op 18.
            Op::Textbox(b) => self.textbox = b != 0,
            // Ops 19 / 20: line 0 / 1 with text_colour[0].
            Op::Text0(s) => self.text_on_line(anm, rng, 0, Text::new(key, s)),
            Op::Text1(s) => self.text_on_line(anm, rng, 1, Text::new(key, s)),
            // Other groups are dispatched by `exec`.
            _ => {}
        }
    }

    fn text_on_line(
        &mut self,
        anm: &mut AnmManager,
        rng: &mut crate::rng::Rng,
        line: usize,
        s: Text,
    ) {
        let c = self.text_colour[0];
        self.render(anm, rng, line, s, c);
        self.wait_count = 0;
    }

    // Flow ops 0, 4, 6, 7, 9–12, 14, 21, 22 and unknown opcodes.
    fn exec_flow(&mut self, inp: &Input, op: &Op) -> Exec {
        match *op {
            // Op 0.
            Op::End => {
                self.entry = -1;
                return Exec::End;
            }
            // Op 4 wait: Skip advances at once; Shot after min_wait; else stall up to n frames, then time out.
            Op::Wait(n) => {
                let skip = self.skippable && inp.down(input::SKIP);
                if skip {
                } else if inp.pressed(input::SHOT) && self.wait_count >= self.min_wait {
                    self.new_box = true;
                    self.min_wait = 8;
                } else if self.wait_count < n {
                    self.wait_count += 1;
                    return Exec::Stall;
                } else {
                    self.new_box = true;
                    self.min_wait = 30;
                }
            }
            // Op 6.
            Op::ReleaseTimeline => self.release_counter += 1,
            // Op 7.
            Op::Bgm(n) => self.events.push(Event::Bgm(n)),
            // Op 9.
            Op::StageEnd => {
                self.stage_end = true;
                self.events.push(Event::StageEnd);
            }
            // Op 10 halt stalls forever; the game manager ends the stage.
            Op::Halt => return Exec::Stall,
            Op::HaltFinal => {
                // Op 11 sets GM flags & ~0x60 | 0x40 every stalled frame (one event is enough), but only in stage idx
                // 6–8; in stages 1–5 it only stalls like op 10.
                if matches!(self.info.stage, 6..=8) && !self.events.contains(&Event::HaltFinal) {
                    self.events.push(Event::HaltFinal);
                }
                return Exec::Stall;
            }
            // Op 12.
            Op::BgmFade => self.events.push(Event::BgmFade),
            // Op 14.
            Op::Flash => self.events.push(Event::Flash),
            Op::Choice(n) => {
                // Op 21 choice: ignores Skip; Shot needs wait_count ≥ 60; timeout keeps the current choice.
                let old = self.choice;
                if inp.pressed(input::UP) {
                    self.choice = 0;
                } else if inp.pressed(input::DOWN) {
                    self.choice = 1;
                }
                if self.choice != old {
                    self.events.push(Event::Sfx(12));
                }
                let (on, off) = (usize::from(self.choice), 1 - usize::from(self.choice));
                self.lines[on].color = [255; 4];
                self.lines[off].color = argb(0xE060_6060);
                if inp.pressed(input::SHOT) && self.wait_count >= 60 {
                    self.events.push(Event::Sfx(10));
                } else if self.wait_count < n {
                    self.wait_count += 1;
                    return Exec::Stall;
                } else {
                    self.new_box = true;
                    self.min_wait = 30;
                }
            }
            // Op 22: Msg_Start(choice + 1) and the new entry runs in the same frame.
            Op::ChoiceJump => {
                self.events.push(Event::Route(self.choice));
                return Exec::Choice(self.choice);
            }
            // Unknown opcodes and the other groups (dispatched by `exec`): the instruction just advances.
            _ => {}
        }
        Exec::Next
    }

    // Msg_Run; `dying` = player state 2. Returns true when items should be auto-collected.
    pub fn run(
        &mut self,
        anm: &mut AnmManager,
        rng: &mut crate::rng::Rng,
        inp: &Input,
        dying: bool,
    ) -> bool {
        if !self.active() {
            return false;
        }
        if self.release_counter > 0 {
            self.release_counter -= 1;
        }
        let skip = self.skippable && inp.down(input::SKIP);
        // Ctrl fast-forward: clock = time of the current instruction.
        if skip && let Some(i) = self.file.as_ref().unwrap().ins(self.ip) {
            self.clock.set(i32::from(i.time));
        }
        let mut stalled = false;
        loop {
            let Some(ins) = self.file.as_ref().unwrap().ins(self.ip) else {
                // Port-only guard: running off the file ends the dialogue.
                self.entry = -1;
                return !dying;
            };
            if self.clock.cur < i32::from(ins.time) {
                break;
            }
            let f = self.file.take().unwrap();
            let r = self.exec(&f, anm, rng, inp, ins);
            self.file = Some(f);
            match r {
                Exec::Next => self.ip = ins.next(),
                Exec::Stall => {
                    stalled = true;
                    break;
                }
                // Op 0 returns at once: no clock tick and no VM run this frame.
                Exec::End => return !dying,
                Exec::Choice(c) => self.start(i32::from(c) + 1),
            }
        }
        // A stall leaves without ticking the clock, but the VMs still run.
        if !stalled {
            self.clock.tick(anm.speed);
        }
        for vm in self
            .faces
            .iter_mut()
            .chain(self.lines.iter_mut())
            .chain(std::iter::once(&mut self.boss_card))
        {
            if vm.script.is_some() {
                vm.run(anm, rng);
            }
        }
        if self.clock.cur < 60 && self.skippable && inp.down(input::SKIP) {
            self.clock.set(60);
        }
        !dying
    }
}

// D3DCOLOR 0xAARRGGBB → VM colour bytes B, G, R, A (05 §2).
pub fn argb(c: u32) -> [u8; 4] {
    D3dColor(c).to_bgra()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Every msg::FILES name is in textref::MSG_FILES.
    #[test]
    fn msg_files_cover_the_table() {
        for stage in 0..9 {
            for shot in 0..12 {
                let n = file_name(stage, shot);
                assert!(
                    crate::textref::MSG_FILES.contains(&n.trim_end_matches(".dat")),
                    "{n}"
                );
            }
        }
    }

    // Face tables: normal per stage and team, spell practice by card id.
    #[test]
    fn face_file_tables() {
        let f = |v: [Option<String>; 4]| v.map(Option::unwrap_or_default);
        assert_eq!(
            f(face_files(7, 9, None)),
            [
                "face_sk00.anm",
                "face_rs00.anm",
                "face_st06.anm",
                "face_st07.anm"
            ]
        );
        assert_eq!(f(face_files(3, 1, None))[3], "");
        assert_eq!(
            f(face_files(3, 0, Some(0xDB))),
            ["", "", "face_rssp.anm", ""]
        );
        assert_eq!(f(face_files(3, 0, Some(0xD6)))[2], "face_st04asp.anm");
        assert_eq!(f(face_files(5, 0, Some(0xD4)))[2], "face_st05msp.anm");
        assert_eq!(f(face_files(7, 0, Some(0x96)))[2], "face_st06sp.anm");
        assert_eq!(f(face_files(7, 0, Some(0x97)))[2], "face_st07sp.anm");
        assert_eq!(f(face_files(8, 0, Some(0xD5)))[2], "face_st08msp.anm");
        assert_eq!(f(face_files(8, 0, Some(0xD3)))[2], "face_st08sp.anm");
        assert_eq!(face_files(8, 0, Some(0xCD)), [None, None, None, None]);
    }

    // Every msg file parses, every entry walks to op 0 / halt with known opcodes and in-range strings.
    #[test]
    fn all_msg_files() {
        let Some(ar) = crate::testdata::archive() else {
            return;
        };
        let names: std::collections::BTreeSet<String> =
            FILES.iter().flatten().map(|s| format!("{s}.dat")).collect();
        let mut ops = [0usize; 23];
        for name in &names {
            let f = parse(ar.read(name).unwrap()).unwrap();
            for &e in &f.entries {
                let mut at = e;
                let mut last = 0;
                loop {
                    let i = f
                        .ins(at)
                        .unwrap_or_else(|| panic!("{name}: bad instruction at {at:#x}"));
                    assert!(i.op <= 22, "{name}: op {}", i.op);
                    ops[usize::from(i.op)] += 1;
                    if i.op == 0 || i.op == 10 || i.op == 11 {
                        break;
                    }
                    assert!(i.time >= last, "{name}: time goes back at {at:#x}");
                    last = i.time;
                    if matches!(i.op, 3 | 16 | 19 | 20) {
                        let s = f.string(&i, if i.op == 3 { 4 } else { 0 }).decode();
                        assert!(!s.contains('\u{FFFD}'), "{name}: undecodable text {s:?}");
                    }
                    at = i.next();
                }
            }
        }
        assert!(ops.iter().all(|&c| c > 0), "unused opcodes: {ops:?}");
        let f = parse(ar.read("msg1a.dat").unwrap()).unwrap();
        assert_eq!(f.entries.len(), 2);
    }

    // Op 11: the game-clear trigger only in stage idx 6–8; elsewhere it only stalls like op 10.
    #[test]
    fn halt_final_trigger_by_stage() {
        let mut rng = crate::rng::Rng::default();
        let f = parse(vec![1, 0, 0, 0, 8, 0, 0, 0, 0, 0, 11, 0]).unwrap();
        let mut anm = AnmManager::default();
        for stage in 0..9 {
            let mut d = Dialogue::default();
            d.info.stage = stage;
            let ins = f.ins(MsgOff(8)).unwrap();
            for _ in 0..2 {
                assert!(matches!(
                    d.exec(&f, &mut anm, &mut rng, &Input::default(), ins),
                    Exec::Stall
                ));
            }
            let n = d.events.iter().filter(|e| **e == Event::HaltFinal).count();
            assert_eq!(n, usize::from(stage >= 6), "stage {stage}");
        }
    }

    // Op 16: no clamp, so the third line of a box goes to boss_card (line index 2).
    #[test]
    fn op16_third_line_on_boss_card() {
        let mut rng = crate::rng::Rng::default();
        let text = |c: u8| [0, 0, 16, 2, c ^ 0x77, 0x77];
        let mut data = vec![1, 0, 0, 0, 8, 0, 0, 0];
        for c in *b"abc" {
            data.extend(text(c));
        }
        let f = parse(data).unwrap();
        let mut anm = AnmManager::default();
        let mut d = Dialogue {
            new_box: true,
            ..Dialogue::default()
        };
        for k in 0..3 {
            let ins = f.ins(MsgOff(8 + 6 * k)).unwrap();
            d.exec(&f, &mut anm, &mut rng, &Input::default(), ins);
        }
        let lines: Vec<(usize, String)> = d.texts.iter().map(|t| (t.0, t.1.raw.decode())).collect();
        assert_eq!(lines, [(0, "a".into()), (1, "b".into()), (2, "c".into())]);
        assert!(std::ptr::eq(d.line_vm(2).unwrap(), &d.boss_card));
        assert!(d.line_vm(3).is_none());
    }

    // Wait: Shot advances only after min_wait frames; timeout after n; skip is instant.
    #[test]
    fn wait_and_release() {
        let mut rng = crate::rng::Rng::default();
        let Some(ar) = crate::testdata::archive() else {
            return;
        };
        let mut d = Dialogue {
            file: Some(parse(ar.read("msg1a.dat").unwrap()).unwrap().into()),
            ..Dialogue::default()
        };
        let mut anm = AnmManager::default();
        d.start(0);
        assert!(d.blocking() && d.side_effects);
        let idle = Input::default();
        let mut frames = 0;
        let mut released = false;
        while d.active() && frames < 100_000 {
            d.run(&mut anm, &mut rng, &idle, false);
            released |= !d.blocking() && d.active();
            frames += 1;
        }
        assert!(
            !d.active() && released,
            "dialogue ended by timeouts after {frames} frames"
        );
        assert!(frames > 1000);
        d.start(0);
        let skip = Input {
            held: input::SKIP,
            ..Input::default()
        };
        let mut n = 0;
        while d.active() && n < 100_000 {
            d.run(&mut anm, &mut rng, &skip, false);
            n += 1;
        }
        assert!(n < frames / 10, "skip: {n} frames vs {frames}");
    }

    // Stage-5 entry 10 → Final A / B dialogue or the choice menu.
    #[test]
    fn final_route_resolution() {
        let Some(ar) = crate::testdata::archive() else {
            return;
        };
        let run = |shot: u8, a: bool, b: bool, cont: u8| {
            let info = StartInfo {
                stage: 5,
                shot,
                cleared_6a: a,
                cleared_6b: b,
                continues: cont,
                ..StartInfo::default()
            };
            let mut d = Dialogue {
                file: Some(parse(ar.read(&file_name(5, shot)).unwrap()).unwrap().into()),
                info,
                ..Dialogue::default()
            };
            d.start(10);
            (d.entry, d.choice)
        };
        assert_eq!(run(0, false, false, 0), (1, 0));
        assert_eq!(run(0, true, false, 0), (2, 1));
        assert_eq!(run(0, true, true, 0), (3, 0));
        assert_eq!(run(4, false, false, 0), (3, 0));
        assert_eq!(run(1, true, true, 1), (1, 0));
    }
}
