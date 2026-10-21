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
// Warning table 0x4C7AAC, shown for a comment of a track not yet heard (16 §5.3).
// SPEC-GAP: 16 §5.3 gives only the first two of the seven lines; the rest are left empty.
const WARNING: [&str; COMMENT_LINES] = [
    "＊＊＊注意＊＊＊",
    "この曲はまだ、ゲーム中で流れておりません",
    "",
    "",
    "",
    "",
    "",
];

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

// Title-list text of entry i (16 §5.2): (text, colour, outline), colours 0xRRGGBB.
// SPEC-GAP: 16 §5.2 does not say whether the colours are 0xRRGGBB or a GDI COLORREF (0xBBGGRR); 0xRRGGBB assumed.
fn row_text(e: &Entry, heard: bool) -> (String, u32, u32) {
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

// Up / Down (16 §5.3): cursor ∓/± 1 wrapping over n entries; the 10-row window follows. Returns (cursor, top).
fn move_cursor(cursor: i32, top: i32, n: i32, d: i32) -> (i32, i32) {
    let c = (cursor + d).rem_euclid(n.max(1));
    let top = if c < top {
        c
    } else if c >= top + ROWS {
        c - ROWS + 1
    } else {
        top
    };
    (c, top)
}

fn rgb(c: u32) -> [u8; 3] {
    [(c >> 16) as u8, (c >> 8) as u8, c as u8]
}

// Text render into the VM's current sprite strip (10 §6 rasteriser, font 15), then visible.
// SPEC-GAP: the outline offset mode of the music-room text calls is not given; the ±2 px (outline 0) path is used.
fn render_vm(
    ctx: &mut Ctx,
    raster: &mut crate::text::Raster,
    vm: &mut AnmVm,
    s: &str,
    colour: u32,
    edge: u32,
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
    let (tx, ty) = (
        (sp.uv[0] * t.width as f32).round() as u32,
        (sp.uv[1] * t.height as f32).round() as u32,
    );
    let (w, h) = (sp.w as usize, sp.h as usize);
    let mut rgba = raster.render_coloured(s, rgb(colour), rgb(edge), w, 15, 0.0, 0);
    rgba.resize(w * h * 4, 0);
    ctx.gfx
        .write_texture(sp.tex, tx, ty, w as u32, h as u32, &rgba);
    vm.flags |= anmvm::F_VISIBLE;
}

// The 0x6A28-byte music-room object at 0x17CF4C8 (16 §5).
#[derive(Default)]
pub struct MusicRoom {
    entries: Vec<Entry>,
    // +0xC + 4i: heard flags copied from PLST+0x208 at init (15 §2, 16 §5.1).
    heard: [bool; MAX_ENTRIES],
    // +0x6C sub-frame, +0x70 state, +0x74 cursor, +0x7C top row.
    sub_frame: i32,
    state: i32,
    cursor: i32,
    top: i32,
    // SPEC-GAP: the initial playing entry is not given; 0 (the title theme the menu was playing).
    playing: i32,
    // VM +0x88: music00.anm script 0.
    bg: AnmVm,
    // VM 0x32C + i·0x2A4: music00.anm script i + 1, one per entry.
    rows: Vec<AnmVm>,
    // VM 0x5508 + k·0x2A4: text.anm script 10 + k.
    comments: Vec<AnmVm>,
    // VM +0x6784: playing title, text.anm script 0x11.
    title: AnmVm,
    raster: crate::text::Raster,
    // Surface slot 0: music.jpg.
    surface: Option<TextureId>,
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

    // Music_PlayFile(entry.file, 0) (02 §4, 16 §5.3): marks flag 0 heard, not the chosen track. If music mode is WAV the
    // stream is restarted from thbgm.dat, which `play_bgm` always does.
    fn play(&mut self, ctx: &mut Ctx) {
        let Some(e) = self.entries.get(self.playing as usize) else {
            return;
        };
        ctx.audio.play_bgm(&String::from_utf8_lossy(&e.file));
        // SPEC-GAP: whether flag 0 is also marked in music mode 0 is not given; it is here.
        ctx.game.score.plst.music[0] = 1;
    }

    // Comment line k = 0..6 on its VM: text.anm script 10 + k, interrupt 1; empty lines clear VM flag bit 1 (16 §5.3).
    fn reveal(&mut self, ctx: &mut Ctx, k: usize) {
        let Some(e) = self.entries.get(self.cursor as usize) else {
            return;
        };
        let c = self.cursor as usize;
        let s = comment_text(e, self.heard[c], self.cursor == self.playing, k);
        let vm = &mut self.comments[k];
        ctx.anm.set_script(vm, 0, COMMENT_SCRIPT + k);
        // SPEC-GAP: the comment colours are not given in 16 §5.3; Text_RenderToVm white with a black outline.
        render_vm(ctx, &mut self.raster, vm, &s, 0xFFFFFF, 0);
        vm.pending_interrupt = 1;
        if s.is_empty() {
            vm.flags &= !anmvm::F_ENABLED;
        }
    }

    fn calc_state(&mut self, ctx: &mut Ctx) {
        match self.state {
            0 => {
                if self.sub_frame == 0 {
                    self.interrupt_rows();
                    for vm in &mut self.comments {
                        vm.pending_interrupt = 1;
                    }
                }
                // SPEC-GAP: whether the sub-frame restarts on entering state 1 is not given; it keeps counting.
                if self.sub_frame + 1 >= 8 {
                    self.state = 1;
                }
            }
            _ => self.input(ctx),
        }
        // Comment reveal at sub-frames 10, 12, …, 22 (16 §5.3).
        let sf = self.sub_frame;
        if (10..=22).contains(&sf) && sf % 2 == 0 {
            self.reveal(ctx, (sf - 10) as usize / 2);
        }
        self.sub_frame += 1;
    }

    // State 1 keys (16 §5.3). SPEC-GAP: the order of the key tests is not given; the table order, one per frame.
    fn input(&mut self, ctx: &mut Ctx) {
        let inp = ctx.input;
        let d = if inp.pressed_or_repeat(UP) {
            -1
        } else if inp.pressed_or_repeat(DOWN) {
            1
        } else {
            0
        };
        if d != 0 {
            (self.cursor, self.top) = move_cursor(self.cursor, self.top, self.n(), d);
            self.interrupt_rows();
            for vm in &mut self.comments {
                vm.pending_interrupt = 2;
            }
            self.sub_frame = 0;
        } else if inp.pressed(SHOT | ENTER) {
            self.playing = self.cursor;
            self.play(ctx);
            if let Some(e) = self.entries.get(self.playing as usize) {
                let l = e.comments.first().map(Vec::as_slice).unwrap_or_default();
                let s = crate::text::decode_sjis(&l[..l.len().min(COMMENT_SHOWN)]);
                ctx.anm.set_script(&mut self.title, 0, TITLE_SCRIPT);
                render_vm(
                    ctx,
                    &mut self.raster,
                    &mut self.title,
                    &s,
                    0xFFE0C0,
                    0x300000,
                );
                self.title.pending_interrupt = 1;
            }
        // SPEC-GAP: 16 §5.3 does not say whether Ctrl is an edge or held; edge here (held would restart the fade).
        } else if inp.pressed(SKIP) {
            ctx.audio.fade_bgm(8.0);
        } else if inp.pressed(KEY_R) {
            self.play(ctx);
        } else if inp.pressed(BOMB | PAUSE) {
            // The loading banner and screen capture (0x44886F) are done by the Supervisor's 8 → 1 switch.
            ctx.game.request(gamemgr::SCENE_MENU);
        }
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
            ctx.anm.set_script(&mut vm, ANM_SLOT, i + 1);
            vm.flags |= anmvm::F_ANCHOR_X | anmvm::F_ANCHOR_Y;
            let (s, c, o) = row_text(&self.entries[i], self.heard[i]);
            render_vm(ctx, &mut self.raster, &mut vm, &s, c, o);
            self.rows.push(vm);
        }
        // SPEC-GAP: the comment VMs' state before the first reveal is not given; started and hidden like an empty line.
        self.comments = (0..COMMENT_LINES)
            .map(|k| {
                let mut vm = AnmVm::default();
                ctx.anm.set_script(&mut vm, 0, COMMENT_SCRIPT + k);
                vm.flags &= !anmvm::F_ENABLED;
                vm
            })
            .collect();
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

    // Calc 0x449399 (16 §5.3). SPEC-GAP: the VM run order is not given; after the state, as Result_Calc.
    fn calc(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        self.calc_state(ctx);
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

    // Draw 0x449487 (16 §5.2). SPEC-GAP: the draw order of the parts is not given.
    fn draw(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        ctx.gfx.set_viewport(FULLSCREEN);
        if let Some(t) = self.surface {
            ctx.gfx.image(t, 0.0, 0.0);
        }
        ctx.anm.draw(&mut ctx.gfx, &mut self.bg);
        let end = (self.top + ROWS).min(self.n());
        for i in self.top..end {
            let y = (18 * (i - self.top + 1) + 104 - 20) as f32;
            let vm = &mut self.rows[i as usize];
            vm.pos = [93.0, y, 0.0];
            ctx.anm.draw(&mut ctx.gfx, vm);
            ctx.ascii.push(Text::new(48.0, y, format!("{:2}.", i + 1)));
            if i == self.cursor {
                // SPEC-GAP: the cursor marker string is not given.
                ctx.ascii.push(Text::new(33.0, y, ">"));
            }
        }
        ctx.ascii.push(Text::new(320.0, 32.0, "Now Playing"));
        self.title.pos = [320.0, 52.0, 0.0];
        ctx.anm.draw(&mut ctx.gfx, &mut self.title);
        for vm in &mut self.comments {
            ctx.anm.draw(&mut ctx.gfx, vm);
        }
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
        // Wraps both ways; the 10-row window follows.
        assert_eq!(move_cursor(0, 0, 21, -1), (20, 11));
        assert_eq!(move_cursor(20, 11, 21, 1), (0, 0));
        assert_eq!(move_cursor(9, 0, 21, 1), (10, 1));
        assert_eq!(move_cursor(5, 1, 21, -1), (4, 1));
        assert_eq!(move_cursor(1, 1, 21, -1), (0, 0));
    }
}
