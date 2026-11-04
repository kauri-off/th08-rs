# I18N_QUESTIONS — open points from run G0

These are questions the numbered specs do not answer, raised during the inventory (`I18N_STRINGS.md`). Each one says
what evidence would settle it. None of them blocks G1. Q3 and Q4 affect the G2/G3 design.

## Q1 — What does `front.anm` entry #1 `ename.png` show?

thcrap (`script_latin`, lang_ru) replaces `data/front/ename.png`, so it contains text. 12 §5.2 lists the front.anm
layout by sprite but does not name the entry-#1 sprites or say what they show. Candidates: the boss name / "Enemy"
label of the boss bar (12 §5.4) or the markers (12 §5.10).
**Evidence:** the `anmdis`/`anm_index` sprite table of front.anm entry #1, the scripts / HUD code that set those
sprites, and a look at the image. Then add the entry to the 12 §5.2 table.

## Q2 — Which RU-only images carry text, and where are they shown?

lang_ru replaces `etama/etama3.png` (etama.anm #2, the bullet sheet), `face/face_cdbg.png` (cut-in streak strips,
12 §5.8), `title/title01.png` (title01.anm #1), `title00.png` and `th08logo.jpg` (03 §1). The specs do not say that any
of these contain text, and `title00.png` (03 §1: "loaded as `title/title00.png` by the title screen") has no screen
section.
**Evidence:** the sprite list of each entry, plus the code site and screen that draw `title00.png`. If an image has
text, add it to the spec section of its screen. If not, the RU pack is restyling, not translation, and G3 can import
the images unchanged.

## Q3 — Is there a free code range in `ascii.png` for non-Latin glyphs?

ASCII strings (12 §2.9) map byte c to ascii.anm sprite c − 1. Bytes 0x7F–0x81 and sprites 0x88+ are already in use
(▶, SP/END, digits, 12 §2.9 / §5.8 / 16 §2.2). ascii.anm entry #0 has 160 sprites and #1 has 108. The specs do not
list which sprite indices are unused. So RU ascii strings (clear panel, practice / replay headers, popups) cannot be
mapped to Cyrillic cells without such a list.
**Evidence:** the full sprite table of ascii.anm #0/#1 (index → rect) and a scan of every `Ascii_Printf` /
`AnmVm_SetSprite` on ascii.anm (or a texture dump) for indices that are never used. Then check whether lang_ru's
`ascii.png`/`asciis.png` replace Latin cells or fill unused ones. lang_ru is not in the local thcrap install;
`srv.thpatch.net/lang_ru/th08/data/ascii/` is needed for this.

## Q4 — Does a score.dat written by thcrap-patched th08 contain translated card names?

15 §2 (CATK): `Spell_Start` strcpy's the card name from the Spell object into CATK +0x10. The checksum byte covers the
name bytes up to the NUL, and a mismatch zeroes all 78 counters of that card. thcrap's `spell_name` breakpoint
(`<thcrap>/nmlgc/base_tsa/th08.js`) replaces the name at `Spell_Start`. If it does so **before** the strcpy, a
score.dat from a thcrap player holds English names with matching checksums. On that player's first attempt of the
card in the port (Japanese name written), the checksum still matches, because the new name is used for both the sum and
the store (step 4 recomputes n from the name just written). But the display paths (spell practice rows, result card
list, spell replay names) would show the English bytes as Shift-JIS until then.
**Evidence:** the address of thcrap's `spell_name` breakpoint (th08.v1.00d.js) against the instruction order of
`Spell_Start` 0x4152A0 (15 §2 steps 1–4), or a score.dat produced under thcrap. Spec impact: none for logic (L1 holds).
G1 must not display CATK names directly when a pack is active. It must look them up by card id. The spec should state
that the CATK name is only a cache of the ECL name.

## Q5 — Can `Text_RenderToVm` formatting reach translated text?

10 §6: every entry point `vsprintf`s its text into a 128- or 72-byte buffer. Retail msg/end/musiccmt contain no `%`
(16 §5.2 states it for musiccmt). For msg and `.end` text, the specs do not say whether a `%` in the data would be
formatted. Ending text is copied pair-wise, not through `Text_RenderToVm`'s format (16 §1.3 says "the line is drawn
with `Text_RenderToVm(…, text)`").
**Evidence:** the argument order at the msg (6 sites) and ending call sites of 0x4663B0: is the data string the format
or a `%s` argument? Design impact is small. Translations are rendered from the pack without formatting, but the
vanilla fallback path must stay byte-exact.

## Q6 — Do the four `@` text textures ever reach logic through VM state?

10 §6 says only the rows of the strip are replaced, and that `Text_RenderToVm` "finally sets VM flag bit 0"
(visible) for **every** call. The port's `menulist::render_strip` and `gui`/`hud` flush paths do not set the flag.
`ending.rs`/`musicroom.rs` do. The flag is VM state (draw and calc gating, 05). It does not depend on content. Still,
a pack must not suppress a vanilla call (for example "translation empty → skip") because that would change it.
**Evidence:** confirm in 05 / 10 §6 whether flag bit 0 is read by any calc path (AnmVm_Run gating) and whether the
port's omission at those sites is a draw-parity bug. This is outside I18N. It should go to the next IMPL brief if it is
real.

## Q7 — Is the music-room "Now Playing" line always comment line 0?

16 §5.3 step 4 uses comment line 0 of entry[cursor] (the retail `No.N <title>` line). thcrap rebuilds it from
`themes.js` + `Music Room Numbered Title` (`No. <r$%2d$99>  %s`). The spec does not say whether the retail title row
(+0x40) and comment line 0 always agree (for example the `No.N` numbering vs entry index + 1, and entries whose
comment line 0 is not a title).
**Evidence:** `re/tools/musiccmt.py` over the retail `musiccmt.txt`: for each of the 21 entries, compare comment line 0
with `No.<i+1> <title>`. Record the result in 16 §5.1. G3 can then generate line 0 from the translated title.

## Q8 — Is the result name-entry shot title (16 §2.2) ever visible?

`Result_NameEntry` frame 0 renders the shot name (table 0x4C7F4C) with `Text_RenderCentered` into text VM +0xBFC0.
16 §2.4 says text VMs are drawn only in states 8 and 0x13–0x15 after a re-render, so it looks invisible. The port does
not record this call. That is fine for display, but the `@` strip content differs between frame 0 and the first
re-render.
**Evidence:** a draw log of the original covering the frames of name entry (state 9), checking for any draw of text
VM 0 / text.anm sprite 21 before state 8. If it is never drawn, add "not visible" to 16 §2.2.

## Q7 — `menulist::render_strip` never sets VM flag bit 0 (found in G1)

Closed in G4-A: `render_strip` sets bit 0 on its target VM after drawing (10 §6); no draw change on vanilla data.

10 §6: `Text_RenderToVm` always sets bit 0 (visible) on the target VM. The port's ending and music-room sites do; HUD
and gui sites get it from `set_script` just before. `menulist::render_strip` (spell practice, replay, result) never
touches its scratch VM, so if an ANM script cleared bit 0 on that VM before a re-render, the port would not show the
strip where the original would. Not an i18n issue: a draw-parity item for the next IMPL brief (vanilla data shows no
difference in the full draw check).
