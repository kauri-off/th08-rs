//! Ending object and `.end` script interpreter (16 §1.2–§1.7): Ending_Register 0x4296A0, calc prio 5, draw prio 4.

use crate::anmvm::{self, AnmVm};
use crate::game::Ctx;
use crate::gamemgr::{self, F_ENDING_OK};
use crate::gfx::TextureId;
use crate::input::{ENTER, SHOT, SKIP};
use crate::task::{Chain, Ret, Task};
use anyhow::Result;

pub const CALC_PRIO: u16 = 5;
pub const DRAW_PRIO: u16 = 4;
// ANM slot 0x18: staff01.anm (8 scripts, 13 sprites).
const STAFF_SLOT: usize = 24;
// 15 text VMs: text.anm (slot 0) scripts 0x12 + i, sprites 21 + i (512×17 strips).
const LINES: usize = 15;
const TEXT_SCRIPT: usize = 0x12;
// Text buffer 0x44 bytes (16 §1.3).
const LINE_BYTES: usize = 0x44;

// Interpreter result of one command/text run (0x428B80).
enum Step {
    Wait,
    End,
}

pub struct Ending {
    // Script buffer and read pointer (Ending_LoadScript 0x4295E0).
    script: Vec<u8>,
    ip: usize,
    // Surface slot 0 (`@b` picture); +8 / +0xC source x / y, +0x10 scroll speed (px per frame).
    picture: Option<TextureId>,
    pic_x: f32,
    pic_y: f32,
    speed: f32,
    vms: Vec<AnmVm>,
    raster: crate::text::Raster,
    // +0x2A58 skip-allowed (Ctrl fast-forward), +0x2A5C skip-allowed for the next `@F` file.
    skip_allowed: bool,
    skip_next: bool,
    // +0x2A64 frame counter.
    frame: i32,
    // +0x2A70 line timer / +0x2A8C its min-wait; +0x2A7C page timer / +0x2A88 its min-wait.
    line_timer: i32,
    line_min: i32,
    page_timer: i32,
    page_min: i32,
    // +0x2A90 per-line wait normal, +0x2A94 fast (Shot / Enter held) and min-wait.
    line_normal: i32,
    line_fast: i32,
    line: usize,
    // +0x2AA0 text colour 0xRRGGBB.
    text_rgb: u32,
    // Fade overlay (0x428910): mode 1–4 = `@0`–`@3`, counter +0x2AA8, duration +0x2AAC, colour +0x2AA4.
    fade_mode: u8,
    fade_t: i32,
    fade_n: i32,
    fade_argb: u32,
    // Port: the object is freed by its on-remove, so the draw node does nothing afterwards.
    done: bool,
}

impl Default for Ending {
    fn default() -> Self {
        Self::new()
    }
}

// Team of a shot: solo shots 4,5 → 0, 6,7 → 1, 8,9 → 2, 10,11 → 3 (16 §1.2 step 5).
pub fn team(shot: u8) -> usize {
    let s = shot as usize % 12;
    if s < 4 { s } else { (s - 4) / 2 }
}

// Script file for the current game (16 §1.2 step 5): `end0Na` bad, `end0Nb` good 6A, `end0Nc` good 6B.
pub fn script_name(g: &gamemgr::Game) -> String {
    let k = if g.flags & F_ENDING_OK == 0 {
        'a'
    } else if g.stage % 9 == 7 {
        'c'
    } else {
        'b'
    };
    format!("end{:02}{k}.end", team(g.shot))
}

// Archive lookups ignore directories (16 §1.3, 01): "data/end/end00.jpg" → "end00.jpg".
fn base_name(s: &str) -> &str {
    s.rsplit(['/', '\\']).next().unwrap_or(s)
}

// 0x43C1E9: some team 0..3 has a no-continue Final B clear on E–L (15 §2.4).
fn extra_unlocked(g: &gamemgr::Game) -> bool {
    (0..4).any(|t| g.clear_nc[t][..4].iter().any(|m| m & 0x4000 != 0))
}

// 0x43C2C5: some team 0..3 has a Final A clear on E–L (15 §2.4).
fn any_final_a(g: &gamemgr::Game) -> bool {
    (0..4).any(|t| g.clear_any[t][..4].iter().any(|m| m & 0x8000 != 0))
}

impl Ending {
    pub fn new() -> Self {
        Self {
            script: Vec::new(),
            ip: 0,
            picture: None,
            pic_x: 0.0,
            pic_y: 0.0,
            speed: 0.0,
            vms: Vec::new(),
            raster: crate::text::Raster::default(),
            skip_allowed: false,
            skip_next: false,
            frame: 0,
            line_timer: 0,
            line_min: 0,
            page_timer: 0,
            page_min: 0,
            // Ending_Ctor (16 §1.2): only +0x2A90 starts at 8; +0x2A94 and the colour are zero-filled (black).
            line_normal: 8,
            line_fast: 0,
            line: 0,
            text_rgb: 0,
            fade_mode: 0,
            fade_t: 0,
            fade_n: 1,
            fade_argb: 0,
            done: false,
        }
    }

    // Ending_Init 0x429980 (16 §1.2).
    fn init(&mut self, ctx: &mut Ctx) -> Result<()> {
        // Step 1: AnmManager texture / texture-matrix caches reset (the blend and vertex-path cache resets only force the
        // next state set, the device state the port tracks is unchanged), then two white clear+Presents. The HUD flag
        // 0x17CE8D0 has no effect here because the GM (and its HUD) is already unregistered (11 §7, 2 → 9).
        ctx.anm.reset_render_state();
        ctx.clear_present_twice([0xFF; 4]);
        ctx.anm
            .load(&mut ctx.gfx, &ctx.archive, STAFF_SLOT, "staff01.anm")?;
        self.update_clear_records(&mut ctx.game);
        // Step 4: text VMs i = 0..14 at (64, 16·i + 400); the owner's pos overrides the script's time-0 op 6 (05 §8).
        self.vms = (0..LINES)
            .map(|i| {
                let mut vm = AnmVm::default();
                ctx.anm.set_script(&mut vm, 0, TEXT_SCRIPT + i);
                vm.pos = [64.0, 16.0 * i as f32 + 400.0, 0.0];
                vm
            })
            .collect();
        // Debug `TH08_ENDING=<file>.end` loads that script instead of the one chosen by the game state.
        let name = match &ctx.settings.ending {
            Some(crate::settings::Ending::File(f)) => f.clone(),
            _ => script_name(&ctx.game),
        };
        log::info!("ending script {name}");
        self.load_script(ctx, &name)?;
        Ok(())
    }

    // Ending_Init step 3: clear records by ending type (16 §1.2, 15 §2.4).
    fn update_clear_records(&mut self, g: &mut gamemgr::Game) {
        let shot = g.shot as usize % 12;
        if g.flags & F_ENDING_OK == 0 {
            // CLRD[shot]+0x20 is read and cleared (never written nonzero by the game itself).
            self.skip_allowed = g.score.clrd[shot].bad_ending != 0;
            g.score.clrd[shot].bad_ending = 0;
            g.score.plst.music[18] = 1;
            return;
        }
        let final_b = g.stage % 9 == 7;
        let bit: u16 = if final_b { 0x4000 } else { 0x8000 };
        g.play_counter(crate::scoredat::PL_CLEARS);
        // "Already unlocked before this run", tested before this ending's bits are set.
        self.skip_next = if final_b {
            extra_unlocked(g)
        } else {
            any_final_a(g)
        };
        let no_continue = g.stats.continues == 0;
        let masks = if no_continue {
            &g.clear_nc[shot]
        } else {
            &g.clear_any[shot]
        };
        self.skip_allowed = masks[..4].iter().any(|m| m & bit != 0);
        g.set_clear(no_continue, bit);
        g.score.plst.music[18] = 1;
        g.score.plst.music[19] = 1;
    }

    // Ending_LoadScript 0x4295E0: script := file, ip := start, per-line wait normal +0x2A90 := 8 (+0x2A94 keeps the
    // last `@s` value), timers +0x2A70 / +0x2A64 reset (16 §1.2 step 6).
    fn load_script(&mut self, ctx: &mut Ctx, name: &str) -> Result<()> {
        self.set_script(ctx.archive.read(base_name(name))?);
        Ok(())
    }

    fn set_script(&mut self, data: Vec<u8>) {
        self.script = data;
        self.ip = 0;
        self.line_normal = 8;
        self.line_timer = 0;
        self.frame = 0;
    }

    fn byte(&self, p: usize) -> u8 {
        self.script.get(p).copied().unwrap_or(0)
    }

    // Integer argument: `atol`, then skip to the next NUL and over any NULs (16 §1.3).
    fn int_arg(&mut self) -> i32 {
        let mut p = self.ip;
        while matches!(self.byte(p), b' ' | b'\t') {
            p += 1;
        }
        let neg = match self.byte(p) {
            b'-' => {
                p += 1;
                true
            }
            b'+' => {
                p += 1;
                false
            }
            _ => false,
        };
        let mut v: i32 = 0;
        while self.byte(p).is_ascii_digit() {
            v = v
                .wrapping_mul(10)
                .wrapping_add((self.byte(p) - b'0') as i32);
            p += 1;
        }
        self.ip = p;
        self.skip_arg();
        if neg { v.wrapping_neg() } else { v }
    }

    // String argument up to its NUL terminator.
    fn str_arg(&mut self) -> String {
        let start = self.ip;
        let mut p = start;
        while p < self.script.len() && !matches!(self.byte(p), 0 | b'\n' | b'\r') {
            p += 1;
        }
        let s = String::from_utf8_lossy(&self.script[start..p]).into_owned();
        self.ip = p;
        self.skip_arg();
        s
    }

    fn skip_arg(&mut self) {
        while self.ip < self.script.len() && !matches!(self.byte(self.ip), 0 | b'\n') {
            self.ip += 1;
        }
        while self.ip < self.script.len() && self.byte(self.ip) == 0 {
            self.ip += 1;
        }
    }

    // After a command (and a text line): skip to the end of the line and over all CR/LF.
    fn skip_line(&mut self) {
        while self.ip < self.script.len() && self.byte(self.ip) != b'\n' {
            self.ip += 1;
        }
        while self.ip < self.script.len() && matches!(self.byte(self.ip), b'\n' | b'\r') {
            self.ip += 1;
        }
    }

    // Shot / Enter edge (mask 0x1001), or Ctrl held while skip-allowed (16 §1.4).
    fn released(&self, ctx: &Ctx) -> bool {
        ctx.input.pressed(SHOT | ENTER) || self.skip_allowed && ctx.input.down(SKIP)
    }

    // One interpreter frame (0x428B80, 16 §1.4); false when the script ended (`@z`).
    fn interpret(&mut self, ctx: &mut Ctx) -> Result<bool> {
        'run: {
            if self.page_timer > 0 {
                self.page_timer -= 1;
                if self.page_min == 0 {
                    if self.released(ctx) {
                        self.page_timer = 0;
                    }
                } else {
                    self.page_min -= 1;
                }
                if self.page_timer > 0 {
                    break 'run;
                }
                // Page end: interrupt 2 clears the 15 lines.
                for vm in &mut self.vms {
                    vm.pending_interrupt = 2;
                }
                self.line = 0;
            }
            if self.line_timer == 0 {
                if let Step::End = self.run_commands(ctx)? {
                    return Ok(false);
                }
            } else {
                self.line_timer -= 1;
                if self.line_min == 0 {
                    if self.released(ctx) {
                        self.line_timer = 0;
                    }
                } else {
                    self.line_min -= 1;
                }
            }
        }
        self.frame += 1;
        self.pic_y -= self.speed;
        if self.pic_y <= 0.0 {
            self.pic_y = 0.0;
            self.speed = 0.0;
        }
        Ok(true)
    }

    // Commands and text until something waits (text line, `@r`, `@w`) or the script ends (16 §1.3).
    fn run_commands(&mut self, ctx: &mut Ctx) -> Result<Step> {
        loop {
            // SPEC-GAP: a script running off its end without `@z` is treated as `@z`.
            if self.ip >= self.script.len() {
                return Ok(Step::End);
            }
            if self.byte(self.ip) != b'@' {
                self.text_line(ctx);
                self.skip_line();
                return Ok(Step::Wait);
            }
            let cmd = self.byte(self.ip + 1);
            self.ip += 2;
            let mut wait = false;
            match cmd {
                b'0'..=b'3' => {
                    let n = self.int_arg();
                    self.fade_mode = cmd - b'0' + 1;
                    self.fade_t = 0;
                    self.fade_n = n;
                }
                b'F' => {
                    let name = self.str_arg();
                    match self.load_script(ctx, &name) {
                        Ok(()) => {
                            self.skip_allowed = self.skip_next;
                        }
                        Err(e) => log::warn!("ending @F {name}: {e:#}"),
                    }
                    // QUIRK: falls through into `@R`; the skip below then drops the new file's first line.
                    self.reset_scripts();
                }
                b'R' => self.reset_scripts(),
                b'M' => {
                    let n = self.int_arg();
                    ctx.audio.fade_bgm(n as f32);
                }
                b'V' => {
                    let a = self.int_arg();
                    let b = self.int_arg();
                    self.speed = a as f32 / b as f32;
                }
                b'a' => {
                    let vm = self.int_arg();
                    let script = self.int_arg();
                    let sprite = self.int_arg();
                    if let Some(v) = self.vms.get_mut(vm.max(0) as usize) {
                        ctx.anm.set_script(v, STAFF_SLOT, script.max(0) as usize);
                        v.set_sprite(&ctx.anm, sprite);
                    }
                }
                b'b' => {
                    let name = self.str_arg();
                    let file = base_name(&name).to_string();
                    let tex = ctx
                        .archive
                        .read(&file)
                        .and_then(|b| ctx.gfx.load_image(&file, &b));
                    match tex {
                        Ok(t) => {
                            if let Some(old) = self.picture.replace(t) {
                                ctx.gfx.free_texture(old);
                            }
                        }
                        Err(e) => log::warn!("ending @b {name}: {e:#}"),
                    }
                }
                b'c' => self.text_rgb = self.int_arg() as u32 & 0xFFFFFF,
                b'm' => {
                    let name = self.str_arg();
                    // Music_Play(0, file) + Music_PlaySlot(0, 0), which marks music flag 0 heard.
                    ctx.audio.play_bgm(&name);
                    ctx.game.score.plst.music[0] = 1;
                }
                b'r' => {
                    self.page_timer = self.int_arg();
                    self.page_min = self.int_arg();
                    self.line_timer = 0;
                    wait = true;
                }
                b's' => {
                    self.line_normal = self.int_arg();
                    self.line_fast = self.int_arg();
                }
                b'v' => self.pic_y = self.int_arg() as f32,
                b'w' => {
                    self.line_timer = self.int_arg();
                    self.line_min = self.int_arg();
                    wait = true;
                }
                b'z' => return Ok(Step::End),
                _ => {}
            }
            self.skip_line();
            if wait {
                return Ok(Step::Wait);
            }
        }
    }

    // `@R`: script index := 0 on the VMs.
    // SPEC-GAP: 16 §1.3 says 16 VMs; the object has 15 text VMs, the 16th write is not modelled.
    fn reset_scripts(&mut self) {
        for vm in &mut self.vms {
            vm.script_index = 0;
        }
    }

    // Text line (16 §1.3): copied two bytes at a time until 0 / LF / CR at an even position, drawn into
    // VM[line] with Text_RenderToVm(colour, outline −1), interrupt 1, then the per-line wait.
    fn text_line(&mut self, ctx: &mut Ctx) {
        let mut buf = Vec::new();
        while buf.len() + 2 < LINE_BYTES {
            let c = self.byte(self.ip);
            if self.ip >= self.script.len() || matches!(c, 0 | b'\n' | b'\r') {
                break;
            }
            buf.push(c);
            buf.push(self.byte(self.ip + 1));
            self.ip += 2;
        }
        let text = crate::text::decode_sjis(&buf);
        // SPEC-GAP: a page with more than 15 lines is not handled by 16 §1.3; extra lines are dropped.
        if let Some(vm) = self.vms.get_mut(self.line) {
            let rgb = [
                (self.text_rgb >> 16) as u8,
                (self.text_rgb >> 8) as u8,
                self.text_rgb as u8,
            ];
            render_vm(ctx, &mut self.raster, vm, &text, rgb);
            vm.pending_interrupt = 1;
        }
        self.line += 1;
        let fast = ctx.input.down(SHOT | ENTER);
        self.line_timer = if fast {
            self.line_fast
        } else {
            self.line_normal
        };
        self.line_min = self.line_fast;
    }

    // Fade overlay 0x428910 (16 §1.5): one step per draw call; returns the overlay colour.
    // SPEC-GAP: whether the counter steps before or after the alpha is computed is not given; after here.
    fn fade_step(&mut self) -> u32 {
        if self.fade_mode == 0 {
            return 0;
        }
        let n = self.fade_n.max(1);
        let t = self.fade_t.min(n);
        let rgb = if self.fade_mode >= 3 { 0xFFFFFF } else { 0 };
        let alpha = match self.fade_mode {
            1 | 3 if self.fade_t >= n => {
                self.fade_mode = 0;
                0
            }
            1 | 3 => 255 - t * 255 / n,
            _ => t * 255 / n,
        };
        if self.fade_t < n {
            self.fade_t += 1;
        }
        self.fade_argb = ((alpha as u32) << 24) | rgb;
        self.fade_argb
    }
}

// Text_RenderToVm (10 §6) into the VM's text.anm strip, font size 15 (VM+0x298), then visible.
fn render_vm(
    ctx: &mut Ctx,
    raster: &mut crate::text::Raster,
    vm: &mut AnmVm,
    s: &str,
    rgb: [u8; 3],
) {
    let Some(sp) = ctx.anm.sprite(0, vm.sprite_index.max(0) as usize).copied() else {
        return;
    };
    let t = ctx.gfx.texture(sp.tex);
    let (w, ty) = (t.width, (sp.uv[1] * t.height as f32).round() as u32);
    let size = 15;
    let rgba = raster.render_outline(s, rgb, w as usize, size, 0.0, -1);
    ctx.gfx.write_texture(sp.tex, 0, ty, w, size as u32, &rgba);
    vm.flags |= anmvm::F_VISIBLE;
}

impl Task<Ctx> for Ending {
    fn on_register(&mut self, ctx: &mut Ctx, chain: Chain, _: u8) -> Result<()> {
        if chain == Chain::Calc {
            self.init(ctx)?;
        }
        Ok(())
    }

    // Calc 0x429860: interpreter then the 15 VMs; up to 9 times with Ctrl while skip-allowed (16 §1.2).
    fn calc(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        let n = if self.skip_allowed && ctx.input.down(SKIP) {
            9
        } else {
            1
        };
        for _ in 0..n {
            match self.interpret(ctx) {
                Ok(true) => {}
                Ok(false) => {
                    self.done = true;
                    return Ret::Remove;
                }
                Err(e) => {
                    log::error!("ending: {e:#}");
                    self.done = true;
                    return Ret::Remove;
                }
            }
            for vm in &mut self.vms {
                if vm.script_index >= 0 {
                    vm.run(&mut ctx.anm);
                }
            }
        }
        Ret::Next
    }

    // Draw 0x4298F0 (16 §1.5): picture, 15 VMs, fade overlay.
    fn draw(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        if self.done {
            return Ret::Next;
        }
        ctx.gfx.set_viewport(crate::gfx::FULLSCREEN);
        if let Some(t) = self.picture {
            // SPEC-GAP: the float source offset is truncated to whole pixels (integer copy rectangle).
            let (sx, sy) = (self.pic_x.max(0.0) as u32, self.pic_y.max(0.0) as u32);
            ctx.gfx.image_rect(t, sx, sy, 640, 480, 0.0, 0.0);
        }
        for vm in &mut self.vms {
            ctx.anm.draw(&mut ctx.gfx, vm);
        }
        let c = self.fade_step();
        if c >> 24 != 0 {
            let rgba = [(c >> 16) as u8, (c >> 8) as u8, c as u8, (c >> 24) as u8];
            ctx.gfx.rect(0.0, 0.0, 640.0, 480.0, rgba);
        }
        Ret::Next
    }

    // On-remove 0x429D40 (16 §1.6): ANM slot 0x18 and surface 0 released, next scene 6 (result, name entry).
    fn on_remove(&mut self, ctx: &mut Ctx, chain: Chain, _: u8) {
        if chain != Chain::Calc {
            return;
        }
        ctx.anm.unload(&mut ctx.gfx, STAFF_SLOT);
        if let Some(t) = self.picture.take() {
            ctx.gfx.free_texture(t);
        }
        self.script.clear();
        // Port guard: a removal by a scene switch away from 9 keeps that switch's target.
        if ctx.game.requested == gamemgr::SCENE_ENDING {
            ctx.game.request(gamemgr::SCENE_RESULT);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_choice() {
        let mut g = gamemgr::Game::default();
        for (shot, t) in [(0, 0), (3, 3), (4, 0), (5, 0), (7, 1), (9, 2), (11, 3)] {
            assert_eq!(team(shot), t);
        }
        g.shot = 6;
        g.flags.set(0);
        assert_eq!(script_name(&g), "end01a.end");
        g.flags.set(F_ENDING_OK);
        g.stage = 6;
        assert_eq!(script_name(&g), "end01b.end");
        g.stage = 7;
        assert_eq!(script_name(&g), "end01c.end");
    }

    #[test]
    fn args() {
        let mut e = Ending::new();
        e.script = b"@w120\x00120\x00\n\r\nline\x00\n".to_vec();
        e.ip = 2;
        assert_eq!(e.int_arg(), 120);
        assert_eq!(e.int_arg(), 120);
        e.skip_line();
        assert_eq!(e.byte(e.ip), b'l');
        e.script = b"@Fdata/staff00b.end\x00\n".to_vec();
        e.ip = 2;
        assert_eq!(base_name(&e.str_arg()), "staff00b.end");
    }

    // 16 §1.2: Ending_Ctor zero-fills colour and +0x2A94; LoadScript resets only +0x2A90.
    #[test]
    fn start_values() {
        let mut e = Ending::new();
        assert_eq!((e.text_rgb, e.line_normal, e.line_fast), (0, 8, 0));
        (e.line_normal, e.line_fast) = (20, 3);
        e.set_script(b"@z\n".to_vec());
        assert_eq!((e.line_normal, e.line_fast), (8, 3));
    }

    #[test]
    fn fades() {
        let mut e = Ending::new();
        (e.fade_mode, e.fade_t, e.fade_n) = (1, 0, 4);
        let a: Vec<u32> = (0..6).map(|_| e.fade_step() >> 24).collect();
        assert_eq!(a, [255, 192, 128, 64, 0, 0]);
        (e.fade_mode, e.fade_t, e.fade_n) = (4, 0, 2);
        let a: Vec<u32> = (0..4).map(|_| e.fade_step()).collect();
        assert_eq!(a, [0x00FFFFFF, 0x7FFFFFFF, 0xFFFFFFFF, 0xFFFFFFFF]);
    }
}
