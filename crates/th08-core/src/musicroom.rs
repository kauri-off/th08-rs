//! Music room `MusicRoom_Register` 0x449254, scene 8 (16 §5.1–§5.3): musiccmt.txt list, heard flags, track selection.

use crate::anmvm::{self, AnmVm};
use crate::ascii::Text;
use crate::builtin::Builtin;
use crate::drawlist::{FULLSCREEN, Recorder, TexId, TextCmd};
use crate::game::Ctx;
use crate::gamemgr;
use crate::input::{BOMB, DOWN, ENTER, KEY_R, PAUSE, SHOT, SKIP, UP};
use crate::task::{Chain, Ret};
use crate::textref::{Text as Txt, TextRef};
use crate::types::D3dColor;
pub use th08_formats::musiccmt::{COMMENT_LINES, Entry, MAX_ENTRIES, parse_musiccmt};
use th08_formats::sjis::SjisString;

// Calc 0x449399 at prio 4, draw 0x449487 at prio 3 (16 §5).
pub const CALC_PRIO: u16 = 4;
pub const DRAW_PRIO: u16 = 3;
// music00.anm in ANM slot 0x17 (16 §5.1).
const ANM_SLOT: usize = 23;
// Comment display copies 64 bytes of a line into a zeroed 0x42 buffer (16 §5.1 QUIRK: longer lines are cut).
const COMMENT_SHOWN: usize = 64;
// The draw shows 10 rows (16 §5.2).
const ROWS: i32 = 10;
// text.anm (slot 0) scripts: comment line k = 10 + k, playing title 0x11 (16 §5.3).
const COMMENT_SCRIPT: usize = 10;
const TITLE_SCRIPT: usize = 0x11;
// Comment / title text colour, COLORREF 0xFFE0C0 = RGB (192, 224, 255), and mode arg 0x300000 (16 §5.3).
const COMMENT_COLOUR: u32 = 0x00FF_E0C0;
const COMMENT_MODE: i32 = 0x0030_0000;
// The one-byte cursor string 0x7F: ascii.anm sprite 126, a ▶ glyph (16 §5.2, 12 §2.9).
const CURSOR: &str = "\u{7f}";

// PLST+0x208 heard flags → object +0xC + 4i (16 §5.1); playing in the music room never sets them.
fn heard_flags(plst: &[u8; MAX_ENTRIES]) -> [bool; MAX_ENTRIES] {
    plst.map(|f| f != 0)
}

// Title-list text of entry `track` (16 §5.2): (text, COLORREF colour, mode arg).
fn row_text(e: &Entry, track: usize, heard: bool) -> (Txt, u32, i32) {
    if heard {
        let key = TextRef::MusicTitle { track: track as u8 };
        (Txt::new(key, e.title.clone()), 0x00C0_E0FF, 0x0030_2080)
    } else {
        (Builtin::Unknown8.txt(), 0x0080_A0C0, 0x0010_0040)
    }
}

// Comment line k of the selected entry (16 §5.3): the real comment if heard or playing, else the warning; cut to
// 64 bytes.
fn comment_text(e: &Entry, track: usize, heard: bool, playing: bool, k: usize) -> Txt {
    if heard || playing {
        let l = e
            .comments
            .get(k)
            .map(SjisString::as_bytes)
            .unwrap_or_default();
        let key = TextRef::MusicCmt {
            track: track as u8,
            line: k as u8,
        };
        Txt::new(key, SjisString(l[..l.len().min(COMMENT_SHOWN)].to_vec()))
    } else {
        Builtin::MusicWarning(k as u8).txt()
    }
}

// Up (16 §5.3 state 1 step 1): cursor − 1; below 0 → last entry with the last 10 rows shown. Returns (cursor, top).
fn cursor_up(cursor: i32, top: i32, n: i32) -> (i32, i32) {
    let c = cursor - 1;
    if c < 0 {
        (n - 1, (n - ROWS).max(0))
    } else if c < top {
        (c, c)
    } else {
        (c, top)
    }
}

// Down (16 §5.3 state 1 step 2): cursor + 1; past the end → 0 / top 0; top follows when cursor − 10 ≥ top.
fn cursor_down(cursor: i32, top: i32, n: i32) -> (i32, i32) {
    let c = cursor + 1;
    if c >= n {
        (0, 0)
    } else if top <= c - ROWS {
        (c, c - ROWS + 1)
    } else {
        (c, top)
    }
}

// Multi-bit edge of the music-room key masks (16 §5.3): (cur & m) ≠ 0 and ≠ (prev & m).
fn mask_edge(inp: &crate::input::Input, m: u16) -> bool {
    inp.held & m != 0 && inp.held & m != inp.prev & m
}

// GDI COLORREF 0x00BBGGRR → RGB (10 §6).
fn colorref(c: u32) -> [u8; 3] {
    [c as u8, (c >> 8) as u8, (c >> 16) as u8]
}

// VM +0x1F0 colour as D3DCOLOR 0xAARRGGBB, for `Ascii_SetColor`.
fn vm_argb(vm: &AnmVm) -> u32 {
    D3dColor::from_bgra(vm.color).0
}

// Entry / row number as the object's int fields hold it (at most 32 entries, 16 §5.1).
#[expect(clippy::cast_possible_wrap, reason = "counts < 32 (16 §5.1)")]
fn int(v: usize) -> i32 {
    v as i32
}

// Text_RenderToVm (10 §6), font 15: the rasteriser gets x = u0 and the texture-row width and replaces the whole
// 15-row strip; the outline is always black, `mode` only selects ±1 (−1) / ±2 px. Sets VM flag bit 0.
fn render_vm(ctx: &mut Ctx, vm: &mut AnmVm, s: Txt, colour: u32, mode: i32) {
    let slot = usize::from(vm.anm_slot.max(0).cast_unsigned());
    let Some(sp) = ctx
        .w
        .anm
        .sprite(slot, usize::from(vm.sprite_index.max(0).cast_unsigned()))
        .copied()
    else {
        return;
    };
    let t = ctx.rec.texture(sp.tex);
    #[expect(
        clippy::cast_sign_loss,
        reason = "ftol of the strip's texel row (10 §6); uv is never negative"
    )]
    let (w, tx, ty) = (
        t.width,
        (sp.uv[0] * t.width as f32).round(),
        (sp.uv[1] * t.height as f32).round() as u32,
    );
    ctx.rec.text(TextCmd {
        dst: sp.tex,
        y: ty,
        w,
        size: 15,
        text: s.raw,
        key: s.key,
        color: colorref(colour),
        x: tx,
        outline: mode,
    });
    vm.flags |= anmvm::F_VISIBLE;
}

// The 0x6A28-byte music-room object at 0x17CF4C8 (16 §5, structs.md). Zeroed at register: every VM is inert until
// a script is set (16 §5.2).
#[derive(Default, Clone)]
pub struct MusicRoom {
    entries: Vec<Entry>,
    // +0xC + 4i: heard flags copied from PLST+0x208 at init (15 §2, 16 §5.1).
    heard: [bool; MAX_ENTRIES],
    // +0x6C sub-frame, +0x70 state, +0x74 cursor, +0x78 playing entry (0 at init), +0x7C top row.
    sub_frame: i32,
    state: i32,
    cursor: i32,
    playing: i32,
    top: i32,
    // VM +0x88: music00.anm script 0.
    bg: AnmVm,
    // VM 0x32C + i·0x2A4: music00.anm script i + 1, one per entry.
    rows: Vec<AnmVm>,
    // VM 0x5508 + k·0x2A4: text.anm script 10 + k from the first reveal.
    comments: [AnmVm; COMMENT_LINES],
    // VM +0x6784 (8th VM of that array): playing title, text.anm script 0x11.
    title: AnmVm,
    // Surface slot 0: music.jpg.
    surface: Option<TexId>,
    // Calc returned 0 on the exit frame; the on-remove also removes the draw node (16 §5.3 step 1).
    removed: bool,
}

impl MusicRoom {
    fn n(&self) -> i32 {
        int(self.entries.len())
    }

    // Rows top..end shown by the draw, at y = 18·(i − top + 1) + 84 (16 §5.2).
    fn row_end(&self) -> i32 {
        (self.top + ROWS).min(self.n())
    }

    fn row_y(top: i32, i: i32) -> f32 {
        (18 * (i - top + 1) + 104 - 20) as f32
    }

    // The title is drawn at a forced pos and colour 0xFFFFFFFF, both restored after (16 §5.2).
    fn shown_title(&self) -> AnmVm {
        let mut t = self.title.clone();
        t.pos = [320.0, 52.0, 0.0];
        t.color = [255; 4];
        t
    }

    // Selected row interrupt 1, the others 2 (16 §5.3).
    fn interrupt_rows(&mut self) {
        for (i, vm) in self.rows.iter_mut().enumerate() {
            vm.pending_interrupt = if int(i) == self.cursor { 1 } else { 2 };
        }
    }

    fn interrupt_comments(&mut self, n: u16) {
        for vm in &mut self.comments {
            vm.pending_interrupt = n;
        }
    }

    // Music_PlayFile(entry[playing].file, 0) 0x447F21 (02 §4): marks flag 0 heard, not the chosen track, only in
    // music mode 1/2 and not in replay playback or demo. The Supervisor flag 13 `Bgm_StreamStart` restart is part of
    // `play_bgm`, which always restarts the stream.
    fn play(&mut self, ctx: &mut Ctx) {
        let Some(e) = self.entries.get(anmvm::table_index(self.playing)) else {
            return;
        };
        ctx.w.bgm.play_bgm(&String::from_utf8_lossy(&e.file));
        let g = &mut ctx.w.game;
        if matches!(g.cfg.music, 1 | 2) && g.flags & (gamemgr::F_REPLAY | gamemgr::F_DEMO) == 0 {
            g.score.plst.music[0] = 1;
        }
    }

    // Comment line k = 0..6 (16 §5.3 state 1 step 3): AnmFile_SetScript(text.anm 10 + k), interrupt 1; empty →
    // flag bit 1 cleared and nothing rendered, else bit 1 set and rendered.
    fn reveal(&mut self, ctx: &mut Ctx, k: usize) {
        let Some(e) = self.entries.get(anmvm::table_index(self.cursor)) else {
            return;
        };
        let c = anmvm::table_index(self.cursor);
        let s = comment_text(e, c, self.heard[c], self.cursor == self.playing, k);
        let vm = &mut self.comments[k];
        ctx.w
            .anm
            .file_set_script(&mut ctx.w.rng, vm, 0, COMMENT_SCRIPT + k);
        vm.pending_interrupt = 1;
        if s.raw.is_empty() {
            vm.flags &= !anmvm::F_ENABLED;
        } else {
            vm.flags |= anmvm::F_ENABLED;
            render_vm(ctx, vm, s, COMMENT_COLOUR, COMMENT_MODE);
        }
    }

    // MusicRoom_StateIntro 0x448A65 (16 §5.3).
    fn state_intro(&mut self) {
        if self.sub_frame == 0 {
            self.interrupt_rows();
            self.interrupt_comments(1);
        }
        if self.sub_frame > 7 {
            self.state = 1;
        }
    }

    // MusicRoom_StateInput 0x448B16 (16 §5.3): independent tests on the raw input word in this order; returns true on
    // exit (Ctrl, R and the sub-frame step are skipped).
    fn state_input(&mut self, ctx: &mut Ctx) -> bool {
        let inp = ctx.w.input;
        if inp.pressed_or_repeat(UP) {
            (self.cursor, self.top) = cursor_up(self.cursor, self.top, self.n());
            self.interrupt_rows();
            self.interrupt_comments(2);
            self.sub_frame = 0;
        }
        if inp.pressed_or_repeat(DOWN) {
            (self.cursor, self.top) = cursor_down(self.cursor, self.top, self.n());
            self.interrupt_rows();
            self.interrupt_comments(2);
            self.sub_frame = 0;
        }
        // Reveal at sub-frames 10, 12, …, 22; the sub-frame is always even here.
        let sf = self.sub_frame;
        if sf < 30 && (10..=22).contains(&sf) && sf % 2 == 0 {
            self.reveal(ctx, anmvm::table_index(sf - 10) / 2);
        }
        if mask_edge(&inp, SHOT | ENTER) {
            self.playing = self.cursor;
            self.play(ctx);
            self.sub_frame = 0;
            let c = anmvm::table_index(self.cursor);
            if let Some(e) = self.entries.get(c) {
                // Comment line 0 (`No.N <title>`) of the entry, as the comment it is (16 §5.3 step 4).
                let s = comment_text(e, c, true, true, 0);
                ctx.w
                    .anm
                    .file_set_script(&mut ctx.w.rng, &mut self.title, 0, TITLE_SCRIPT);
                self.title.pending_interrupt = 1;
                render_vm(ctx, &mut self.title, s, COMMENT_COLOUR, COMMENT_MODE);
            }
        }
        if mask_edge(&inp, BOMB | PAUSE) {
            // The loading banner and screen capture (0x44886F) are done by the Supervisor's 8 → 1 switch.
            ctx.w.game.request(crate::types::SceneId::Menu);
            return true;
        }
        if mask_edge(&inp, SKIP) {
            ctx.w.bgm.fade_bgm(8.0, ctx.w.anm.speed);
        }
        if mask_edge(&inp, KEY_R) {
            self.play(ctx);
        }
        self.sub_frame += 1;
        false
    }
}

impl MusicRoom {
    // MusicRoom_Init 0x44970F (16 §5.1–§5.2).
    pub(crate) fn on_register(&mut self, ctx: &mut Ctx, chain: Chain, _: u8) -> anyhow::Result<()> {
        if chain != Chain::Calc {
            return Ok(());
        }
        match ctx
            .archive
            .present()
            .read("music.jpg")
            .map_err(anyhow::Error::from)
            .and_then(|b| ctx.rec.load_image("music.jpg", &b))
        {
            Ok(t) => self.surface = Some(t),
            Err(e) => log::warn!("music.jpg: {e:#}"),
        }
        ctx.w
            .anm
            .load(&mut ctx.rec, &ctx.archive, ANM_SLOT, "music00.anm")?;
        self.entries = parse_musiccmt(&ctx.archive.read("musiccmt.txt")?);
        self.heard = heard_flags(&ctx.w.game.score.plst.music);
        ctx.w
            .anm
            .set_script(&mut ctx.w.rng, &mut self.bg, ANM_SLOT, 0);
        for i in 0..self.entries.len() {
            let mut vm = AnmVm::default();
            ctx.w
                .anm
                .file_set_script(&mut ctx.w.rng, &mut vm, ANM_SLOT, i + 1);
            vm.flags |= anmvm::F_ANCHOR_X | anmvm::F_ANCHOR_Y;
            vm.pos = [93.0, (18 * (int(i) + 1) + 104 - 20) as f32, 0.0];
            let (s, c, m) = row_text(&self.entries[i], i, self.heard[i]);
            render_vm(ctx, &mut vm, s, c, m);
            self.rows.push(vm);
        }
        Ok(())
    }

    // On-remove 0x449C0A.
    pub(crate) fn on_remove(&mut self, ctx: &mut Ctx, chain: Chain, _: u8) {
        if chain != Chain::Calc {
            return;
        }
        if let Some(t) = self.surface.take() {
            ctx.rec.free_texture(t);
        }
        ctx.w.anm.unload(&mut ctx.rec, ANM_SLOT);
    }

    // MusicRoom_Calc 0x449399 (16 §5.3): state, sub-frame step, then the VMs. On exit Calc returns 0 at once.
    pub(crate) fn calc(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        let s0 = self.state;
        if s0 == 0 {
            self.state_intro();
        } else if self.state_input(ctx) {
            self.removed = true;
            return Ret::Remove;
        }
        if self.state == s0 {
            self.sub_frame += 1;
        } else {
            self.sub_frame = 0;
        }
        self.bg.run(&mut ctx.w.anm, &mut ctx.w.rng);
        for vm in self
            .rows
            .iter_mut()
            .chain(&mut self.comments)
            .chain(std::iter::once(&mut self.title))
        {
            vm.run(&mut ctx.w.anm, &mut ctx.w.rng);
        }
        Ret::Next
    }

    // MusicRoom_Draw 0x449487 (16 §5.2).
    // Step: row VM positions, the ASCII queue strings and the VMs' draw writes; the emit draws in the same order.
    pub(crate) fn draw_step(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        if self.removed {
            return Ret::Remove;
        }
        let prep = |anm: &crate::anmmgr::AnmManager, vm: &mut AnmVm| {
            let mode = vm.draw_mode;
            anm.prepare(vm, mode);
        };
        prep(&ctx.w.anm, &mut self.bg);
        for i in self.top..self.row_end() {
            let y = Self::row_y(self.top, i);
            let vm = &mut self.rows[anmvm::table_index(i)];
            let argb = vm_argb(vm);
            vm.pos = [93.0, y, 0.0];
            prep(&ctx.w.anm, vm);
            if i == self.cursor {
                ctx.w
                    .ascii
                    .push(Text::new(93.0 - 60.0, y, CURSOR).argb(argb));
            }
            ctx.w
                .ascii
                .push(Text::new(48.0, y, format!("{:2}.", i + 1)).argb(argb));
        }
        for vm in &mut self.comments {
            prep(&ctx.w.anm, vm);
        }
        ctx.w
            .ascii
            .push(Text::new(320.0, 32.0, "Now Playing").argb(0xFFFF_FFFF));
        let mut title = self.shown_title();
        prep(&ctx.w.anm, &mut title);
        // Only the matrix cache survives the original's pos / colour restore.
        self.title.world_matrix = title.world_matrix;
        self.title.flags = title.flags;
        Ret::Next
    }

    pub(crate) fn emit(&self, ctx: &Ctx, rec: &mut Recorder, _: u8) {
        if self.removed {
            return;
        }
        rec.set_viewport(FULLSCREEN);
        if let Some(t) = self.surface {
            rec.image(t, 0.0, 0.0);
        }
        ctx.w.anm.emit(rec, &self.bg);
        for i in self.top..self.row_end() {
            ctx.w.anm.emit(rec, &self.rows[anmvm::table_index(i)]);
        }
        for vm in &self.comments {
            ctx.w.anm.emit(rec, vm);
        }
        ctx.w.anm.emit(rec, &self.shown_title());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_list_and_unlock() {
        let e = Entry {
            file: b"bgm/th08_03.mid".to_vec(),
            title: "蠢々秋月".as_bytes().into(),
            comments: vec![vec![b'x'; 68].into(), b"second".as_slice().into()],
        };
        let mut plst = [0u8; MAX_ENTRIES];
        plst[0] = 1;
        plst[3] = 1;
        let h = heard_flags(&plst);
        assert!(h[0] && !h[2] && h[3]);
        let (s, c, o) = row_text(&e, 2, h[2]);
        assert_eq!(
            (s.raw.decode().as_str(), c, o),
            ("？？？？？？？？", 0x0080_A0C0, 0x0010_0040)
        );
        let (s, c, o) = row_text(&e, 3, h[3]);
        assert_eq!((c, o), (0x00C0_E0FF, 0x0030_2080));
        assert_eq!(s.key, TextRef::MusicTitle { track: 3 });
        // Comments: real text if heard or playing (cut to 64 bytes), else the warning table.
        let t = |h, p, k| comment_text(&e, 3, h, p, k).raw.decode();
        let warning = |k| Builtin::MusicWarning(k).text();
        assert_eq!(t(true, false, 0), "x".repeat(64));
        assert_eq!(t(false, true, 1), "second");
        assert_eq!(t(true, false, 5), "");
        assert_eq!(t(false, false, 0), warning(0));
        assert_eq!(t(false, false, 1), warning(1));
    }

    #[test]
    fn cursor_window() {
        // Up below 0 → last entry with the last 10 rows shown; Down past the end → 0 / 0 (16 §5.3).
        assert_eq!(cursor_up(0, 0, 21), (20, 11));
        assert_eq!(cursor_up(0, 0, 5), (4, 0));
        assert_eq!(cursor_down(20, 11, 21), (0, 0));
        assert_eq!(cursor_down(9, 0, 21), (10, 1));
        assert_eq!(cursor_down(8, 0, 21), (9, 0));
        assert_eq!(cursor_up(5, 1, 21), (4, 1));
        assert_eq!(cursor_up(1, 1, 21), (0, 0));
    }

    #[test]
    fn key_edges_and_colours() {
        let inp = |held, prev| crate::input::Input {
            held,
            prev,
            ..Default::default()
        };
        // Multi-bit edge: releasing Enter while Shot is held also fires; holding both does not.
        assert!(mask_edge(&inp(SHOT, SHOT | ENTER), SHOT | ENTER));
        assert!(mask_edge(&inp(SHOT | ENTER, SHOT), SHOT | ENTER));
        assert!(!mask_edge(&inp(SHOT | ENTER, SHOT | ENTER), SHOT | ENTER));
        assert!(!mask_edge(&inp(0, SHOT), SHOT | ENTER));
        // COLORREF 0x00BBGGRR.
        assert_eq!(colorref(COMMENT_COLOUR), [192, 224, 255]);
        assert_eq!(colorref(0x00C0_E0FF), [255, 224, 192]);
        // Warning line 4 is one U+3000, not empty.
        let warning = |k| Builtin::MusicWarning(k).text();
        assert_eq!(warning(4), "\u{3000}");
        assert!((1..7).all(|k| warning(k).starts_with('\u{3000}')));
    }

    #[test]
    fn intro_and_input_sub_frames() {
        // Intro: state 1 on the 9th call (sub-frame 8), then the sub-frame restarts at 0 (16 §5.3).
        let mut m = MusicRoom::default();
        let mut calls = 0;
        while m.state == 0 {
            let s0 = m.state;
            m.state_intro();
            m.sub_frame = if m.state == s0 { m.sub_frame + 1 } else { 0 };
            calls += 1;
        }
        assert_eq!((calls, m.sub_frame), (9, 0));
    }
}
