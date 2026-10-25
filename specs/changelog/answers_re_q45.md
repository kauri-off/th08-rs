# RE-Q45 — IMPL109 questions 1–3, IMPL110 question 1 (text centring, strip rows, music-room reveal, キャラ名 length)

Code (decompile + capstone disasm of th08.exe, .rdata reads) and one original run under Xvfb with a proxy d3d8.dll that
dumps every 512-wide A4R4G4B4 texture at creation and every 100 presents (re/work/runs/q45/tex/: tex.c, run.sh).

| # | question | original | spec | port |
|---|---|---|---|---|
| 1 (IMPL109) | `Text_RenderCentered` x: float or integer `len·15/4`? | 0x4666E3–0x466719: `fld sw; fmul sx; fdiv [0x4B42EC]=2.0; fadd u0`, `fild qword len; fimul fs; fmul sx; fdiv [0x4B4308]=4.0; fsubp; call ftol 0x4A3E70` (truncation). Fraction kept: len 5 → 237.25 → 237. | 10 §6 (Centered line), 12 §2.2 | right (`menu.rs` `centred_x`, f32 then trunc; exact in f32) |
| 2 (IMPL109) | rows 15–16 of the 17-row text strips are never written | Write height trunc(fs2·sy) = 15; strip pitch 18 (text/resulttext) / 17 (music00), so no write reaches rows Y+15, Y+16. `@` textures: `Anm_CreateBlankTexture` 0x4657C0 → D3DXCreateTexture 0x47AA8D (MANAGED, no fill). Dump: text.anm texture all 0 at creation (present 9); at presents 500–1500 (top menu, descriptions rendered) rows 15–17, 33–35, … still all 0. | 10 §6 (new paragraph after the rect rule) | right to leave them; `menulist.rs` `render_strip` zero-padding rows 15–16 is equivalent (they are 0). Its W = sprite width (320) instead of 512 is the RE-Q44 CODE BUG, still open. |
| 3 (IMPL109) | 16 §5.3 reveal test for odd sub-frames | `MusicRoom_StateInput` 0x448B16: `if (sub < 30)` then an if/else-if chain `sub == 10 → 0, == 12 → 1, … == 22 → 6`; anything else → no line. | 16 §5.3 step 3 reworded | right (`musicroom.rs` `(10..=22).contains && %2 == 0` is the same set) |
| 4 (IMPL110) | キャラ名 is "10 full-width characters"? | Table 0x4C7F4C[0..11]: every string is 16 bytes = 8 full-width chars incl. U+3000 padding (e.g. 0x4B7250 `90BC 8D73 8E9B 8140 9748 8158 8E71 8140` = 西行寺　幽々子　). | 15 §3.5 "8 full-width characters = 16 bytes" | right (`menulist.rs` `SHOT_NAMES`) |

CODE BUG list: none new. Still open from RE-Q44: `menulist.rs` `render_strip` (also used by `result.rs`) writes W =
sprite width instead of W = 512 (10 §6), so columns ≥ the sprite width keep stale text.
