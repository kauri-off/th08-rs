use crate::anmmgr::AnmManager;
use crate::anmvm::{self, AnmVm, Timer};
use crate::input::{self, Input};
use anyhow::{Result, ensure};

pub const FACE_SLOTS: [usize; 4] = [16, 17, 18, 19];
pub const TEXT_SLOT: usize = 0;

// Msg file per stage index 0–8 and team (10 §1).
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

pub fn file_name(stage: usize, shot: u8) -> String {
    format!("{}.dat", FILES[stage % 9][crate::sht::team(shot) as usize])
}

// Gui_LoadFaces (10 §3): player A, partner, stage face, second stage face.
pub fn face_files(stage: usize, shot: u8) -> [Option<String>; 4] {
    let team = crate::sht::team(shot) as usize;
    let a = ["rm", "mr", "sk", "ym"][team];
    let b = ["yk", "al", "rs", "yy"][team];
    let (s2, s3) = match stage % 9 {
        0 => ("st01", None),
        1 => ("st02", None),
        2 => ("st03", None),
        3 => ("st04a", None),
        4 => ("st04b", None),
        5 => ("st05", Some("st05b")),
        6 => ("st06", None),
        7 => ("st06", Some("st07")),
        _ => ("st08m", Some("st08")),
    };
    [Some(format!("face_{a}00.anm")), Some(format!("face_{b}00.anm")), Some(format!("face_{s2}.anm")), s3.map(|s| format!("face_{s}.anm"))]
}

pub struct MsgFile {
    data: Vec<u8>,
    pub entries: Vec<usize>,
}

#[derive(Clone, Copy, Debug)]
pub struct Ins {
    pub time: u16,
    pub op: u8,
    pub size: usize,
    pub at: usize,
}

pub fn parse(data: Vec<u8>) -> Result<MsgFile> {
    ensure!(data.len() >= 4, "msg file too short");
    let n = u32::from_le_bytes(data[0..4].try_into()?) as usize;
    ensure!(data.len() >= 4 + 4 * n, "msg entry table truncated");
    let entries = (0..n).map(|i| u32::from_le_bytes(data[4 + 4 * i..8 + 4 * i].try_into().unwrap()) as usize).collect::<Vec<_>>();
    ensure!(entries.iter().all(|&o| o + 4 <= data.len()), "msg entry offset out of range");
    Ok(MsgFile { data, entries })
}

impl MsgFile {
    pub fn ins(&self, at: usize) -> Option<Ins> {
        let h = self.data.get(at..at + 4)?;
        let size = h[3] as usize;
        (at + 4 + size <= self.data.len()).then_some(Ins { time: u16::from_le_bytes([h[0], h[1]]), op: h[2], size, at })
    }

    pub fn args(&self, i: &Ins) -> &[u8] {
        &self.data[i.at + 4..i.at + 4 + i.size]
    }

    pub fn i16(&self, i: &Ins, k: usize) -> i16 {
        let a = self.args(i);
        a.get(2 * k..2 * k + 2).map_or(0, |b| i16::from_le_bytes([b[0], b[1]]))
    }

    pub fn i32(&self, i: &Ins, k: usize) -> i32 {
        let a = self.args(i);
        a.get(4 * k..4 * k + 4).map_or(0, |b| i32::from_le_bytes(b.try_into().unwrap()))
    }

    pub fn u8(&self, i: &Ins, k: usize) -> u8 {
        self.args(i).get(k).copied().unwrap_or(0)
    }

    // XOR-0x77 Shift-JIS string starting at byte `from` of the arguments.
    pub fn string(&self, i: &Ins, from: usize) -> String {
        let raw: Vec<u8> = self.args(i).iter().skip(from).map(|b| b ^ 0x77).take_while(|&b| b != 0).collect();
        crate::text::decode_sjis(&raw)
    }
}

// Things the HUD / game manager must do after a Msg_Run (10 §5.1).
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Bgm(i32),
    BgmFade,
    Flash,
    StageEnd,
    HaltFinal,
    Route(u8),
    Sfx(u8),
}

// Facts Msg_Start needs about the running game (10 §4).
#[derive(Clone, Copy, Debug, Default)]
pub struct StartInfo {
    pub stage: usize,
    pub shot: u8,
    pub continues: u8,
    pub replay_route: Option<u8>,
    pub cleared_6a: bool,
    pub cleared_6b: bool,
    pub clock: i32,
}

pub struct Dialogue {
    pub file: Option<MsgFile>,
    pub info: StartInfo,
    pub face_slot: [usize; 4],
    ip: usize,
    pub entry: i32,
    pub clock: Timer,
    wait_count: i32,
    min_wait: i32,
    pub faces: [AnmVm; 4],
    pub lines: [AnmVm; 2],
    pub boss_card: AnmVm,
    text_colour: [[u8; 3]; 4],
    pub font_size: usize,
    pub release_counter: i32,
    skippable: bool,
    text_speaker: u8,
    new_box: bool,
    next_line: u8,
    active_speaker: u8,
    pub textbox: bool,
    pub choice: u8,
    pub stage_end: bool,
    pub texts: Vec<(usize, String, [u8; 3])>,
    pub events: Vec<Event>,
    pub side_effects: bool,
}

// tbl 0x4C7180 as RGB (10 §6).
const COLOURS: [[u8; 3]; 4] = [[255, 240, 232], [255, 232, 240], [240, 232, 255], [240, 232, 255]];

impl Default for Dialogue {
    fn default() -> Self {
        Self {
            file: None,
            info: StartInfo::default(),
            face_slot: FACE_SLOTS,
            ip: 0,
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
    pub fn active(&self) -> bool {
        self.file.is_some() && self.entry >= 0
    }

    pub fn blocking(&self) -> bool {
        self.active() && self.release_counter == 0
    }

    // Msg_Start (10 §4); the caller performs the playfield side effects (§4.6) when `side_effects` is set.
    pub fn start(&mut self, n: i32) {
        let Some(file) = self.file.take() else { return };
        let (info, face_slot, events, texts) = (self.info, self.face_slot, std::mem::take(&mut self.events), std::mem::take(&mut self.texts));
        *self = Dialogue { file: Some(file), info, face_slot, events, texts, ..Dialogue::default() };
        let mut n = n;
        if n == 0 && matches!(info.stage, 7 | 8) {
            // SPEC-GAP: the overlay base (0x4E4B64) and HUD effect 0x437F5C hooks are not modelled.
            self.face_slot.swap(2, 3);
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
                if !info.cleared_6a { (1, 0) } else { (2, 1) }
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
        let Some(&ip) = self.file.as_ref().and_then(|f| f.entries.get(n as usize)) else { return };
        self.entry = n;
        self.ip = ip;
        self.lines.iter_mut().for_each(|l| l.script_index = -1);
        self.textbox = true;
        self.skippable = true;
        self.min_wait = 6;
        self.new_box = true;
        self.side_effects = true;
    }

    fn render(&mut self, anm: &mut AnmManager, line: usize, s: String, colour: [u8; 3]) {
        anm.set_script(&mut self.lines[line], TEXT_SLOT, line);
        self.texts.push((line, s, colour));
    }

    fn clear_line1(&mut self) {
        if self.lines[1].script_index >= 0 {
            self.texts.push((1, " ".into(), [255; 3]));
        }
    }

    // Speaker highlight (10 §5.3).
    fn highlight(&mut self, who: u8) {
        if who >= 4 {
            return;
        }
        if who != self.active_speaker {
            let prev = self.active_speaker;
            for k in 0..4u8 {
                self.faces[k as usize].pending_interrupt = if k == prev && prev / 2 != who / 2 { 6 } else { 4 };
            }
        }
        self.faces[who as usize].pending_interrupt = 3;
        self.active_speaker = who;
    }

    fn set_face_sprite(&mut self, anm: &AnmManager, k: usize, s: i32) {
        if s >= 0 && k < 4 {
            self.faces[k].set_sprite(anm, s);
        }
    }

    fn exec(&mut self, f: &MsgFile, anm: &mut AnmManager, inp: &Input, ins: Ins) -> Exec {
        let skip = self.skippable && inp.down(input::SKIP);
        match ins.op {
            0 => {
                self.entry = -1;
                return Exec::End;
            }
            1 => {
                let (k, s) = (f.i16(&ins, 0) as usize, f.i16(&ins, 1));
                if k < 4 && s >= 0 {
                    anm.set_script(&mut self.faces[k], self.face_slot[k], s as usize);
                }
            }
            2 => {
                let (k, s) = (f.i16(&ins, 0) as usize, f.i16(&ins, 1) as i32);
                self.set_face_sprite(anm, k, s);
            }
            3 => {
                let (sp, line, s) = (f.i16(&ins, 0) as usize, f.i16(&ins, 1) as usize, f.string(&ins, 4));
                if line == 0 {
                    self.clear_line1();
                }
                let c = self.text_colour[sp.min(3)];
                self.render(anm, line.min(1), s, c);
                self.wait_count = 0;
            }
            4 => {
                let n = f.i32(&ins, 0);
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
            5 => {
                let (k, i) = (f.i16(&ins, 0) as usize, f.u8(&ins, 2));
                if k < 4 {
                    self.faces[k].pending_interrupt = i as u16;
                }
            }
            6 => self.release_counter += 1,
            7 => self.events.push(Event::Bgm(f.i32(&ins, 0))),
            8 => {
                anm.set_script(&mut self.boss_card, self.face_slot[2], 1);
                self.wait_count = 0;
            }
            9 => {
                self.stage_end = true;
                self.events.push(Event::StageEnd);
            }
            10 => return Exec::Stall,
            11 => {
                if !self.events.contains(&Event::HaltFinal) {
                    self.events.push(Event::HaltFinal);
                }
                return Exec::Stall;
            }
            12 => self.events.push(Event::BgmFade),
            13 => self.skippable = f.u8(&ins, 0) != 0,
            14 => self.events.push(Event::Flash),
            15 => {
                let who = f.i32(&ins, 0) as u8;
                self.highlight(who);
                for k in 0..4 {
                    let s = f.i32(&ins, 1 + k);
                    self.set_face_sprite(anm, k, s);
                }
                self.text_speaker = who.min(3);
                self.new_box = true;
            }
            16 => {
                let s = f.string(&ins, 0);
                if self.new_box {
                    self.clear_line1();
                    self.next_line = 0;
                }
                let (line, c) = (self.next_line.min(1) as usize, self.text_colour[self.text_speaker as usize]);
                self.render(anm, line, s, c);
                self.wait_count = 0;
                self.new_box = false;
                self.next_line += 1;
            }
            17 => {
                let (who, s) = (f.i32(&ins, 0) as u8, f.i32(&ins, 1));
                self.highlight(who);
                self.set_face_sprite(anm, who as usize, s);
                self.text_speaker = who.min(3);
                self.new_box = true;
            }
            18 => self.textbox = f.u8(&ins, 0) != 0,
            19 | 20 => {
                let s = f.string(&ins, 0);
                let c = self.text_colour[0];
                self.render(anm, (ins.op - 19) as usize, s, c);
                self.wait_count = 0;
            }
            21 => {
                let n = f.i32(&ins, 0);
                let old = self.choice;
                if inp.pressed(input::UP) {
                    self.choice = 0;
                } else if inp.pressed(input::DOWN) {
                    self.choice = 1;
                }
                if self.choice != old {
                    self.events.push(Event::Sfx(12));
                }
                let (on, off) = (self.choice as usize, 1 - self.choice as usize);
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
            22 => {
                self.events.push(Event::Route(self.choice));
                return Exec::Choice(self.choice);
            }
            _ => {}
        }
        Exec::Next
    }

    // Msg_Run (10 §5); `dying` = player state 2. Returns true when items should be auto-collected.
    pub fn run(&mut self, anm: &mut AnmManager, inp: &Input, dying: bool) -> bool {
        if !self.active() {
            return false;
        }
        if self.release_counter > 0 {
            self.release_counter -= 1;
        }
        let skip = self.skippable && inp.down(input::SKIP);
        if skip && let Some(i) = self.file.as_ref().unwrap().ins(self.ip) {
            self.clock.set(i.time as i32);
        }
        let mut stalled = false;
        loop {
            let Some(ins) = self.file.as_ref().unwrap().ins(self.ip) else {
                self.entry = -1;
                return !dying;
            };
            if self.clock.cur < ins.time as i32 {
                break;
            }
            let f = self.file.take().unwrap();
            let r = self.exec(&f, anm, inp, ins);
            self.file = Some(f);
            match r {
                Exec::Next => self.ip = ins.at + 4 + ins.size,
                Exec::Stall => {
                    stalled = true;
                    break;
                }
                Exec::End => return !dying,
                Exec::Choice(c) => self.start(c as i32 + 1),
            }
        }
        if !stalled {
            self.clock.tick(anm.speed);
        }
        for vm in self.faces.iter_mut().chain(self.lines.iter_mut()).chain(std::iter::once(&mut self.boss_card)) {
            if vm.script.is_some() {
                vm.run(anm);
            }
        }
        if self.clock.cur < 60 && self.skippable && inp.down(input::SKIP) {
            self.clock.set(60);
        }
        !dying
    }
}

// D3DCOLOR ARGB → VM colour bytes.
pub fn argb(c: u32) -> [u8; 4] {
    let mut v = [0u8; 4];
    v[anmvm::A] = (c >> 24) as u8;
    v[anmvm::R] = (c >> 16) as u8;
    v[anmvm::G] = (c >> 8) as u8;
    v[anmvm::B] = c as u8;
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    // Every msg file parses, every entry walks to op 0 / halt with known opcodes and in-range strings.
    #[test]
    fn all_msg_files() {
        let Some(ar) = crate::archive::test_archive() else { return };
        let names: std::collections::BTreeSet<String> = FILES.iter().flatten().map(|s| format!("{s}.dat")).collect();
        let mut ops = [0usize; 23];
        for name in &names {
            let f = parse(ar.read(name).unwrap()).unwrap();
            for &e in &f.entries {
                let mut at = e;
                let mut last = 0;
                loop {
                    let i = f.ins(at).unwrap_or_else(|| panic!("{name}: bad instruction at {at:#x}"));
                    assert!(i.op <= 22, "{name}: op {}", i.op);
                    ops[i.op as usize] += 1;
                    if i.op == 0 || i.op == 10 || i.op == 11 {
                        break;
                    }
                    assert!(i.time >= last, "{name}: time goes back at {at:#x}");
                    last = i.time;
                    if matches!(i.op, 3 | 16 | 19 | 20) {
                        let s = f.string(&i, if i.op == 3 { 4 } else { 0 });
                        assert!(!s.contains('\u{FFFD}'), "{name}: undecodable text {s:?}");
                    }
                    at += 4 + i.size;
                }
            }
        }
        assert!(ops.iter().all(|&c| c > 0), "unused opcodes: {ops:?}");
        let f = parse(ar.read("msg1a.dat").unwrap()).unwrap();
        assert_eq!(f.entries.len(), 2);
    }

    // Wait: Shot advances only after min_wait frames; timeout after n; skip is instant.
    #[test]
    fn wait_and_release() {
        let Some(ar) = crate::archive::test_archive() else { return };
        let mut d = Dialogue { file: Some(parse(ar.read("msg1a.dat").unwrap()).unwrap()), ..Dialogue::default() };
        let mut anm = AnmManager::default();
        d.start(0);
        assert!(d.blocking() && d.side_effects);
        let idle = Input::default();
        let mut frames = 0;
        let mut released = false;
        while d.active() && frames < 100_000 {
            d.run(&mut anm, &idle, false);
            released |= !d.blocking() && d.active();
            frames += 1;
        }
        assert!(!d.active() && released, "dialogue ended by timeouts after {frames} frames");
        assert!(frames > 1000);
        d.start(0);
        let skip = Input { held: input::SKIP, ..Input::default() };
        let mut n = 0;
        while d.active() && n < 100_000 {
            d.run(&mut anm, &skip, false);
            n += 1;
        }
        assert!(n < frames / 10, "skip: {n} frames vs {frames}");
    }
}
