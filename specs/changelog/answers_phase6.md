# Answers, Phase 6

## 1. Open items in docs/QUESTIONS.md

answers_phase5.md already closed every item listed in QUESTIONS.md, so none were left for this phase.
The Phase 6 deliverables resolved these further loose ends:

| item | resolution |
|---|---|
| ending fade and selection | 16 §1–3 |
| 12 §2.6 spell-practice details (were INFERRED) | 16 §6 |
| score.dat, replay and cfg formats with validators | 15 |

## 2. Who sets spell flags 10 and 11 (spell object g_Gui 0x4EA670, flag word +0) — VERIFIED

### Bit 10 (0x400): name-only declaration

**Set by** `Spell_ShowNameOnly` 0x415D10. It sets bit 10, clears bit 0 (active) and bit 4, and shows the
name banner (0x415F00, mode 1).

**Caller:** only ECL native 22 (0x424FF0), which passes the string at 0x4B495C 「リザレクション」.
The data uses it in:
- `ecldata8.ecl` sub 88;
- `ecldata8sp.ecl` sub 42 (Extra, Mokou's resurrection).

Native 23 → `Spell_HideName` 0x416160 removes the banner.

**Effect:**
- The banner's second VM (+0x2374) is not interrupted.
- `Spell_Draw` 0x4178C0 skips the bonus and capture-history display.
- There is no bonus, no capture and no card timer.

**Cleared by** `Spell_Start` 0x4152A0.

### Bit 11 (0x800): bonus frozen

**Set by** `Spell_SetBonusFrozen` 0x41F0E0 (bit 11 := n & 1). Its only caller is ECL op 184 (0xB8,
`spell_ex`). All 82 uses in the data are `spell_ex 1`, placed at the start of boss death subs, i.e. the
subs that set death mode/sub and turn flags off (for example `ecldata1` sub 50).

**Effect:**
- `Spell_Calc` 0x416B90 no longer decays the bonus.
- `Spell_AddBonus` 0x416B10 ignores time-orb pickups (normally +8000 per orb, called from 0x4412B0).

So the bonus stays fixed while the boss's death animation plays.

**Cleared by** `Spell_Start` and `Spell_End`.

### Related

Bit 6 is set by op 164 through `Spell_SetFlag6` 0x41F0B0 (bit 6 := n & 1).

## 3. Glyphs 0xD–0xF in large score popups — VERIFIED

`Popup_Large` 0x403460 and `Popup_LargeStatic` 0x403600 store glyphs least significant first:

```
if p4 > 0: 0xF, digits of p4 (LSB first), 0xE
digits of p3 (LSB first; "0" if zero)
0xD
count = number of glyphs
```

The sprite is glyph + 0x88: the ascii.png strip at y = 208, sprites 136–152, 8×12 each. Rendering that
strip (`tools/anm.py extracted/dat/ascii.anm --png`) gives `0123456789%.-+()▼`:

| glyph | character |
|---|---|
| 0–9 | digits |
| 0xA | % |
| 0xB | . |
| 0xC | - |
| **0xD** | **+** |
| **0xE** | **(** |
| **0xF** | **)** |
| 0x10 | ▼ |

So the popup reads **"+p3"**, or **"+p3(p4)"** when p4 > 0. Callers and colours are in answers_impl9 §14.

Draw order — VERIFIED (popup draw 0x405420, prio 14): the routine walks from the last stored glyph to the
first, left to right. x starts at pos.x − count·3.5 and advances 7·popup scale.x per glyph; the start
offset is not scaled, so popups at scale 2 are off-centre. That puts '+' first, then p3 from its most
significant digit, then '(', p4, ')'. Alpha uses the same player-distance rule as the small popups.

## 4. Corrections made to earlier specs in Phase 6 (in place)

| spec | change |
|---|---|
| 11 §6 | replay stage record +0x00 = score ÷ 10 at the **end** of that stage (was "at start") |
| 11 §1.5 | RNG table: replay save = 3 draws; score.dat save = 2 draws (also the mode-2 flushes) |
| 11 §3.2 | `GameMgr_LoadScoreData` first flushes score.dat (2 RNG draws) |
| 12 §2.6 | pointer to 16 §6, which replaces the INFERRED details |
| 07 §11.1 | "bit 10 / 11 display flags" replaced by the meanings above; bit 6 / op 164 noted |
| 07 op 184 | "spell manager 0x41F0E0(n)" → spell flag bit 11 (bonus frozen) := n & 1 |
| 07 op 164 | → spell flag bit 6 := n & 1 |
| 07 §12 native 22/23 | name-only declaration / hide banner |
| 13 §4 | large-popup format and characters; callers |
| 13 §6 | glyph open point closed |
| 15 (entry 214, Last Word table) | "shot 6 (Marisa solo; matches the in-game hint)" |
| NOTES | PLST has 7 blocks (5 = unused, 6 = total), not 6 |

The corrections from the impl9 pass (05, 12, 13) are listed at the end of answers_impl9.md.

## 5. Still open or INFERRED (from 15 / 16)

- Fps flag 0x17CEA90 has no writer (always 0).
- Supervisor+0x1A4 bit 3 has no writer.
- CLRD +0x20 has no writer.
- Info panel +0x69 = 100 is unused.
- Byte 0x164D0B2 is presumably always 0.
- Ending fade quad colour: INFERRED.
- cfg +0x26 has no gameplay reader.
- All of these were found with absolute-address scans. **A write through a struct base pointer would not be
  found** (example: the 0x4EA3F4 correction in answers_impl9 §8). They should be re-checked with a
  struct-relative search if any of them matters.
