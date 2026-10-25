//! Music room `MusicRoom_Register` 0x449254, scene 8 (16 §5.1–§5.3): musiccmt.txt list, heard flags, track selection.

use crate::anmvm::{self, AnmVm};
use crate::ascii::Text;
use crate::game::Ctx;
use crate::gamemgr;
use crate::gfx::{FULLSCREEN, TextureId};
use crate::input::{BOMB, DOWN, ENTER, KEY_R, PAUSE, SHOT, SKIP, UP};
use crate::task::{Chain, Ret, Task};

// Calc 0x449399 at prio 4, draw 0x449487 at prio 3 (16 §5).
pub const CALC_PRIO: u16 = 4;
pub const DRAW_PRIO: u16 = 3;
// music00.anm in ANM slot 0x17 (16 §5.1).
const ANM_SLOT: usize = 23;
// Entry array 32 × 0x292 (16 §5.1).
const MAX_ENTRIES: usize = 32;
const COMMENT_LINES: usize = 7;
// Comment display copies 64 bytes of a line into a zeroed 0x42 buffer (16 §5.1 QUIRK: longer lines are cut).
const COMMENT_SHOWN: usize = 64;
// The draw shows 10 rows (16 §5.2).
const ROWS: i32 = 10;
// text.anm (slot 0) scripts: comment line k = 10 + k, playing title 0x11 (16 §5.3).
const COMMENT_SCRIPT: usize = 10;
const TITLE_SCRIPT: usize = 0x11;
const UNHEARD: &str = "？？？？？？？？";
// Warning table 0x4C7AAC, shown for a comment of a track neither heard nor playing (16 §5.3); U+3000 is part of the
// strings, line 4 is a single U+3000 (not empty, so rendered).
const WARNING: [&str; COMMENT_LINES] = [
    "＊＊＊注意＊＊＊",
    "　この曲はまだ、ゲーム中で流れておりません",
    "　自分でプレイする前に聴きたくない、コメントを見たくない、",
    "　という方は一刻も早くここから立ち去る事をお奨めします。",
    "　",
    "　（なお、ここで決定ボタンを押せば、他の曲と同様、",
    "　　曲の再生とコメント表示が行われますので、注意）",
];
// Comment / title text colour, COLORREF 0xFFE0C0 = RGB (192, 224, 255), and mode arg 0x300000 (16 §5.3).
const COMMENT_COLOUR: u32 = 0xFFE0C0;
const COMMENT_MODE: i32 = 0x300000;
// The one-byte cursor string 0x7F: ascii.anm sprite 126, a ▶ glyph (16 §5.2, 12 §2.9).
const CURSOR: &str = "\u{7f}";

// One musiccmt.txt entry (0x292 bytes, 16 §5.1): Shift-JIS bytes as stored.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Entry {
    // +0x00 file name after `@` (`bgm/…mid`).
    pub file: Vec<u8>,
    // +0x40 title.
    pub title: Vec<u8>,
    // +0x82 + k·0x42, up to 7 comment lines.
    pub comments: Vec<Vec<u8>>,
}

fn is_eol(c: u8) -> bool {
    c == b'\r' || c == b'\n'
}

// musiccmt.txt parsing (16 §5.1): skip to `@`, file name, title line, then up to 7 lines until the next `@`; CR/LF
// runs after each line are skipped, so blank lines are dropped. The array holds 32 entries; the original's copies
// are unbounded, extra entries are dropped here.
pub fn parse_musiccmt(data: &[u8]) -> Vec<Entry> {
    let mut p = 0;
    let line = |p: &mut usize| {
        let start = *p;
        while *p < data.len() && !is_eol(data[*p]) {
            *p += 1;
        }
        let s = data[start..*p].to_vec();
        while *p < data.len() && is_eol(data[*p]) {
            *p += 1;
        }
        s
    };
    let mut out = Vec::new();
    while out.len() < MAX_ENTRIES {
        while p < data.len() && data[p] != b'@' {
            p += 1;
        }
        if p >= data.len() {
            break;
        }
        p += 1;
        let file = line(&mut p);
        let title = line(&mut p);
        let mut comments = Vec::new();
        while comments.len() < COMMENT_LINES && p < data.len() && data[p] != b'@' {
            comments.push(line(&mut p));
        }
        out.push(Entry {
            file,
            title,
            comments,
        });
    }
    out
}

// PLST+0x208 heard flags → object +0xC + 4i (16 §5.1); playing in the music room never sets them.
fn heard_flags(plst: &[u8; MAX_ENTRIES]) -> [bool; MAX_ENTRIES] {
    plst.map(|f| f != 0)
}

// Title-list text of entry i (16 §5.2): (text, COLORREF colour, mode arg).
fn row_text(e: &Entry, heard: bool) -> (String, u32, i32) {
    if heard {
        (crate::text::decode_sjis(&e.title), 0xC0E0FF, 0x302080)
    } else {
        (UNHEARD.to_string(), 0x80A0C0, 0x100040)
    }
}

// Comment line k of the selected entry (16 §5.3): the real comment if heard or playing, else the warning; cut to
// 64 bytes.
fn comment_text(e: &Entry, heard: bool, playing: bool, k: usize) -> String {
    if heard || playing {
        let l = e.comments.get(k).map(Vec::as_slice).unwrap_or_default();
        crate::text::decode_sjis(&l[..l.len().min(COMMENT_SHOWN)])
    } else {
        WARNING[k].to_string()
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
    let c = vm.color;
    (c[anmvm::A] as u32) << 24
        | (c[anmvm::R] as u32) << 16
        | (c[anmvm::G] as u32) << 8
        | c[anmvm::B] as u32
}

// Text_RenderToVm (10 §6), font 15: the rasteriser gets x = u0 and the texture-row width and replaces the whole
// 15-row strip; the outline is always black, `mode` only selects ±1 (−1) / ±2 px. Sets VM flag bit 0.
fn render_vm(
    ctx: &mut Ctx,
    raster: &mut crate::text::Raster,
    vm: &mut AnmVm,
    s: &str,
    colour: u32,
    mode: i32,
) {
    let slot = vm.anm_slot.max(0) as usize;
    let Some(sp) = ctx
        .anm
        .sprite(slot, vm.sprite_index.max(0) as usize)
        .copied()
    else {
        return;
    };
    let t = ctx.gfx.texture(sp.tex);
    let (w, tx, ty) = (
        t.width,
        (sp.uv[0] * t.width as f32).round(),
        (sp.uv[1] * t.height as f32).round() as u32,
    );
    let size = 15;
    let rgba = raster.render_outline(s, colorref(colour), w as usize, size, tx, mode);
    ctx.gfx.write_texture(sp.tex, 0, ty, w, size as u32, &rgba);
    vm.flags |= anmvm::F_VISIBLE;
}

// The 0x6A28-byte music-room object at 0x17CF4C8 (16 §5, structs.md). Zeroed at register: every VM is inert until
// a script is set (16 §5.2).
#[derive(Default)]
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
    raster: crate::text::Raster,
    // Surface slot 0: music.jpg.
    surface: Option<TextureId>,
    // Calc returned 0 on the exit frame; the on-remove also removes the draw node (16 §5.3 step 1).
    removed: bool,
}

impl MusicRoom {
    pub fn new() -> Self {
        Self::default()
    }

    fn n(&self) -> i32 {
        self.entries.len() as i32
    }

    // Selected row interrupt 1, the others 2 (16 §5.3).
    fn interrupt_rows(&mut self) {
        for (i, vm) in self.rows.iter_mut().enumerate() {
            vm.pending_interrupt = if i as i32 == self.cursor { 1 } else { 2 };
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
        let Some(e) = self.entries.get(self.playing as usize) else {
            return;
        };
        ctx.audio.play_bgm(&String::from_utf8_lossy(&e.file));
        let g = &mut ctx.game;
        if matches!(g.cfg.music, 1 | 2) && g.flags & (gamemgr::F_REPLAY | gamemgr::F_DEMO) == 0 {
            g.score.plst.music[0] = 1;
        }
    }

    // Comment line k = 0..6 (16 §5.3 state 1 step 3): AnmFile_SetScript(text.anm 10 + k), interrupt 1; empty →
    // flag bit 1 cleared and nothing rendered, else bit 1 set and rendered.
    fn reveal(&mut self, ctx: &mut Ctx, k: usize) {
        let Some(e) = self.entries.get(self.cursor as usize) else {
            return;
        };
        let c = self.cursor as usize;
        let s = comment_text(e, self.heard[c], self.cursor == self.playing, k);
        let vm = &mut self.comments[k];
        ctx.anm.file_set_script(vm, 0, COMMENT_SCRIPT + k);
        vm.pending_interrupt = 1;
        if s.is_empty() {
            vm.flags &= !anmvm::F_ENABLED;
        } else {
            vm.flags |= anmvm::F_ENABLED;
            render_vm(ctx, &mut self.raster, vm, &s, COMMENT_COLOUR, COMMENT_MODE);
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
        let inp = ctx.input;
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
            self.reveal(ctx, (sf - 10) as usize / 2);
        }
        if mask_edge(&inp, SHOT | ENTER) {
            self.playing = self.cursor;
            self.play(ctx);
            self.sub_frame = 0;
            if let Some(e) = self.entries.get(self.cursor as usize) {
                let l = e.comments.first().map(Vec::as_slice).unwrap_or_default();
                let s = crate::text::decode_sjis(&l[..l.len().min(COMMENT_SHOWN)]);
                ctx.anm.file_set_script(&mut self.title, 0, TITLE_SCRIPT);
                self.title.pending_interrupt = 1;
                render_vm(
                    ctx,
                    &mut self.raster,
                    &mut self.title,
                    &s,
                    COMMENT_COLOUR,
                    COMMENT_MODE,
                );
            }
        }
        if mask_edge(&inp, BOMB | PAUSE) {
            // The loading banner and screen capture (0x44886F) are done by the Supervisor's 8 → 1 switch.
            ctx.game.request(gamemgr::SCENE_MENU);
            return true;
        }
        if mask_edge(&inp, SKIP) {
            ctx.audio.fade_bgm(8.0);
        }
        if mask_edge(&inp, KEY_R) {
            self.play(ctx);
        }
        self.sub_frame += 1;
        false
    }
}

impl Task<Ctx> for MusicRoom {
    // MusicRoom_Init 0x44970F (16 §5.1–§5.2).
    fn on_register(&mut self, ctx: &mut Ctx, chain: Chain, _: u8) -> anyhow::Result<()> {
        if chain != Chain::Calc {
            return Ok(());
        }
        match ctx
            .archive
            .read("music.jpg")
            .and_then(|b| ctx.gfx.load_image("music.jpg", &b))
        {
            Ok(t) => self.surface = Some(t),
            Err(e) => log::warn!("music.jpg: {e:#}"),
        }
        ctx.anm
            .load(&mut ctx.gfx, &ctx.archive, ANM_SLOT, "music00.anm")?;
        self.entries = parse_musiccmt(&ctx.archive.read("musiccmt.txt")?);
        self.heard = heard_flags(&ctx.game.score.plst.music);
        ctx.anm.set_script(&mut self.bg, ANM_SLOT, 0);
        for i in 0..self.entries.len() {
            let mut vm = AnmVm::default();
            ctx.anm.file_set_script(&mut vm, ANM_SLOT, i + 1);
            vm.flags |= anmvm::F_ANCHOR_X | anmvm::F_ANCHOR_Y;
            vm.pos = [93.0, (18 * (i as i32 + 1) + 104 - 20) as f32, 0.0];
            let (s, c, m) = row_text(&self.entries[i], self.heard[i]);
            render_vm(ctx, &mut self.raster, &mut vm, &s, c, m);
            self.rows.push(vm);
        }
        Ok(())
    }

    // On-remove 0x449C0A.
    fn on_remove(&mut self, ctx: &mut Ctx, chain: Chain, _: u8) {
        if chain != Chain::Calc {
            return;
        }
        if let Some(t) = self.surface.take() {
            ctx.gfx.free_texture(t);
        }
        ctx.anm.unload(&mut ctx.gfx, ANM_SLOT);
    }

    // MusicRoom_Calc 0x449399 (16 §5.3): state, sub-frame step, then the VMs. On exit Calc returns 0 at once.
    fn calc(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        let s0 = self.state;
        if s0 == 0 {
            self.state_intro();
        } else if self.state_input(ctx) {
            self.removed = true;
            return Ret::Remove;
        }
        if self.state != s0 {
            self.sub_frame = 0;
        } else {
            self.sub_frame += 1;
        }
        self.bg.run(&mut ctx.anm);
        for vm in self
            .rows
            .iter_mut()
            .chain(&mut self.comments)
            .chain(std::iter::once(&mut self.title))
        {
            vm.run(&mut ctx.anm);
        }
        Ret::Next
    }

    // MusicRoom_Draw 0x449487 (16 §5.2).
    fn draw(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        if self.removed {
            return Ret::Remove;
        }
        ctx.gfx.set_viewport(FULLSCREEN);
        if let Some(t) = self.surface {
            ctx.gfx.image(t, 0.0, 0.0);
        }
        ctx.anm.draw(&mut ctx.gfx, &mut self.bg);
        let end = (self.top + ROWS).min(self.n());
        for i in self.top..end {
            let y = (18 * (i - self.top + 1) + 104 - 20) as f32;
            let vm = &mut self.rows[i as usize];
            let argb = vm_argb(vm);
            vm.pos = [93.0, y, 0.0];
            ctx.anm.draw(&mut ctx.gfx, vm);
            if i == self.cursor {
                ctx.ascii.push(Text::new(93.0 - 60.0, y, CURSOR).argb(argb));
            }
            ctx.ascii
                .push(Text::new(48.0, y, format!("{:2}.", i + 1)).argb(argb));
        }
        for vm in &mut self.comments {
            ctx.anm.draw(&mut ctx.gfx, vm);
        }
        ctx.ascii
            .push(Text::new(320.0, 32.0, "Now Playing").argb(0xFFFF_FFFF));
        // The title is drawn at a forced pos and colour 0xFFFFFFFF, both restored after.
        let (pos, color) = (self.title.pos, self.title.color);
        self.title.pos = [320.0, 52.0, 0.0];
        self.title.color = [255; 4];
        ctx.anm.draw(&mut ctx.gfx, &mut self.title);
        (self.title.pos, self.title.color) = (pos, color);
        Ret::Next
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn musiccmt_parse() {
        let mut d = b"junk\r\n@bgm/th08_01.mid\r\nTitle A\r\n\r\nc0\r\nc1\r\n".to_vec();
        d.extend_from_slice(
            b"@bgm/th08_00.mid\nTitle B\n1\n2\n3\n4\n5\n6\n7\n8\n@bgm/x.mid\r\nT\r\n",
        );
        let e = parse_musiccmt(&d);
        assert_eq!(e.len(), 3);
        assert_eq!(e[0].file, b"bgm/th08_01.mid");
        assert_eq!(e[0].title, b"Title A");
        // The blank line is dropped.
        assert_eq!(e[0].comments, vec![b"c0".to_vec(), b"c1".to_vec()]);
        // At most 7 comment lines; the 8th is skipped on the way to the next `@`.
        assert_eq!(e[1].comments.len(), 7);
        assert_eq!(e[1].comments[6], b"7");
        assert_eq!(e[2].file, b"bgm/x.mid");
        assert!(e[2].comments.is_empty());
        // The array holds 32 entries.
        assert_eq!(parse_musiccmt(&b"@f\nt\n".repeat(40)).len(), 32);
    }

    #[test]
    fn title_list_and_unlock() {
        let e = Entry {
            file: b"bgm/th08_03.mid".to_vec(),
            title: "蠢々秋月".as_bytes().to_vec(),
            comments: vec![vec![b'x'; 68], b"second".to_vec()],
        };
        let mut plst = [0u8; MAX_ENTRIES];
        plst[0] = 1;
        plst[3] = 1;
        let h = heard_flags(&plst);
        assert!(h[0] && !h[2] && h[3]);
        let (s, c, o) = row_text(&e, h[2]);
        assert_eq!((s.as_str(), c, o), (UNHEARD, 0x80A0C0, 0x100040));
        let (_, c, o) = row_text(&e, h[3]);
        assert_eq!((c, o), (0xC0E0FF, 0x302080));
        // Comments: real text if heard or playing (cut to 64 bytes), else the warning table.
        assert_eq!(comment_text(&e, true, false, 0), "x".repeat(64));
        assert_eq!(comment_text(&e, false, true, 1), "second");
        assert_eq!(comment_text(&e, true, false, 5), "");
        assert_eq!(comment_text(&e, false, false, 0), WARNING[0]);
        assert_eq!(comment_text(&e, false, false, 1), WARNING[1]);
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
        assert_eq!(colorref(0xC0E0FF), [255, 224, 192]);
        // Warning line 4 is one U+3000, not empty.
        assert_eq!(WARNING[4], "\u{3000}");
        assert!(WARNING[1..7].iter().all(|l| l.starts_with('\u{3000}')));
    }

    #[test]
    fn intro_and_input_sub_frames() {
        // Intro: state 1 on the 9th call (sub-frame 8), then the sub-frame restarts at 0 (16 §5.3).
        let mut m = MusicRoom::new();
        let mut calls = 0;
        while m.state == 0 {
            let s0 = m.state;
            m.state_intro();
            m.sub_frame = if m.state != s0 { 0 } else { m.sub_frame + 1 };
            calls += 1;
        }
        assert_eq!((calls, m.sub_frame), (9, 0));
    }
}
