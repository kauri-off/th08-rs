# IMPL27 questions (comment pass batch D: menus / files / audio / input / frame loop; tools)

Answered in [answers_re_q25](../answers_re_q25.md).

No behaviour was changed in this pass. These are places where the code and the numbered specs disagree, or where code
has no spec section behind it. Each one has a `// SPEC-GAP:` at the given place.

## Code vs spec (decide: fix the code, or fix the spec)

1. **Key-repeat phase** (`input.rs` `Input::tick`). 15 §3.3 tests the unchanged-input counter before incrementing it
   (`counter > 29` → repeat = `(counter & 7) == 0`; reset to 30 above 37), so the first repeat comes on the 33rd
   unchanged frame. The poller in the port increments first and fires on the 32nd. The replay emulation in `replay.rs`
   follows 15 §3.3. Does the Supervisor's poller (12 intro, 11 §7) use exactly the 15 §3.3 rule? This affects every
   held key in the menus, the name entry and the pause menu.
2. **Replay stage scores** (`replay.rs` `record_stage_start`). 15 §3.2 sets only the previous stage's record +0 to
   the current score. The port writes the current score into every earlier record, so a saved multi-stage replay
   shows the same LastScore for all stages but the last, and playback of a later stage takes the wrong start score
   (11 §3.4). Fix the code?
3. **Spell-practice difficulty** (`menulist.rs` screen 17 confirm). 16 §6.2 sets difficulty = `Spell_DifficultyOf(id)`.
   The port uses CATK +0x0F (the last attempt's difficulty), or Extra for row 8. For a never-attempted card this is 0
   (Easy). Fix to 16 §6.2?
4. **Option volume steps** (`menu.rs` option). 15 §1.3: an edge steps ±4 then ±1 (±5 per press); a held key adds ±1
   per frame once the counter is > 29. The port steps ±1 per press and ±4 on repeat frames, and the SE 0x1D volume
   preview every 50 frames is missing. Fix to 15 §1.3?
5. **Back from a practice game** (`menu.rs` on_register). 12 §2.2 forwards 8 → 9 → 11 (cursor = g_StageIdx) or
   15 → 16 → 17 on consecutive frames. The port stays on screen 8 (practice), or jumps straight to 17 (spell practice).
6. **Screen entry on the press frame** (`menulist.rs`: practice 11, screens 16 ↔ 17). The Menu_SetScreen entry rule
   (12 §2) says the new handler first runs on the next calc. The port builds and draws these screens on the press
   frame because the mopup1 draw log shows them there. Which is right for these screens?
7. **Replay stage row** (`menulist.rs` replay sub-state 2). The row starts at 0 and snaps to the first recorded stage
   on the next frame (mopup1 log). 15 §3.8 does not say where the stage cursor starts. What is the rule?
8. **Spell-practice replay flags** (`replay.rs` `start_playback`). 15 §3.8 sets only flags bit 14 for a spell id ≥ 0;
   the port also sets bit 0 (as the menu's 0x4001). Does replay playback of a spell-practice replay set bit 0?
9. **Screen 16 details** (`menulist.rs`). 16 §6.1: a shot change zeroes the screen frame counter (the pie easing
   restarts), and Bomb stores the row into g_StageIdx. The port does neither.
10. **Title music after the result viewer** (`menu.rs` on_register). 00 §3.2 does not start `th08_01` when the
    previous scene was 5. The port always starts it.
11. **Last-difficulty clamp** (`menu.rs` `last_difficulty`). 12 §2.2 item 0 says "clamp ≤ 3 → 2", 12 §2.3 says
    "> 3 → 1". Which one? (The port uses > 3 → 1.)
12. **Name entry, two directions in one frame** (`result.rs` name entry). 16 §2.2 runs Up, Down, Left, Right in that
    order, so Up+Down both move. The port does only Up (or Left).
13. **resulttext.anm slot** (`menulist.rs` `RT_SLOT`). 00 §3.2 / §4 load it into slot 22 for the menu; the port's
    menu uses 21 (result00.anm's slot). No visible effect, since the two scenes never coexist.
14. **Archive edge cases** (`archive.rs`). 01 §6 step 4 uses data without a known edz wrapper as-is (the port fails),
    and 01 §4 rejects count 0 / table offset ≥ file size (not checked). No shipped file is affected.
15. **Funcchain on-register failure** (`task.rs` `add`). 00 §3 only says a non-zero on-register result is returned.
    Is the node left linked?
16. **Screen effects registered mid-chain** (`game.rs` `calc`). ScreenEffect_Register (13 §3) links its nodes at once;
    the port links them after the calc chain. An effect registered by a calc node below prio 3 (Supervisor, ASCII,
    GM) would run its first calc on the same frame in the original. Which callers register from below prio 3?

## Spec errors

- 15 §2.1 cites "01 §3" for the archive block cipher and "01 §4" for LZSS; they are 01 §2 and 01 §3.

## Uncited code (no spec section)

- `archive.rs`: the LZSS encoder (only decoder compatibility is specified; used for score.dat and replays).
- `input.rs`: the keys for arrows, X (bomb) and Shift (focus); the specs name only Z, Ctrl, ESC, Enter, Q, S,
  Home/P, D, R.
- `audio.rs`: the Music_Fade curve (linear here).
- `sound.rs`: out-of-range SE ids (e.g. sht +0x26 = −1) are ignored.
- `supervisor.rs`: boot / menu loading holds measured on the draw log (12 §1 has no fixed time); the playback fps text
  refresh phase (15 §3.3 says it cannot be reproduced).
- `supervisor.rs`: the date is UTC (the original uses local time).
- `menu.rs`: "fade music 30" (12 §2.2 item 8) has no unit; 30 frames assumed.
- `menu.rs`: delayed screen switch placed at press + 21 (draw log) for "after 20 frames".
- `menu.rs`: the gauge readout limits are reset on menu load.
- `menu.rs`, `menulist.rs`: input start frame 8 on the character, practice, screen 16 / 17 and replay screens; the
  option-screen cancel; the replay screen's 30-frame leave delay and cursor 4.
- `replay.rs`: whether fast playback and the dialogue skip share one Replay_FastForward call.
- `config.rs`: the effect level of a replay's config copy is clamped to 2.

## Not implemented (spec exists)

- th08.cfg load/save at boot and exit (15 §1.2), the thbgm.dat probe for the default music mode, pad buttons /
  thresholds / frame skip / ShotSlow / debug flags (15 §1.1), joystick input (15 §1.1), key config (15 §1.3),
  option return 5 and the 3600-frame idle exit.
- MIDI music mode and music off (02 §4).
- Frame skip (00 §2), the Supervisor +0x154 counter and the slow-mode input accumulation (11 §7); 6 → −1 replay
  finish (11 §7).
- score.dat: VRSM exe check (15 §2.10); boot loads the whole file instead of PLST only (15 §2.2).
- Replays: exe check on load (15 §3.7), USER section (15 §3.5), Replay_FrameHook (11 §1.4), 0x164D0B0 in the
  record, info +0x11C / +0x7E, the save dialogue and its skip tests (16 §2.6, 15 §3.6), the fps meter (15 §3.2).
- Result: hidden unlock code (15 §2.12), score.txt (16 §2.7).
- Menu: sound flushes after confirm sfx (12 §2.2, 16 §6.2).
- Ending skip flags (16 §1.2) in the ending stub.
